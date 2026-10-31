//! Revalidated queue and exact core postimages share one consumed writer.
use super::super::{
    admission::{table_error, total},
    publication,
    store::RedbStateTransaction,
};
use super::{
    plan::{self, ReleasePublicationPlan},
    read,
};
use wf_domain::{
    governance::release::{ReleaseIntentRef, ReleaseOutboxState, append_release_outbox},
    workflow::{WorkflowRecord, request::RequestScope},
};
use wf_ports::state::{GroupApplyChanges, StateTransactionError as E};
use wf_types::identity::PartitionId;

pub(in crate::state::redb) struct VerifiedReleaseWrites<'plan> {
    records: Vec<(&'plan PartitionId, &'plan WorkflowRecord)>,
}
impl VerifiedReleaseWrites<'_> {
    pub(in crate::state::redb) fn permits(
        &self,
        partition: &PartitionId,
        record: &WorkflowRecord,
    ) -> bool {
        let key = (partition.as_bytes(), record.id().as_bytes());
        self.records
            .binary_search_by(|(p, r)| (p.as_bytes(), r.id().as_bytes()).cmp(&key))
            .is_ok_and(|index| std::ptr::eq(self.records[index].1, record))
    }
}

pub(in crate::state::redb) fn validate<'a>(
    tx: &RedbStateTransaction<'_>,
    changes: &'a GroupApplyChanges<'_, '_>,
    release: &ReleasePublicationPlan,
) -> Result<VerifiedReleaseWrites<'a>, E> {
    let scope = changes.scope();
    let context = read::require(tx, scope)?;
    if Some(release.entry) != changes.plan().control().next().last_entry()
        || plan::capacity(changes.plan()) > tx.admission.batch_items()
        || release.sorted.len() != release.entries.len()
    {
        return Err(E::InvalidChanges);
    }
    tx.temporary(total(
        std::mem::size_of::<VerifiedReleaseWrites<'_>>() + release.entries.len(),
        [plan::capacity(changes.plan())
            .checked_mul(std::mem::size_of::<(&PartitionId, &WorkflowRecord)>())
            .ok_or(E::Limit)?],
    )?)?;
    let mut records = Vec::new();
    records
        .try_reserve_exact(plan::capacity(changes.plan()))
        .map_err(|_| E::Allocation)?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    let mut seen = Vec::new();
    seen.try_reserve_exact(release.entries.len())
        .map_err(|_| E::Allocation)?;
    seen.resize(release.entries.len(), false);
    let mut count = 0;
    plan::walk(tx, changes.plan(), |item| {
        records.push((
            item.partition(),
            &item.plan().write().ok_or(E::InvalidChanges)?.record,
        ));
        plan::with_binding(tx, &table, scope, item, context, |intent| {
            if item.plan().write().ok_or(E::InvalidChanges)?.record.state() == b"running" {
                return Ok(());
            }
            let index =
                plan::locate(release, intent.reservation_id)?.map_err(|_| E::InvalidChanges)?;
            if seen[index] {
                return Ok(());
            }
            if index != count {
                return Err(E::InvalidChanges);
            }
            let encoded = &release.entries[index];
            let stored = wf_ports::log::inspect_release_intent(encoded, encoded.len())
                .map_err(read::codec_error)?;
            if !same_intent(
                stored,
                ReleaseIntentRef {
                    created_at: item
                        .plan()
                        .write()
                        .ok_or(E::InvalidChanges)?
                        .record
                        .updated_at(),
                    ..intent
                },
            ) {
                return Err(E::InvalidChanges);
            }
            seen[index] = true;
            count += 1;
            Ok(())
        })
    })?;
    if count != release.entries.len() {
        return Err(E::InvalidChanges);
    }
    match release.append {
        None if count == 0 => {}
        Some((expected, next)) if count > 0 => {
            let actual =
                read::meta(tx, &table, scope)?.unwrap_or(ReleaseOutboxState { head: 1, tail: 0 });
            if actual != expected {
                return Err(E::KnownAborted);
            }
            if append_release_outbox(actual, u64::try_from(count).map_err(|_| E::Limit)?)
                .map_err(|_| E::InvalidChanges)?
                != next
            {
                return Err(E::InvalidChanges);
            }
        }
        _ => return Err(E::InvalidChanges),
    }
    records.sort_unstable_by(|(ap, ar), (bp, br)| {
        (ap.as_bytes(), ar.id().as_bytes()).cmp(&(bp.as_bytes(), br.id().as_bytes()))
    });
    if records
        .windows(2)
        .any(|pair| pair[0].0 == pair[1].0 && pair[0].1.id() == pair[1].1.id())
    {
        return Err(E::InvalidChanges);
    }
    Ok(VerifiedReleaseWrites { records })
}

fn same_intent(a: ReleaseIntentRef<'_>, b: ReleaseIntentRef<'_>) -> bool {
    a.flow_id == b.flow_id
        && a.partition == b.partition
        && a.scope == b.scope
        && a.shard_id == b.shard_id
        && a.reservation_id == b.reservation_id
        && a.enforcement == b.enforcement
        && a.created_at == b.created_at
}

pub(in crate::state::redb) fn publish(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    release: &ReleasePublicationPlan,
) -> Result<(), E> {
    let Some((old, next)) = release.append else {
        return Ok(());
    };
    for (offset, entry) in release.entries.iter().enumerate() {
        let sequence = old.tail + u64::try_from(offset).map_err(|_| E::Limit)? + 1;
        let key = read::queue_key(tx, scope, 22, Some(sequence))?;
        tx.step()?;
        {
            let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
            if read::read(tx, &table, &key, |_| Ok(()))?.is_some()
                || read::read(
                    tx,
                    &table,
                    &read::queue_key(tx, scope, 23, Some(sequence))?,
                    |_| Ok(()),
                )?
                .is_some()
            {
                return Err(E::Corrupt);
            }
        }
        publication::put(tx, &key, entry)?;
    }
    tx.temporary(22)?;
    let bytes = wf_ports::log::encode_release_meta(next, 22).map_err(read::codec_error)?;
    publication::put(tx, &read::queue_key(tx, scope, 21, None)?, &bytes)
}
