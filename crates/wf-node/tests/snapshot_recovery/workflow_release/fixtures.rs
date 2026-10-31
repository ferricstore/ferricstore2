//! Actual GC4/5 databases and independent full-table and original-ID checks.
use super::*;
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
use wf_domain::workflow::{
    active::LifecycleDefaults, control::PreparedDigest, request::RetainedStatus,
};
use wf_ports::{
    consensus::WorkflowValueDefaults,
    log::{PreparedBytes, RaftLogLimits, RaftLogStore},
    state::{GroupApplyStore, GroupApplyTransaction},
};
use wf_storage::{
    log::RedbLogStore,
    state::{RedbStateStore, StateStoreGenesis, StateStoreLimits},
    transaction::{
        ClaimTransactionOutcome, RecordBatchTransactionOutcome, WorkflowValueTransactionLimits,
        apply_credit_transaction, apply_governed_claim_transaction,
        apply_release_maintenance_transaction, apply_workflow_release_transaction,
    },
};
use wf_types::identity::{PartitionId, RecordId, RequestId};

pub(super) fn selected(paid: bool) -> TestResult<CommittedRecoveryLimits> {
    let limits = stored()?.with_identified_credit_catalog_limits(4, CreditLimits::default())?;
    Ok(if paid {
        limits
            .with_workflow_value_claim_limits(
                ClaimWorkLimits::default(),
                1024,
                64 * f::MIB,
                64 * f::MIB,
                32 * f::MIB,
            )?
            .with_governed_claim_recovery()?
    } else {
        limits.with_workflow_release_recovery()?
    })
}
pub(super) fn defaults() -> WorkflowValueDefaults {
    let lifecycle = LifecycleDefaults {
        ttl_ms: 7000,
        history_hot_max_events: 2,
        history_max_events: 10,
        max_history_hot_max_events: 100,
        max_history_max_events: 1000,
    };
    let mut hash = Sha256::new();
    hash.update(b"wf-workflow-value-defaults-v1");
    hash.update(lifecycle.ttl_ms.to_be_bytes());
    hash.update(lifecycle.history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.history_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_max_events.to_be_bytes());
    WorkflowValueDefaults {
        configuration_digest: PreparedDigest::new(hash.finalize().into()),
        lifecycle,
    }
}
pub(super) fn log(dir: &Path) -> TestResult<RedbLogStore> {
    let s = f::scope()?;
    Ok(RedbLogStore::create(
        &dir.join("log.redb"),
        s.group().clone(),
        s.recovery().clone(),
        RaftLogLimits::default(),
    )?)
}
pub(super) fn pair(dir: &Path, paid: bool) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let s = f::scope()?;
    let placements = [(PartitionId::new(b"p".to_vec())?, s.group().clone())];
    let genesis = StateStoreGenesis {
        initial_time: f::clock(0, 0, 0)?,
        placements: &placements,
        policies: &[],
    };
    let db = redb::Builder::new().create(dir.join("state.redb"))?;
    let create = if paid {
        RedbStateStore::create_governed_claim_database
    } else {
        RedbStateStore::create_workflow_release_database
    };
    let state = create(
        db,
        s.group().clone(),
        s.recovery().clone(),
        genesis,
        defaults(),
        4,
        StateStoreLimits::default(),
    )?;
    Ok((log(dir)?, state))
}
pub(super) fn values() -> TestResult<WorkflowValueTransactionLimits> {
    selected(true)?
        .workflow_stored_value_limits()
        .ok_or_else(|| "stored limits".into())
}
pub(super) fn append(
    log: &mut RedbLogStore,
    frames: &[PreparedBytes],
    corrupt: Option<usize>,
) -> TestResult {
    for (index, frame) in frames.iter().enumerate() {
        let mut body = frame.as_bytes().to_vec();
        if corrupt == Some(index) {
            body.push(0);
        }
        f::append(log, u64::try_from(index)?, &body)?;
    }
    let end = u64::try_from(frames.len())?;
    f::append(log, end, b"opaque uncommitted release/paid Claim suffix")?;
    log.save_committed(f::at(end - 1))?;
    Ok(())
}
pub(super) fn apply(
    state: &mut RedbStateStore,
    frames: &[PreparedBytes],
    end: usize,
) -> TestResult {
    let s = f::scope()?;
    for (index, frame) in frames.iter().take(end).enumerate() {
        let at = f::at(u64::try_from(index)?);
        let outcome = match frame.as_bytes().get(6) {
            Some(17) => {
                let result = apply_governed_claim_transaction(
                    state,
                    &s,
                    frame.as_bytes(),
                    at,
                    values()?,
                    ClaimWorkLimits::default(),
                )?;
                let ClaimTransactionOutcome::Retained { outcome, .. } = result else {
                    return Err("Claim result".into());
                };
                assert_eq!(outcome.status(), RetainedStatus::Accepted);
                continue;
            }
            Some(10) => apply_credit_transaction(
                state,
                &s,
                frame.as_bytes(),
                at,
                f::driver_limits(),
                CreditLimits::default(),
            )?,
            Some(16) => apply_release_maintenance_transaction(
                state,
                &s,
                frame.as_bytes(),
                at,
                f::driver_limits(),
            )?,
            _ => apply_workflow_release_transaction(state, &s, frame.as_bytes(), at, values()?)?,
        };
        let RecordBatchTransactionOutcome::Retained { outcome, .. } = outcome else {
            return Err("native result".into());
        };
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
    }
    Ok(())
}
pub(super) fn record(
    state: &mut RedbStateStore,
    id: &[u8],
) -> TestResult<wf_ports::state::StoredWorkflowRecord> {
    let s = f::scope()?;
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        f::driver_limits().codec.admission(),
        10000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    tx.record(
        s.tenant(),
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(id.to_vec())?,
    )?
    .ok_or_else(|| "record".into())
}
pub(super) fn check(state: &mut RedbStateStore) -> TestResult {
    let s = f::scope()?;
    assert_eq!(record(state, b"i")?.record.state(), b"cancelled");
    assert_eq!(record(state, b"j")?.record.state(), b"running");
    let page = state.read_workflow_release_outbox(&s, None, 256, 10000, 16 * f::MIB)?;
    assert_eq!(
        (page.meta.head, page.meta.tail, page.entries.len()),
        (2, 1, 0)
    );
    let owner = state
        .read_credit_owner(&s, b"global", CreditLimits::default(), 10000, 16 * f::MIB)?
        .ok_or("owner")?;
    assert_eq!(owner.view().leases[0].in_use, 1);
    let tx = state.begin_apply(
        s.group(),
        s.recovery(),
        f::driver_limits().codec.admission(),
        10000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let claim = tx
        .outcome(s.tenant(), &RequestId::new(b"claim".to_vec())?)?
        .ok_or("receipt")?;
    assert_eq!(
        (claim.intent().version(), claim.response_version()),
        (19, 16)
    );
    let reply = wf_ports::log::inspect_governed_claim_reply(
        claim.response(),
        claim.original_time(),
        f::driver_limits().codec.admission(),
    )?;
    assert_eq!(
        reply
            .payment()
            .reservation_ids()
            .collect::<Result<Vec<_>, _>>()?,
        vec![b"a\0".as_slice(), b"b".as_slice()]
    );
    let ack = tx
        .outcome(s.tenant(), &RequestId::new(b"ack".to_vec())?)?
        .ok_or("ack")?;
    assert_eq!(
        (
            ack.intent().version(),
            ack.response_version(),
            ack.original_position()
        ),
        (18, 15, f::at(8))
    );
    assert_eq!(tx.control()?.position(), f::at(8).index());
    Ok(())
}
pub(super) fn cohort(dir: &Path) -> TestResult<BTreeMap<String, f::Rows>> {
    let db = redb::Builder::new().open(dir.join("state.redb"))?;
    let tx = db.begin_read()?;
    assert_eq!(tx.list_multimap_tables()?.count(), 0);
    let mut out = BTreeMap::new();
    for handle in tx.list_tables()? {
        let name = handle.name();
        let table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        let mut rows = BTreeMap::new();
        for row in table.iter()? {
            let (k, v) = row?;
            assert!(
                rows.insert(k.value().to_vec(), v.value().to_vec())
                    .is_none()
            );
        }
        assert!(out.insert(name.to_owned(), rows).is_none());
    }
    assert_eq!(out.len(), 6);
    Ok(out)
}
