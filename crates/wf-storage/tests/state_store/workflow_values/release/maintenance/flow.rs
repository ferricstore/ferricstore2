//! Real paid work, terminal outbox, restart/snapshot and exact credit-release loop.
use super::*;
use crate::workflow_values::claim::fixtures as c;
use wf_domain::governance::{
    credit::CreditLimits,
    identified::{CreditConfiguration, IdentifiedCreditMutation as M, IdentifiedCreditRequest},
    release::{ReleaseDisposition, ReleaseRecordObservation, classify_release},
};
use wf_ports::{
    consensus::{PreparedClaimPhase, PreparedClaimQuery},
    log::{
        GovernedClaimPayment, PreparedBytes, encode_credit_request, encode_governed_claim_request,
    },
    state::{GroupApplyStore, GroupApplyTransaction},
};
use wf_storage::transaction::{
    ClaimTransactionOutcome, apply_credit_transaction, apply_governed_claim_transaction,
    apply_workflow_release_transaction,
};
use wf_types::identity::RecordId;

fn credit_frame(
    request: &[u8],
    now: u64,
    configuration: Option<CreditConfiguration>,
    mutation: M<'_>,
) -> TestResult<PreparedBytes> {
    Ok(encode_credit_request(
        &crate::fixtures::scope()?,
        &RequestId::new(request.to_vec())?,
        crate::fixtures::clock(now)?,
        IdentifiedCreditRequest {
            scope: b"global",
            shard_count: 4,
            configuration,
            mutation,
        },
        f::limits().record_batch.codec,
        16 * 1024 * 1024,
    )?)
}
fn credit(
    store: &mut RedbStateStore,
    frame: &PreparedBytes,
    index: u64,
) -> TestResult<Box<RetainedOutcome>> {
    let result = apply_credit_transaction(
        store,
        &crate::fixtures::scope()?,
        frame.as_bytes(),
        crate::fixtures::log(index),
        f::limits().record_batch,
        CreditLimits::default(),
    )?;
    let O::Retained { outcome, .. } = result else {
        return Err("credit receipt".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    Ok(outcome)
}
fn paid(path: &Path) -> TestResult<RedbStateStore> {
    let scope = crate::fixtures::scope()?;
    let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
    let mut store = RedbStateStore::create_governed_claim_database(
        redb::Builder::new().create(path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: crate::fixtures::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        f::defaults(),
        4,
        StateStoreLimits::default(),
    )?;
    c::create(&mut store, b"i", b"q", (0, 100, 0), Some(b"payload"))?;
    c::create(&mut store, b"j", b"q", (1, 101, 0), Some(b"sibling"))?;
    credit(
        &mut store,
        &credit_frame(
            b"lease",
            102,
            Some(CreditConfiguration {
                limit: 3,
                config_version: Some(1),
                policy_fingerprint: None,
            }),
            M::Lease {
                shard_id: 0,
                amount: 3,
                ttl_ms: 1000,
            },
        )?,
        2,
    )?;
    credit(
        &mut store,
        &credit_frame(
            b"spend",
            103,
            None,
            M::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 2,
                ttl_ms: None,
                reservation_ids: &[b"a\0".to_vec(), b"b".to_vec()],
            },
        )?,
        3,
    )?;
    let frame = encode_governed_claim_request(
        &scope,
        &RequestId::new(b"claim".to_vec())?,
        crate::fixtures::clock(104)?,
        &PreparedClaimPhase {
            query: PreparedClaimQuery {
                workflow_type: b"r".to_vec(),
                states: Some(vec![b"q".to_vec()]),
                exclude_states: vec![],
                partitions: Some(vec![b"p".to_vec()]),
                priority: None,
                worker: b"w".to_vec(),
                lease_ms: 30000,
                limit: 2,
            },
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 1000,
            head_budget: 1000,
        },
        GovernedClaimPayment {
            scope: b"global",
            shard_id: 0,
            epoch: 1,
            enforcement: LimitEnforcement::StrictGlobal,
            reservation_ids: &[b"a\0".to_vec(), b"b".to_vec()],
        },
        f::limits().record_batch.codec,
        f::limits().record_batch.workspace_bytes,
    )?;
    let result = apply_governed_claim_transaction(
        &mut store,
        &scope,
        frame.as_bytes(),
        crate::fixtures::log(4),
        f::limits(),
        wf_domain::claim::apply::ClaimWorkLimits::default(),
    )?;
    let ClaimTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("paid Claim".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    Ok(store)
}
fn terminal(store: &mut RedbStateStore) -> TestResult {
    use wf_domain::workflow::Cancel;
    use wf_ports::log::encode_workflow_stored_value_batch;
    let record = c::record(store, b"i")?.record;
    let command = f::batch(
        5,
        b"cancel",
        105,
        PreparedOperation::Batch(vec![f::item(
            b"i",
            105,
            Operation::Cancel(Cancel {
                fence: record.fence(),
                lease_token: record.lease().map(|lease| lease.token.clone()),
                expected_state: Some(b"running".to_vec()),
                expected_version: Some(record.version()),
                error: None,
            }),
        )?]),
        vec![f::input(0, 0)],
        vec![f::witness(b"r", None)?],
    )?;
    let frame = encode_workflow_stored_value_batch(&command, f::limits().record_batch.codec)?;
    let outcome = apply_workflow_release_transaction(
        store,
        &crate::fixtures::scope()?,
        frame.as_bytes(),
        crate::fixtures::log(5),
        f::limits(),
    )?;
    assert_eq!(
        c::retained_record(outcome)?.status(),
        RetainedStatus::Accepted
    );
    Ok(())
}

fn terminal_release_frame(store: &mut RedbStateStore) -> TestResult<PreparedBytes> {
    let scope = crate::fixtures::scope()?;
    let page = store.read_workflow_release_outbox(&scope, None, 256, 10000, 1024 * 1024)?;
    assert_eq!(page.entries.len(), 1);
    let pending = page.entries[0].intent()?;
    assert_eq!(pending.reservation_id, b"a\0");
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        f::limits().record_batch.codec.admission(),
        10000,
        1024 * 1024,
        1024 * 1024,
    )?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"i".to_vec())?;
    let current = tx
        .record(scope.tenant(), &partition, &id)?
        .ok_or("terminal")?;
    let (_, binding) = tx
        .workflow_governance_binding(&scope, &partition, &id)?
        .ok_or("binding")?;
    assert_eq!(
        classify_release(
            pending.reservation_id,
            ReleaseRecordObservation::Record {
                state: Some(current.record.state()),
                reservation_id: Some(&binding.reservation_id)
            }
        ),
        ReleaseDisposition::Release
    );
    drop(tx);
    credit_frame(
        b"release",
        106,
        None,
        M::Release {
            shard_id: pending.shard_id,
            amount: 1,
            reservation_ids: &[pending.reservation_id.to_vec()],
        },
    )
}

#[test]
fn actual_paid_claim_restart_snapshot_exact_release_and_ack_preserve_running_sibling() -> TestResult
{
    use std::io::Cursor;
    use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
    use wf_storage::snapshot::RedbSnapshotStage;
    let dir = crate::fixtures::directory("release-maintenance-real-loop")?;
    let path = dir.join("state");
    let target = dir.join("received");
    let mut store = paid(&path)?;
    terminal(&mut store)?;
    drop(store);
    let mut restarted = open(&path)?;
    let mut wire = Vec::new();
    restarted.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop(restarted);
    let scope = crate::fixtures::scope()?;
    let stage = RedbSnapshotStage::receive(
        &target,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let mut restored = stage.into_store()?;
    let frame = terminal_release_frame(&mut restored)?;
    let original = credit(&mut restored, &frame, 6)?;
    // A successful credit result leaves durable outbox progress pending until marked/ACKed.
    assert!(
        !restored
            .read_workflow_release_outbox(&scope, None, 256, 10000, 1024 * 1024)?
            .entries[0]
            .completed
    );
    apply(
        &mut restored,
        b"mark",
        &C::MarkCompleted { sequences: vec![1] },
        7,
    )?;
    let acknowledged = apply(
        &mut restored,
        b"ack",
        &C::Acknowledge {
            expected_head: 1,
            up_to: 1,
        },
        8,
    )?;
    assert_eq!(
        reply(&acknowledged)?,
        R::Accepted {
            meta: ReleaseOutboxState { head: 2, tail: 1 },
            affected: 1
        }
    );
    assert_eq!(credit(&mut restored, &frame, 9)?, original);
    assert_eq!(
        apply(
            &mut restored,
            b"ack",
            &C::Acknowledge {
                expected_head: 1,
                up_to: 1
            },
            10
        )?,
        acknowledged
    );
    assert_eq!(
        restored
            .read_workflow_release_outbox(&scope, None, 256, 10000, 1024 * 1024)?
            .entries
            .len(),
        0
    );
    let owner = restored
        .read_credit_owner(
            &scope,
            b"global",
            CreditLimits::default(),
            10000,
            16 * 1024 * 1024,
        )?
        .ok_or("owner")?;
    assert_eq!(owner.view().leases[0].in_use, 1);
    assert_eq!(c::record(&mut restored, b"j")?.record.state(), b"running");
    drop(restored);
    drop(open(&target)?);
    assert_released_and_held(&target)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn credit_key(family: u8, id: &[u8]) -> TestResult<Vec<u8>> {
    let scope = crate::fixtures::scope()?;
    let mut out = b"FWPK\0\x01".to_vec();
    out.push(family);
    for field in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        b"global",
        &0_u64.to_be_bytes(),
        &1_u64.to_be_bytes(),
        id,
    ] {
        for byte in field {
            out.push(*byte);
            if *byte == 0 {
                out.push(255);
            }
        }
        out.extend_from_slice(&[0, 0]);
    }
    Ok(out)
}

fn assert_released_and_held(path: &Path) -> TestResult {
    let rows = crate::fixtures::raw_rows(path)?;
    assert!(rows.contains_key(&credit_key(24, b"b")?));
    assert!(!rows.contains_key(&credit_key(24, b"a\0")?));
    assert_eq!(
        rows.get(&credit_key(13, b"a\0")?).and_then(|v| v.last()),
        Some(&1)
    );
    assert_eq!(
        rows.get(&credit_key(13, b"b")?).and_then(|v| v.last()),
        Some(&0)
    );
    assert_eq!(rows.keys().filter(|k| k.get(6) == Some(&24)).count(), 1);
    assert_eq!(
        rows.keys()
            .filter(|k| matches!(k.get(6), Some(22 | 23)))
            .count(),
        0
    );
    Ok(())
}
