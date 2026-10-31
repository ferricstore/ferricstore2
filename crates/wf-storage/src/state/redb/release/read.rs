//! One coherent revision, bounded physical probes and prospectively paid owners.
use super::super::{
    admission::{copy_bytes, storage_error, table_error, total, value_error, wire_record_limit},
    store::RedbStateTransaction,
};
use crate::state::{key, value};
use redb::ReadableTable;
use wf_domain::{
    governance::release::{GovernanceReservation, ReleaseIntentRef, ReleaseOutboxState},
    workflow::request::RequestScope,
};
use wf_ports::{
    log::{
        ReleaseBindingRef, inspect_release_binding, inspect_release_intent, inspect_release_meta,
    },
    state::{
        GroupApplyTransaction, StateTransactionError as E, WorkflowReleaseEntry,
        WorkflowReleasePage,
    },
};
use wf_types::{
    identity::{PartitionId, RecordId},
    version::TimestampMillis,
};

pub(super) fn require(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
) -> Result<(u64, TimestampMillis), E> {
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(E::NamespaceConflict);
    }
    tx.require_workflow_release()?;
    let shards = tx.require_identified_credit_profile()?.1;
    Ok((shards, tx.control()?.time().hlc().physical()))
}

pub(super) fn queue_key(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    family: u8,
    sequence: Option<u64>,
) -> Result<Vec<u8>, E> {
    tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
        ],
        if sequence.is_some() { 8 } else { 0 },
        |cap| key::release::queue(scope, family, sequence, cap),
    )
}

pub(super) fn read<T>(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &[u8],
    decode: impl FnOnce(&[u8]) -> Result<T, E>,
) -> Result<Option<T>, E> {
    tx.step()?;
    let Some(row) = table.get(key).map_err(|e| storage_error(&e))? else {
        return Ok(None);
    };
    tx.recovery_row(key, row.value())?;
    tx.admit_read(total(64, [row.value().len()])?)?;
    decode(row.value()).map(Some)
}

pub(super) fn meta(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
) -> Result<Option<ReleaseOutboxState>, E> {
    read(tx, table, &queue_key(tx, scope, 21, None)?, inspect_meta)
}
pub(super) fn inspect_meta(bytes: &[u8]) -> Result<ReleaseOutboxState, E> {
    let meta = inspect_release_meta(bytes, bytes.len()).map_err(codec_error)?;
    if meta.tail + 1 - meta.head > 65_536 {
        return Err(E::Corrupt);
    }
    Ok(meta)
}
pub(super) fn intent(
    bytes: &[u8],
    (shards, now): (u64, TimestampMillis),
) -> Result<ReleaseIntentRef<'_>, E> {
    let intent = inspect_release_intent(bytes, bytes.len()).map_err(codec_error)?;
    if intent.shard_id >= shards || intent.created_at > now {
        return Err(E::Corrupt);
    }
    Ok(intent)
}
pub(super) fn completion(bytes: &[u8]) -> Result<(), E> {
    if bytes != b"FWRC\0\x01" {
        return Err(E::Corrupt);
    }
    Ok(())
}

pub(super) fn page(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    after: Option<u64>,
    limit: u16,
) -> Result<WorkflowReleasePage, E> {
    if !(1..=256).contains(&limit)
        || after.is_some_and(|n| n > wf_domain::workflow::MAX_EXACT_MILLIS)
    {
        return Err(E::InvalidChanges);
    }
    let context = require(tx, scope)?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    let meta = meta(tx, &table, scope)?.unwrap_or(ReleaseOutboxState { head: 1, tail: 0 });
    let start = after.map_or(meta.head, |n| (n + 1).max(meta.head));
    let count = usize::try_from((meta.tail + 1).saturating_sub(start).min(u64::from(limit)))
        .map_err(|_| E::Limit)?;
    tx.admit_read(total(
        std::mem::size_of::<WorkflowReleasePage>(),
        [count
            .checked_mul(std::mem::size_of::<WorkflowReleaseEntry>())
            .ok_or(E::Limit)?],
    )?)?;
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(count)
        .map_err(|_| E::Allocation)?;
    for offset in 0..count {
        let sequence = start + u64::try_from(offset).map_err(|_| E::Limit)?;
        let encoded = read(
            tx,
            &table,
            &queue_key(tx, scope, 22, Some(sequence))?,
            |bytes| {
                intent(bytes, context)?;
                tx.admit_read(bytes.len())?;
                copy_bytes(bytes)
            },
        )?
        .ok_or(E::Corrupt)?;
        let completed = read(
            tx,
            &table,
            &queue_key(tx, scope, 23, Some(sequence))?,
            completion,
        )?
        .is_some();
        entries.push(WorkflowReleaseEntry::new(sequence, completed, encoded).map_err(codec_error)?);
    }
    Ok(WorkflowReleasePage {
        meta,
        more: start + u64::try_from(count).map_err(|_| E::Limit)? <= meta.tail,
        entries,
    })
}

pub(super) fn check_binding(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    (partition, id): (&PartitionId, &RecordId),
    binding: ReleaseBindingRef<'_>,
    context: (u64, TimestampMillis),
) -> Result<(), E> {
    let original = binding.intent;
    if original.flow_id != id.as_bytes()
        || original.partition != Some(partition.as_bytes())
        || original.shard_id >= context.0
        || original.created_at > context.1
    {
        return Err(E::Corrupt);
    }
    let key = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            partition.as_bytes(),
            id.as_bytes(),
        ],
        0,
        |cap| key::encode_record_key(scope, partition, id, cap),
    )?;
    read(tx, table, &key, |bytes| {
        let record = value::scan_record_in_layout(
            bytes,
            scope,
            partition,
            id,
            tx.admission,
            wire_record_limit(tx.admission)?,
            // GC4 is admitted only with layout8; startup has no cached layout yet.
            8,
        )
        .map_err(value_error)?;
        if original.created_at > record.updated_at() {
            return Err(E::Corrupt);
        }
        Ok(())
    })?
    .ok_or(E::Corrupt)
}

pub(super) fn binding(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
) -> Result<Option<(u64, GovernanceReservation)>, E> {
    let context = require(tx, scope)?;
    let key = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            partition.as_bytes(),
            id.as_bytes(),
        ],
        0,
        |cap| key::release::binding(scope, partition.as_bytes(), id.as_bytes(), cap),
    )?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    read(tx, &table, &key, |bytes| {
        let binding = inspect_release_binding(bytes, bytes.len()).map_err(codec_error)?;
        check_binding(tx, &table, scope, (partition, id), binding, context)?;
        let original = binding.intent;
        tx.admit_read(total(
            128,
            [original.scope.len(), original.reservation_id.len()],
        )?)?;
        Ok((
            binding.epoch,
            GovernanceReservation {
                scope: copy_bytes(original.scope)?,
                shard_id: original.shard_id,
                reservation_id: copy_bytes(original.reservation_id)?,
                enforcement: original.enforcement,
            },
        ))
    })
}

pub(super) fn codec_error(error: wf_ports::log::CodecError) -> E {
    match error {
        wf_ports::log::CodecError::Allocation => E::Allocation,
        wf_ports::log::CodecError::ByteLimit
        | wf_ports::log::CodecError::OwnedLimit
        | wf_ports::log::CodecError::WorkLimit
        | wf_ports::log::CodecError::ItemLimit => E::Limit,
        _ => E::Corrupt,
    }
}

pub(super) fn refuse_legacy_write(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
) -> Result<(), E> {
    require(tx, scope)?;
    let key = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            partition.as_bytes(),
            id.as_bytes(),
        ],
        0,
        |cap| key::release::binding(scope, partition.as_bytes(), id.as_bytes(), cap),
    )?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    if read(tx, &table, &key, |bytes| {
        inspect_release_binding(bytes, bytes.len())
            .map_err(codec_error)
            .map(|_| ())
    })?
    .is_some()
    {
        return Err(E::UnsupportedPolicyProfile);
    }
    Ok(())
}
