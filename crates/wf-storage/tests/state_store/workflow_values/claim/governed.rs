//! Real paid IDs flow through consumed Claim publication, retention and release.
use super::*;
use std::path::Path;
mod custody;
use wf_domain::governance::{
    credit::CreditLimits,
    identified::{CreditConfiguration, IdentifiedCreditMutation as M, IdentifiedCreditRequest},
    release::LimitEnforcement,
};
use wf_ports::{
    consensus::{PreparedClaimPhase, PreparedClaimQuery},
    log::{
        GovernedClaimPayment, encode_credit_request, encode_governed_claim_request,
        inspect_governed_claim_reply,
    },
    state::{GroupApplyStore, GroupApplyTransaction},
};
use wf_storage::{
    state::{StateStoreGenesis, StateStoreLimits},
    transaction::{apply_credit_transaction, apply_governed_claim_transaction},
};
use wf_types::identity::{PartitionId, RecordId, RequestId};

fn paid(path: &Path) -> TestResult<RedbStateStore> {
    let scope = f::scope()?;
    let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
    let mut store = RedbStateStore::create_governed_claim_database(
        redb::Builder::new().create(path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        v::defaults(),
        4,
        StateStoreLimits::default(),
    )?;
    fixtures::create(&mut store, b"i", b"q", (0, 1000, 0), Some(b"payload"))?;
    for (index, request, configuration, mutation) in [
        (
            1,
            b"lease".as_slice(),
            Some(CreditConfiguration {
                limit: 3,
                config_version: Some(1),
                policy_fingerprint: None,
            }),
            M::Lease {
                shard_id: 0,
                amount: 3,
                ttl_ms: 100_000,
            },
        ),
        (
            2,
            b"spend",
            None,
            M::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 2,
                ttl_ms: None,
                reservation_ids: &[b"a".to_vec(), b"b".to_vec()],
            },
        ),
    ] {
        let frame = encode_credit_request(
            &scope,
            &RequestId::new(request.to_vec())?,
            f::clock(1000 + index)?,
            IdentifiedCreditRequest {
                scope: b"global",
                shard_count: 4,
                configuration,
                mutation,
            },
            v::limits().record_batch.codec,
            16 * 1024 * 1024,
        )?;
        assert_eq!(
            fixtures::retained_record(apply_credit_transaction(
                &mut store,
                &scope,
                frame.as_bytes(),
                f::log(index),
                v::limits().record_batch,
                CreditLimits::default()
            )?)?
            .status(),
            RetainedStatus::Accepted
        );
    }
    Ok(store)
}

fn frame(request: &[u8], now: u64, ids: &[Vec<u8>]) -> TestResult<PreparedBytes> {
    frame_with(request, now, ids, 1, b"w", 1)
}

fn frame_with(
    request: &[u8],
    now: u64,
    ids: &[Vec<u8>],
    epoch: u64,
    worker: &[u8],
    limit: usize,
) -> TestResult<PreparedBytes> {
    Ok(encode_governed_claim_request(
        &f::scope()?,
        &RequestId::new(request.to_vec())?,
        f::clock(now)?,
        &PreparedClaimPhase {
            query: PreparedClaimQuery {
                workflow_type: b"r".to_vec(),
                states: Some(vec![b"q".to_vec()]),
                exclude_states: vec![],
                partitions: Some(vec![b"p".to_vec()]),
                priority: None,
                worker: worker.to_vec(),
                lease_ms: 30000,
                limit,
            },
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 1000,
            head_budget: 1000,
        },
        GovernedClaimPayment {
            scope: b"global",
            shard_id: 0,
            epoch,
            enforcement: LimitEnforcement::StrictGlobal,
            reservation_ids: ids,
        },
        v::limits().record_batch.codec,
        v::limits().record_batch.workspace_bytes,
    )?)
}

#[test]
fn paid_claim_binds_original_used_id_and_replays_before_new_payment_reads() -> TestResult {
    let dir = f::directory("governed-claim-original")?;
    let path = dir.join("state");
    let mut store = paid(&path)?;
    let scope = f::scope()?;
    let prepared = frame(b"claim", 1003, &[b"a".to_vec(), b"b".to_vec()])?;
    let applied = apply_governed_claim_transaction(
        &mut store,
        &scope,
        prepared.as_bytes(),
        f::log(3),
        v::limits(),
        ClaimWorkLimits::default(),
    )?;
    let ClaimTransactionOutcome::Retained { outcome, .. } = applied else {
        return Err("claim receipt".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    let decoded_response = inspect_governed_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    assert_eq!(decoded_response.used(), 1);
    assert_eq!(
        decoded_response
            .payment()
            .reservation_ids()
            .collect::<Result<Vec<_>, _>>()?,
        [b"a".as_slice(), b"b".as_slice()]
    );
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        v::limits().record_batch.codec.admission(),
        10000,
        1024 * 1024,
        1024 * 1024,
    )?;
    let (epoch, binding) = tx
        .workflow_governance_binding(
            &scope,
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?,
        )?
        .ok_or("binding")?;
    assert_eq!(epoch, 1);
    assert_eq!(binding.reservation_id, b"a");
    assert_eq!(
        tx.record(
            scope.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"i".to_vec())?
        )?
        .ok_or("record")?
        .record
        .state(),
        b"running"
    );
    drop(tx);
    let retry = frame(b"claim", 1004, &[b"unknown-original".to_vec()])?;
    let replay = apply_governed_claim_transaction(
        &mut store,
        &scope,
        retry.as_bytes(),
        f::log(4),
        v::limits(),
        ClaimWorkLimits::default(),
    )?;
    let ClaimTransactionOutcome::Retained {
        outcome: replay, ..
    } = replay
    else {
        return Err("replay".into());
    };
    assert_eq!(outcome, replay);
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn apply(
    store: &mut RedbStateStore,
    prepared: &PreparedBytes,
    at: u64,
) -> TestResult<Box<RetainedOutcome>> {
    let result = apply_governed_claim_transaction(
        store,
        &f::scope()?,
        prepared.as_bytes(),
        f::log(at),
        v::limits(),
        ClaimWorkLimits::default(),
    )?;
    let ClaimTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("paid receipt".into());
    };
    Ok(outcome)
}

fn cancel(store: &mut RedbStateStore, id: &[u8], index: u64) -> TestResult {
    use wf_domain::workflow::{Cancel, Operation};
    use wf_ports::{consensus::PreparedOperation, log::encode_workflow_stored_value_batch};
    use wf_storage::transaction::apply_workflow_release_transaction;
    let record = fixtures::record(store, id)?.record;
    let item = v::item(
        id,
        1000 + index,
        Operation::Cancel(Cancel {
            fence: record.fence(),
            lease_token: record.lease().map(|lease| lease.token.clone()),
            expected_state: Some(record.state().to_vec()),
            expected_version: Some(record.version()),
            error: None,
        }),
    )?;
    let prepared = v::batch(
        5,
        b"cancel",
        1000 + index,
        PreparedOperation::Batch(vec![item]),
        vec![v::input(0, 0)],
        vec![v::witness(b"r", None)?],
    )?;
    let encoded = encode_workflow_stored_value_batch(&prepared, v::limits().record_batch.codec)?;
    let result = apply_workflow_release_transaction(
        store,
        &f::scope()?,
        encoded.as_bytes(),
        f::log(index),
        v::limits(),
    )?;
    assert_eq!(
        fixtures::retained_record(result)?.status(),
        RetainedStatus::Accepted
    );
    Ok(())
}

#[test]
fn paid_claim_reuse_refuses_and_unused_sibling_claim_reaches_pending_release() -> TestResult {
    use wf_domain::claim::apply::ClaimRejection as R;
    let dir = f::directory("governed-claim-siblings")?;
    let path = dir.join("state");
    let mut store = paid(&path)?;
    assert_eq!(
        apply(
            &mut store,
            &frame(b"claim-i", 1003, &[b"a".to_vec(), b"b".to_vec()])?,
            3
        )?
        .status(),
        RetainedStatus::Accepted
    );
    fixtures::create(&mut store, b"j", b"q", (4, 1004, 0), None)?;
    let refused = apply(
        &mut store,
        &frame(b"reuse", 1005, &[b"a".to_vec(), b"b".to_vec()])?,
        5,
    )?;
    assert_eq!(
        refused.status(),
        RetainedStatus::ClaimRejected(R::GoverningReservationUnavailable)
    );
    let decoded_response = inspect_governed_claim_reply(
        refused.response(),
        refused.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    assert_eq!(decoded_response.used(), 0);
    assert_eq!(decoded_response.payment().count(), 2);
    assert_eq!(fixtures::record(&mut store, b"j")?.record.state(), b"q");
    assert_eq!(
        apply(&mut store, &frame(b"claim-j", 1006, &[b"b".to_vec()])?, 6)?.status(),
        RetainedStatus::Accepted
    );
    cancel(&mut store, b"i", 7)?;
    let scope = f::scope()?;
    let page = store.read_workflow_release_outbox(&scope, None, 256, 10000, 1024 * 1024)?;
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].intent()?.reservation_id, b"a");
    assert_eq!(page.entries[0].intent()?.created_at.get(), 1007);
    let owner = store
        .read_credit_owner(
            &scope,
            b"global",
            CreditLimits::default(),
            10000,
            16 * 1024 * 1024,
        )?
        .ok_or("owner")?;
    assert_eq!(
        (
            owner.view().leases[0].available,
            owner.view().leases[0].in_use
        ),
        (1, 2)
    );
    assert_eq!(
        fixtures::record(&mut store, b"j")?.record.state(),
        b"running"
    );
    drop(store);
    drop(f::open(&path)?);
    let rows = f::raw_rows(&path)?;
    assert_eq!(rows.keys().filter(|key| key.get(6) == Some(&24)).count(), 2);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn paid_claim_refuses_missing_expired_wrong_epoch_and_insufficient_original_ids() -> TestResult {
    use wf_domain::claim::apply::ClaimRejection as R;
    for (label, now, epoch, ids, limit, worker, expected) in [
        (
            "missing",
            1003,
            1,
            vec![b"missing".to_vec()],
            1,
            b"w".as_slice(),
            R::GoverningReservationUnavailable,
        ),
        (
            "epoch",
            1003,
            2,
            vec![b"a".to_vec()],
            1,
            b"w",
            R::GoverningReservationUnavailable,
        ),
        (
            "expired",
            101_001,
            1,
            vec![b"a".to_vec()],
            1,
            b"w",
            R::GoverningReservationUnavailable,
        ),
        (
            "insufficient",
            1003,
            1,
            vec![b"a".to_vec(), b"b".to_vec()],
            3,
            b"w",
            R::GoverningReservationUnavailable,
        ),
        (
            "query-first",
            1003,
            1,
            vec![b"missing".to_vec()],
            1,
            b"",
            R::EmptyWorker,
        ),
    ] {
        let dir = f::directory(label)?;
        let path = dir.join("state");
        let mut store = paid(&path)?;
        let original = fixtures::record(&mut store, b"i")?;
        let response = apply(
            &mut store,
            &frame_with(b"claim", now, &ids, epoch, worker, limit)?,
            3,
        )?;
        assert_eq!(
            response.status(),
            RetainedStatus::ClaimRejected(expected),
            "{label}"
        );
        assert_eq!(fixtures::record(&mut store, b"i")?, original);
        assert_eq!(
            store
                .read_workflow_release_outbox(&f::scope()?, None, 256, 10000, 1024 * 1024)?
                .entries
                .len(),
            0
        );
        drop(store);
        drop(f::open(&path)?);
        assert_eq!(
            f::raw_rows(&path)?
                .keys()
                .filter(|key| key.get(6) == Some(&24))
                .count(),
            0
        );
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn paid_claim_snapshot_preserves_original_reply_binding_and_pending_release() -> TestResult {
    use std::io::Cursor;
    use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
    use wf_storage::snapshot::RedbSnapshotStage;
    let dir = f::directory("governed-claim-snapshot")?;
    let path = dir.join("state");
    let target = dir.join("received");
    let mut store = paid(&path)?;
    let original = apply(
        &mut store,
        &frame(b"claim", 1003, &[b"a".to_vec(), b"b".to_vec()])?,
        3,
    )?;
    cancel(&mut store, b"i", 4)?;
    let scope = f::scope()?;
    let mut wire = Vec::new();
    store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    drop(store);
    let stage = RedbSnapshotStage::receive(
        &target,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let mut restored = stage.into_store()?;
    let replay = apply(
        &mut restored,
        &frame(b"claim", 1005, &[b"missing".to_vec()])?,
        5,
    )?;
    assert_eq!(original, replay);
    assert_eq!(
        restored
            .read_workflow_release_outbox(&scope, None, 256, 10000, 1024 * 1024)?
            .entries[0]
            .intent()?
            .reservation_id,
        b"a"
    );
    assert_eq!(
        fixtures::record(&mut restored, b"i")?.record.state(),
        b"cancelled"
    );
    drop(restored);
    drop(f::open(&target)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn installed_running_governance_refuses_unpaid_leaf_and_accepts_real_paid_capture() -> TestResult {
    use wf_domain::policy::complete::RawFlowPolicyValue as Raw;
    use wf_storage::transaction::ClaimTransactionError;
    let dir = f::directory("paid-claim-installed-policy")?;
    let path = dir.join("state");
    let mut store = paid(&path)?;
    let rule = [
        (Raw::Atom("limit"), Raw::Integer(1)),
        (Raw::Atom("enforcement"), Raw::Atom("strict_global")),
    ];
    let running = [(Raw::Binary(b"running"), Raw::Map(&rule))];
    let governance = [(Raw::Atom("limits"), Raw::Map(&running))];
    let attrs = [(Raw::Atom("governance"), Raw::Map(&governance))];
    fixtures::policy(&mut store, 3, 1003, Raw::Map(&attrs))?;
    let legacy = fixtures::phase(b"unpaid", 1004, b"q", b"w", 1)?;
    assert_eq!(
        apply_workflow_value_claim_transaction(
            &mut store,
            &f::scope()?,
            legacy.as_bytes(),
            f::log(4),
            v::limits(),
            ClaimWorkLimits::default()
        )
        .err(),
        Some(ClaimTransactionError::UnsupportedGovernance)
    );
    assert_eq!(fixtures::record(&mut store, b"i")?.record.state(), b"q");
    assert_eq!(
        apply(&mut store, &frame(b"paid", 1004, &[b"a".to_vec()])?, 4)?.status(),
        RetainedStatus::Accepted
    );
    assert_eq!(
        fixtures::record(&mut store, b"i")?.record.state(),
        b"running"
    );
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
