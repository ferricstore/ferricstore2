//! Bidirectional canonical and policy-resolved authority under one open table.

use super::super::{
    admission::{entry, storage_error, total, value_error},
    store::RedbStateTransaction,
};
use super::rows::{decode_policy, lookup_record};
use crate::state::key::{self, DecodedStateKey};
use redb::ReadableTable;
use wf_domain::claim::{DueOrder, LaneKey, LaneOrder};
use wf_domain::policy::complete::PolicySchemaCharge;
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{StateTransactionError, StoredWorkflowRecord};
use wf_types::version::TimestampMillis;

fn mode(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    owner: &StoredWorkflowRecord,
    state: &[u8],
    charge: &mut PolicySchemaCharge,
    layout: u16,
) -> Result<StateMode, StateTransactionError> {
    let scope = tx.scope(&owner.tenant)?;
    let key = tx.key(
        &[
            tx.group.as_bytes(),
            tx.recovery.as_bytes(),
            owner.tenant.as_bytes(),
            owner.record.workflow_type(),
        ],
        0,
        |cap| key::encode_policy_key(&scope, owner.record.workflow_type(), cap),
    )?;
    tx.step()?;
    let guard = table.get(key.as_slice()).map_err(|e| storage_error(&e))?;
    let Some(guard) = guard else {
        return Ok(StateMode::Parallel);
    };
    tx.recovery_row(&key, guard.value())?;
    let policy = decode_policy(
        tx,
        guard.value(),
        owner.record.workflow_type(),
        charge,
        layout,
    )?;
    match policy {
        wf_ports::state::StoredPolicyValue::Complete(policy) => Ok(policy
            .data()
            .states
            .get(state)
            .and_then(|p| p.mode)
            .unwrap_or(StateMode::Parallel)),
        wf_ports::state::StoredPolicyValue::Original(raw) => {
            let schema = tx.limits.schema();
            tx.temporary(super::super::original_policy::original_owner(raw.len())?)?;
            let bytes = wf_domain::policy::complete::original_record_batch_policy_projection_bytes(
                &raw,
                owner.record.workflow_type(),
                schema,
                charge,
            )
            .map_err(super::super::original_policy::original_error)?;
            tx.temporary(
                super::super::original_policy::original_owner(raw.len())?
                    .checked_add(bytes)
                    .ok_or(StateTransactionError::Limit)?,
            )?;
            let (policy, _) = wf_domain::policy::complete::project_original_record_batch_policy(
                &raw,
                owner.record.workflow_type(),
                schema,
                charge,
                bytes,
            )
            .map_err(super::super::original_policy::original_error)?;
            Ok(policy.state_mode(state))
        }
    }
}

pub(super) fn canonical(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    owner: &StoredWorkflowRecord,
    charge: &mut PolicySchemaCharge,
    layout: u16,
) -> Result<(), StateTransactionError> {
    let placement = tx.key(
        &[tx.recovery.as_bytes(), owner.partition.as_bytes()],
        0,
        |cap| key::encode_placement_key(tx.recovery, &owner.partition, cap),
    )?;
    exact(tx, table, &placement, Some(tx.group.as_bytes()))?;
    let row = entry(owner);
    let Some(lane) = LaneKey::for_record(&row) else {
        return Ok(());
    };
    let fifo = mode(tx, table, owner, lane.state, charge, layout)? == StateMode::Fifo;
    let scope = tx.scope(&owner.tenant)?;
    let member = tx.fifo_key(&scope, lane, LaneOrder::for_record(&row))?;
    exact(tx, table, &member, if fifo { Some(&[]) } else { None })?;
    if let Some(due) = DueOrder::for_record(&row) {
        let first = if fifo {
            head(tx, table, &scope, lane)?.as_ref() == Some(owner)
        } else {
            false
        };
        let key = tx.due_key(&scope, due)?;
        exact(
            tx,
            table,
            &key,
            if !fifo {
                Some(&[0])
            } else if first {
                Some(&[1])
            } else {
                None
            },
        )?;
    }
    Ok(())
}

fn exact(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &[u8],
    expected: Option<&[u8]>,
) -> Result<(), StateTransactionError> {
    tx.step()?;
    let guard = table.get(key).map_err(|e| storage_error(&e))?;
    if let Some(guard) = &guard {
        tx.recovery_row(key, guard.value())?;
    }
    if guard.as_ref().map(redb::AccessGuard::value) != expected {
        return Err(StateTransactionError::Corrupt);
    }
    Ok(())
}

fn head(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    lane: LaneKey<'_>,
) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
    let prefix = tx.key(
        &[
            tx.group.as_bytes(),
            tx.recovery.as_bytes(),
            scope.tenant().as_bytes(),
            lane.workflow_type,
            lane.state,
            lane.partition_key,
        ],
        0,
        |cap| key::encode_fifo_prefix(scope, lane, cap),
    )?;
    tx.temporary(prefix.len())?;
    tx.step()?;
    let mut entries = table
        .range(prefix.as_slice()..)
        .map_err(|e| storage_error(&e))?;
    tx.step()?;
    let Some(row) = entries.next() else {
        return Ok(None);
    };
    let (physical, bytes) = row.map_err(|e| storage_error(&e))?;
    tx.recovery_row(physical.value(), bytes.value())?;
    if !physical.value().starts_with(&prefix) {
        return Ok(None);
    }
    if !bytes.value().is_empty() {
        return Err(StateTransactionError::Corrupt);
    }
    let maximum = total(128, [physical.value().len()])?;
    tx.temporary(maximum)?;
    let decoded = key::decode_fifo_key(
        physical.value(),
        scope,
        lane,
        physical.value().len(),
        maximum,
    )
    .map_err(value_error)?;
    let (member_score, sequence, id) = decoded.member().ok_or(StateTransactionError::Corrupt)?;
    let owner = lookup_record(tx, table, scope.tenant().as_bytes(), lane.partition_key, id)?
        .ok_or(StateTransactionError::Corrupt)?;
    let row = entry(&owner);
    let member = LaneOrder::for_record(&row);
    if LaneKey::for_record(&row) != Some(lane)
        || (member.score(), member.sequence(), member.id()) != (member_score, sequence, id)
    {
        return Err(StateTransactionError::Corrupt);
    }
    Ok(Some(owner))
}

pub(super) fn reverse(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    key: &DecodedStateKey,
    bytes: &[u8],
    charge: &mut PolicySchemaCharge,
    layout: u16,
) -> Result<(), StateTransactionError> {
    let scope = tx.scope_bytes(key.part(2))?;
    let (partition, id) = if key.family() == 3 {
        (key.part(4), key.part(3))
    } else {
        (key.part(5), key.part(6))
    };
    let owner = lookup_record(tx, table, scope.tenant().as_bytes(), partition, id)?
        .ok_or(StateTransactionError::Corrupt)?;
    let row = entry(&owner);
    let lane = LaneKey::for_record(&row).ok_or(StateTransactionError::Corrupt)?;
    let fifo = mode(tx, table, &owner, lane.state, charge, layout)? == StateMode::Fifo;
    if key.family() == 3 {
        let due = DueOrder {
            priority: 2 - key.rank(),
            due: TimestampMillis::new(
                u64::try_from(key.number()).map_err(|_| StateTransactionError::Corrupt)?,
            ),
            id: key.part(3),
            partition_key: key.part(4),
            state: key.part(5),
            workflow_type: key.part(6),
        };
        if DueOrder::for_record(&row) != Some(due) || bytes != if fifo { &[1] } else { &[0] } {
            return Err(StateTransactionError::Corrupt);
        }
        if fifo && head(tx, table, &scope, lane)?.as_ref() != Some(&owner) {
            return Err(StateTransactionError::Corrupt);
        }
    } else {
        let member = LaneOrder::for_record(&row);
        if !fifo
            || !bytes.is_empty()
            || (lane.workflow_type, lane.state, lane.partition_key)
                != (key.part(3), key.part(4), key.part(5))
            || (member.score(), member.sequence(), member.id())
                != (
                    if key.rank() == 0 { -1 } else { 0 },
                    key.number(),
                    key.part(6),
                )
        {
            return Err(StateTransactionError::Corrupt);
        }
    }
    Ok(())
}
