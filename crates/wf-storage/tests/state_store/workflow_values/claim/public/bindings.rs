//! Supplemental restored-core controls; no original source-case coverage credit.
use super::super::fixtures as leaf;
use super::{fixtures as p, *};
use std::path::Path;
use wf_domain::claim::{
    apply::ClaimRejection,
    caller::{ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput},
};
use wf_domain::policy::complete::RawFlowPolicyValue as Raw;
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_ports::log::{inspect_public_claim_reply, running_claim_scope};
use wf_ports::state::GroupApplyStore;
use wf_storage::{state::value, transaction::apply_public_claim_transaction};
use wf_types::identity::{PartitionId, RecordId};

fn limited_frame(reclaim: bool, limit: u64) -> TestResult<PreparedBytes> {
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", b"new");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(limit);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(reclaim);
    p::encode(input, p::BATCH, 100)
}

fn with_second(path: &Path) -> TestResult<RedbStateStore> {
    let mut store = p::filled(path, false)?;
    leaf::create(&mut store, b"q2", b"q", (5, 2400, 1), None)?;
    let rule = [
        (Raw::Atom("limit"), Raw::Integer(8)),
        (Raw::Atom("lease_size"), Raw::Integer(6)),
    ];
    let running = [(Raw::Binary(b"running"), Raw::Map(&rule))];
    let governance = [(Raw::Atom("limits"), Raw::Map(&running))];
    let attrs = [
        (Raw::Atom("version"), Raw::Integer(17)),
        (Raw::Atom("governance"), Raw::Map(&governance)),
    ];
    leaf::policy(&mut store, 6, 2500, Raw::Map(&attrs))?;
    Ok(store)
}

fn assert_binding(store: &mut RedbStateStore, id: &[u8], expected: Option<&[u8]>) -> TestResult {
    let scope = f::scope()?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        v::limits().record_batch.codec.admission(),
        10000,
        1_048_576,
        1_048_576,
    )?;
    let actual = tx.workflow_governance_binding(
        &scope,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(id.to_vec())?,
    )?;
    assert_eq!(
        actual
            .as_ref()
            .map(|(_, binding)| binding.reservation_id.as_slice()),
        expected
    );
    Ok(())
}

fn restore_attempts(
    path: &Path,
    mut store: RedbStateStore,
    id: &[u8],
    attempts: u64,
) -> TestResult<RedbStateStore> {
    let scope = f::scope()?;
    let mut owner = leaf::record(&mut store, id)?;
    let record = &owner.record;
    let latest = record.latest_event_witness();
    owner.record = restore_record(
        WorkflowRecordData {
            id: record.id().clone(),
            workflow_type: record.workflow_type().to_vec(),
            state: record.state().to_vec(),
            version: record.version(),
            fence: record.fence(),
            attempts,
            created_at: record.created_at(),
            updated_at: record.updated_at(),
            next_run_at: record.next_run_at(),
            run_state: record.run_state().map(<[u8]>::to_vec),
            lease: record.lease().cloned(),
            payload: record.payload().map(<[u8]>::to_vec),
            result: record.result().map(<[u8]>::to_vec),
            error: record.error().map(<[u8]>::to_vec),
            latest_kind: latest.0,
            latest_version: latest.1,
            latest_fence: latest.2,
        },
        v::limits().record_batch.codec.admission(),
    )?;
    let key = wf_storage::state::key::encode_record_key(
        &scope,
        &owner.partition,
        owner.record.id(),
        1_048_576,
    )?;
    let wire = value::encode_record(
        scope.group(),
        scope.recovery(),
        &owner,
        v::limits().record_batch.codec.admission(),
        1_048_576,
    )?;
    drop(store);
    let db = redb::Database::open(path)?;
    let write = db.begin_write()?;
    {
        let mut rows = write.open_table(redb::TableDefinition::<&[u8], &[u8]>::new(
            "wf_state_rows_v1",
        ))?;
        rows.insert(key.as_slice(), wire.as_slice())?;
    }
    write.commit()?;
    drop(db);
    f::open(path)
}

#[test]
fn source_query_integer_failure_skips_the_actual_eligible_record() -> TestResult {
    let dir = f::directory("public-bind-source-integer")?;
    let path = dir.join("state");
    let original = p::filled(&path, false)?;
    let mut store = restore_attempts(&path, original, b"q1", u64::MAX)?;
    let before = leaf::record(&mut store, b"q1")?;
    let frame = p::frame(false, p::BATCH)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(5),
        p::limits()?,
    )?)?;
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    assert_eq!(reply.claim().ok_or("claim")?.count(), 0);
    assert_eq!(leaf::record(&mut store, b"q1")?, before);
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn largest_source_integer_remains_claimable() -> TestResult {
    let dir = f::directory("public-bind-largest-source-integer")?;
    let path = dir.join("state");
    let original = p::filled(&path, false)?;
    let mut store = restore_attempts(&path, original, b"q1", u64::try_from(i64::MAX)?)?;
    let frame = p::frame(false, p::BATCH)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(5),
        p::limits()?,
    )?)?;
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    assert_eq!(reply.claim().ok_or("claim")?.count(), 1);
    assert_eq!(
        leaf::record(&mut store, b"q1")?.record.attempts(),
        u64::try_from(i64::MAX)?
    );
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn private_atomic_profile_keeps_its_full_u64_record_contract() -> TestResult {
    let dir = f::directory("public-bind-private-unchanged")?;
    let path = dir.join("state");
    let original = p::filled(&path, false)?;
    let mut store = restore_attempts(&path, original, b"q1", u64::MAX)?;
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", b"new");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(4);
    input.lease_ms = Some(100);
    let caller = wf_domain::claim::caller::normalize_claim_due(
        input,
        wf_domain::claim::caller::ClaimCallerLimits::default(),
    )?;
    let frame = wf_ports::log::encode_atomic_claim_request(
        &f::scope()?,
        &wf_types::identity::RequestId::new(b"private-parent".to_vec())?,
        f::clock(3000)?,
        &caller,
        wf_ports::log::AtomicClaimOptions {
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 100,
            head_budget: 100,
            payment: None,
        },
        v::limits().record_batch.codec,
        v::limits().record_batch.workspace_bytes,
    )?;
    let outcome = leaf::retained(wf_storage::transaction::apply_atomic_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(5),
        p::limits()?.base,
    )?)?;
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(leaf::record(&mut store, b"q1")?.record.version(), 2);
    assert_eq!(leaf::record(&mut store, b"q1")?.record.attempts(), u64::MAX);
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn a_skipped_bind_refunds_original_zero_and_binds_the_next_original_id() -> TestResult {
    let dir = f::directory("public-bind-nonprefix")?;
    let path = dir.join("state");
    let original = with_second(&path)?;
    let mut store = restore_attempts(&path, original, b"q1", u64::MAX)?;
    let before = leaf::record(&mut store, b"q1")?;
    let frame = p::frame(false, p::BATCH)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(7),
        p::limits()?,
    )?)?;
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    let claim = reply.claim().ok_or("claim")?;
    assert_eq!(claim.count(), 1);
    assert_eq!(claim.used_indices().collect::<Vec<_>>(), [1]);
    let original_ids = claim
        .payment()
        .ok_or("payment")?
        .reservation_ids()
        .collect::<Result<Vec<_>, _>>()?;
    assert_binding(&mut store, b"q1", None)?;
    assert_binding(&mut store, b"q2", Some(original_ids[1]))?;
    assert_eq!(leaf::record(&mut store, b"q1")?, before);
    assert_eq!(leaf::record(&mut store, b"q2")?.record.version(), 2);
    let owner = store
        .read_credit_owner(
            &f::scope()?,
            &running_claim_scope(b"r"),
            p::limits()?.base.credit,
            10000,
            16 * 1_048_576,
        )?
        .ok_or("owner")?;
    assert_eq!(
        (
            owner.lease(2).ok_or("lease")?.available,
            owner.lease(2).ok_or("lease")?.in_use
        ),
        (5, 1)
    );
    drop(store);
    let mut reopened = f::open(&path)?;
    assert_eq!(
        leaf::retained(apply_public_claim_transaction(
            &mut reopened,
            &f::scope()?,
            frame.as_bytes(),
            f::log(8),
            p::limits()?,
        )?)?,
        outcome
    );
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn exactly_exhausted_skipped_batch_can_complete_only_at_actual_stream_end() -> TestResult {
    let dir = f::directory("public-bind-empty-at-boundary")?;
    let path = dir.join("state");
    let original = p::filled(&path, true)?;
    let mut store = restore_attempts(&path, original, b"q1", u64::MAX)?;
    let frame = limited_frame(false, 1)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(6),
        p::limits()?,
    )?)?;
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    let claim = reply.claim().ok_or("claim")?;
    assert_eq!((claim.count(), claim.phase_count()), (0, 1));
    assert_eq!(claim.status(), RetainedStatus::Accepted);
    assert_eq!(claim.payment().ok_or("payment")?.count(), 1);
    assert!(claim.used_indices().next().is_none());
    let owner = store
        .read_credit_owner(
            &f::scope()?,
            &running_claim_scope(b"r"),
            p::limits()?.base.credit,
            10000,
            16 * 1_048_576,
        )?
        .ok_or("owner")?;
    assert_eq!(
        (
            owner.lease(2).ok_or("lease")?.available,
            owner.lease(2).ok_or("lease")?.in_use
        ),
        (6, 0)
    );
    assert_binding(&mut store, b"q1", None)?;
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn another_eligible_bind_after_exhaustion_rejects_and_refunds_the_whole_parent() -> TestResult {
    let dir = f::directory("public-bind-refuse-unpaid")?;
    let path = dir.join("state");
    let original = with_second(&path)?;
    let mut store = restore_attempts(&path, original, b"q1", u64::MAX)?;
    let before = [
        leaf::record(&mut store, b"q1")?,
        leaf::record(&mut store, b"q2")?,
    ];
    let frame = limited_frame(false, 1)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(7),
        p::limits()?,
    )?)?;
    assert_eq!(
        outcome.status(),
        RetainedStatus::ClaimRejected(ClaimRejection::GoverningReservationUnavailable)
    );
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    let claim = reply.claim().ok_or("claim")?;
    assert_eq!((claim.count(), claim.phase_count()), (0, 0));
    assert!(claim.used_indices().next().is_none());
    assert_eq!(
        [
            leaf::record(&mut store, b"q1")?,
            leaf::record(&mut store, b"q2")?
        ],
        before
    );
    let owner = store
        .read_credit_owner(
            &f::scope()?,
            &running_claim_scope(b"r"),
            p::limits()?.base.credit,
            10000,
            16 * 1_048_576,
        )?
        .ok_or("owner")?;
    assert_eq!(
        (
            owner.lease(2).ok_or("lease")?.available,
            owner.lease(2).ok_or("lease")?.in_use
        ),
        (6, 0)
    );
    assert_binding(&mut store, b"q1", None)?;
    assert_binding(&mut store, b"q2", None)?;
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn skipped_running_candidate_cannot_reset_the_cursor_in_a_later_phase() -> TestResult {
    let dir = f::directory("public-bind-phase-exhaustion")?;
    let path = dir.join("state");
    let original = p::filled(&path, true)?;
    let mut store = restore_attempts(&path, original, b"r1", u64::MAX)?;
    let mut before = Vec::new();
    for id in [b"r1", b"r2", b"r3", b"q1"] {
        before.push(leaf::record(&mut store, id)?);
    }
    let frame = p::frame(true, p::BATCH)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(6),
        p::limits()?,
    )?)?;
    assert_eq!(
        outcome.status(),
        RetainedStatus::ClaimRejected(ClaimRejection::GoverningReservationUnavailable)
    );
    for (id, original) in [b"r1", b"r2", b"r3", b"q1"].into_iter().zip(before) {
        assert_eq!(leaf::record(&mut store, id)?, original);
        assert_binding(&mut store, id, None)?;
    }
    let owner = store
        .read_credit_owner(
            &f::scope()?,
            &running_claim_scope(b"r"),
            p::limits()?.base.credit,
            10000,
            16 * 1_048_576,
        )?
        .ok_or("owner")?;
    assert_eq!(
        (
            owner.lease(2).ok_or("lease")?.available,
            owner.lease(2).ok_or("lease")?.in_use
        ),
        (6, 0)
    );
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn a_phase_with_no_accepted_postimage_still_consumes_its_bind_positions() -> TestResult {
    let dir = f::directory("public-bind-all-skipped-phase")?;
    let path = dir.join("state");
    let mut store = p::filled(&path, true)?;
    for id in [b"r1", b"r2", b"r3"] {
        store = restore_attempts(&path, store, id, u64::MAX)?;
    }
    let queued = leaf::record(&mut store, b"q1")?;
    let frame = p::frame(true, p::BATCH)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(6),
        p::limits()?,
    )?)?;
    assert_eq!(
        outcome.status(),
        RetainedStatus::ClaimRejected(ClaimRejection::GoverningReservationUnavailable)
    );
    assert_eq!(leaf::record(&mut store, b"q1")?, queued);
    for id in [b"r1", b"r2", b"r3", b"q1"] {
        assert_binding(&mut store, id, None)?;
    }
    let owner = store
        .read_credit_owner(
            &f::scope()?,
            &running_claim_scope(b"r"),
            p::limits()?.base.credit,
            10000,
            16 * 1_048_576,
        )?
        .ok_or("owner")?;
    assert_eq!(
        (
            owner.lease(2).ok_or("lease")?.available,
            owner.lease(2).ok_or("lease")?.in_use
        ),
        (6, 0)
    );
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
