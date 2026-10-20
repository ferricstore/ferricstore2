//! Real legacy3/5 bases and original6 DATA share immutable origins and suffixes.

use super::{
    CommittedRecoveryLimits, SnapshotLimits, StateSnapshotStore, TestResult, f, purge_snapshot,
    recover_snapshotted,
};
use redb::{ReadableDatabase, TableHandle};
use sha2::{Digest, Sha256};
use std::path::Path;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput, OriginalPolicySetFrameInput,
    OriginalPolicySetMapOrders, PolicySchemaCharge, RawFlowPolicyValue as Raw,
    prepare_original_policy_set_request,
};
use wf_domain::policy::definition::PolicyPatch;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, EntryStamp};
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::log::{
    RaftLogStore, prepare_complete_core_policy_patch_frame, prepare_original_policy_set_frame,
};
use wf_ports::snapshot::StateSnapshotTransitionStore;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StoredPolicyValue};
use wf_storage::log::RedbLogStore;
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::{
    RecordBatchTransactionLimits, RecordBatchTransactionOutcome,
    apply_complete_core_policy_patch_transaction, apply_original_policy_set_transaction,
};
use wf_types::identity::RequestId;

mod received;

fn normal() -> RecordBatchTransactionLimits {
    let mut limits = f::driver_limits();
    limits.workspace_bytes = 512 * f::MIB;
    limits
}

fn caps() -> TestResult<CommittedRecoveryLimits> {
    Ok(CommittedRecoveryLimits::new(
        RaftMembershipLimits::default(),
        (10_000, 64 * f::MIB, 64 * f::MIB, 32 * f::MIB),
        normal(),
        (1000, 64 * f::MIB),
        16 * f::MIB,
        1024 * f::MIB,
    )?
    .with_core_policy_patches()
    .with_original_policy_sets())
}

fn expected_original(generation: u8) -> TestResult<Vec<u8>> {
    let mut value = f::hex(
        "836803770e666c6f775f706f6c6963795f763161017400000002770673746174657374000000007704747970656d0000000175",
    )?;
    value[20] = generation;
    Ok(value)
}

fn expected_complete(generation: u64) -> TestResult<Vec<u8>> {
    let mut value = f::hex(
        "46575056000200000000000000070000007446575053000200000001720101ffffffffffffffff010000000757b12c00010100000101000000000000000a000000000000001e0001000000066661696c656401000000000001000000017101010100000001000001000000000000000100000000010000000570686173650100000000000000",
    )?;
    value[6..14].copy_from_slice(&generation.to_be_bytes());
    Ok(value)
}

fn policy(scope: &RequestScope, request: &[u8], now: u64, replace: bool) -> TestResult<Vec<u8>> {
    const PATCH: &[u8] = &[0, 0, 0, 17, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 17];
    let limits = normal();
    let mut charge = PolicySchemaCharge::new(limits.schema);
    let mode = [
        Raw::Atom("replace"),
        if replace {
            Raw::Atom("true")
        } else {
            Raw::Atom("false")
        },
    ];
    let opts = [Raw::Tuple(&mode)];
    let prepared = prepare_original_policy_set_request(
        &Raw::Binary(b"u"),
        &Raw::List(&opts),
        limits.schema,
        &mut charge,
    )?
    .map_err(|_| "original public frame input refused")?;
    let orders = OriginalPolicySetMapOrders::new(
        Sha256::digest(PATCH).into(),
        None,
        &[(0, &[], &[])],
        &[],
        &[],
    );
    Ok(prepare_original_policy_set_frame(
        OriginalPolicySetFrameInput::new(
            scope.clone(),
            RequestId::new(request.to_vec())?,
            f::clock(now, 0, now)?,
            &prepared,
            &orders,
        ),
        limits.schema,
        limits.codec.admission(),
        &mut charge,
    )?
    .as_bytes()
    .to_vec())
}

fn publish(state: &mut RedbStateStore, body: &[u8], position: u64, generation: u8) -> TestResult {
    let limits = normal();
    let mut charge = PolicySchemaCharge::new(limits.schema);
    let RecordBatchTransactionOutcome::Retained { control, outcome } =
        apply_original_policy_set_transaction(
            state,
            &f::scope()?,
            body,
            f::at(position),
            limits,
            &mut charge,
        )?
    else {
        return Err("original SET not retained".into());
    };
    let time = if generation == 1 { 12 } else { 13 };
    assert_eq!(
        control.last_entry().map(EntryIdentity::log),
        Some(f::at(position))
    );
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.response(), expected_original(generation)?);
    assert_eq!(
        (outcome.intent().version(), outcome.response_version()),
        (4, 1)
    );
    assert_eq!(
        (
            outcome.original_position(),
            outcome.original_time().get(),
            outcome.expires_at().get()
        ),
        (f::at(position), time, time + 1000)
    );
    Ok(())
}

fn cohort(dir: &Path) -> TestResult<[Option<f::Rows>; 6]> {
    let names = [
        "wf_state_meta_v1",
        "wf_state_rows_v1",
        "wf_state_snapshot_meta_v1",
        "wf_state_snapshot_rows_v1",
        "wf_state_snapshot_alt_meta_v1",
        "wf_state_snapshot_alt_rows_v1",
    ];
    let db = redb::Builder::new().open(dir.join("state.redb"))?;
    let read = db.begin_read()?;
    let actual = read
        .list_tables()?
        .map(|t| t.name().to_owned())
        .collect::<Vec<_>>();
    assert!(actual.iter().all(|name| names.contains(&name.as_str())));
    assert_eq!(read.list_multimap_tables()?.count(), 0);
    assert!(
        names[..4]
            .iter()
            .all(|name| actual.iter().any(|s| s == name))
    );
    assert_eq!(
        actual.iter().any(|s| s == names[4]),
        actual.iter().any(|s| s == names[5])
    );
    drop(read);
    drop(db);
    let mut result = std::array::from_fn(|_| None);
    for (slot, name) in result.iter_mut().zip(names) {
        if actual.iter().any(|s| s == name) {
            *slot = Some(f::bytes_table(&dir.join("state.redb"), name)?);
        }
    }
    Ok(result)
}

fn artifact(state: &mut RedbStateStore, wire: &[u8]) -> TestResult {
    let id: [u8; 32] = Sha256::digest(wire).into();
    let mut actual = Vec::new();
    let metadata = state
        .snapshot_by_id(id, &mut actual, SnapshotLimits::default())?
        .ok_or("retained immutable ID missing")?;
    assert_eq!(metadata.id(), &id);
    assert_eq!(metadata.encoded_bytes(), wire.len());
    assert_eq!(actual, wire);
    Ok(())
}

fn legacy_base(
    dir: &Path,
    complete: bool,
) -> TestResult<(RedbLogStore, RedbStateStore, Vec<u8>, u64)> {
    let (mut log, mut state) = f::create_pair(dir)?;
    let scope = f::scope()?;
    let member = f::member(EntryStamp::Unstamped, 4)?;
    f::append(&mut log, 0, member.as_bytes())?;
    f::apply_control(&mut state, 0, member.as_bytes())?;
    let position = if complete { 1 } else { 0 };
    if complete {
        apply_complete(&mut log, &mut state)?;
    }
    log.save_committed(f::at(position))?;
    let mut old = Vec::new();
    let id = *state
        .capture_snapshot(&mut old, SnapshotLimits::default())?
        .id();
    assert_eq!(
        &old[..6],
        if complete {
            b"FWSP\0\x02"
        } else {
            b"FWSP\0\x01"
        }
    );
    (log, state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(position)));
    assert_eq!(state.snapshot_pin(SnapshotLimits::default())?, Some(id));
    artifact(&mut state, &old)?;
    Ok((log, state, old, position))
}

fn apply_complete(log: &mut RedbLogStore, state: &mut RedbStateStore) -> TestResult {
    let limits = normal();
    let scope = f::scope()?;
    let body = prepare_complete_core_policy_patch_frame(
        CompleteCorePolicyPatchRequestInput::new(
            scope.clone(),
            RequestId::new(b"core".to_vec())?,
            f::clock(11, 0, 11)?,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: Some(7),
                patch: PolicyPatch::default(),
            },
        ),
        limits.schema,
        limits.codec.admission(),
        &mut PolicySchemaCharge::new(limits.schema),
    )?;
    f::append(log, 1, body.as_bytes())?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } =
        apply_complete_core_policy_patch_transaction(
            state,
            &scope,
            body.as_bytes(),
            f::at(1),
            limits,
        )?
    else {
        return Err("real legacy5 core Patch refused".into());
    };
    assert_eq!(outcome.response(), expected_complete(8)?);
    assert_eq!(outcome.intent().version(), 2);
    Ok(())
}

fn seed(dir: &Path, legacy5: bool, complete: bool) -> TestResult<(Vec<u8>, Vec<u8>, u64)> {
    let (mut log, mut state, old, base) = legacy_base(dir, legacy5)?;
    let scope = f::scope()?;
    let position = base + 1;
    let first = policy(&scope, b"set", 12, false)?;
    f::append(&mut log, position, &first)?;
    publish(&mut state, &first, position, 1)?;
    log.save_committed(f::at(position))?;
    let mut wire = Vec::new();
    let id = *state
        .capture_snapshot(&mut wire, SnapshotLimits::default())?
        .id();
    assert_eq!(&wire[..6], b"FWSP\0\x03");
    assert_ne!(Sha256::digest(&old).as_slice(), id.as_slice());
    artifact(&mut state, &old)?;
    artifact(&mut state, &wire)?;
    assert_eq!(
        state.snapshot_pin(SnapshotLimits::default())?,
        Some(Sha256::digest(&old).into())
    );
    let replace = policy(&scope, b"replace", 13, true)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    f::append(&mut log, position + 1, &replace)?;
    f::append(&mut log, position + 2, blank.as_bytes())?;
    if complete {
        publish(&mut state, &replace, position + 1, 2)?;
        f::apply_control(&mut state, position + 2, blank.as_bytes())?;
    }
    log.save_committed(f::at(position + 2))?;
    f::append(
        &mut log,
        position + 3,
        b"uncommitted original snapshot suffix",
    )?;
    Ok((old, wire, position))
}

fn inspect(state: &mut RedbStateStore, generation: u8, position: u64, legacy5: bool) -> TestResult {
    let scope = f::scope()?;
    let limits = normal();
    let mut charge = PolicySchemaCharge::new(limits.schema);
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        1000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let current = tx
        .stored_policy_value(scope.tenant(), b"u", limits.schema, &mut charge)?
        .ok_or("original current missing")?;
    let StoredPolicyValue::Original(current) = current else {
        return Err("original authority not raw ETF".into());
    };
    assert_eq!(current, expected_original(generation)?);
    let complete = tx
        .stored_policy_value(scope.tenant(), b"r", limits.schema, &mut charge)?
        .ok_or("unrelated V2 missing")?;
    let StoredPolicyValue::Complete(complete) = complete else {
        return Err("unrelated V2 format changed".into());
    };
    assert_eq!(
        complete.encoded(),
        expected_complete(if legacy5 { 8 } else { 7 })?
    );
    for (name, at, time, allocated) in [
        (b"set".as_slice(), position, 12, 1),
        (b"replace", position + 1, 13, 2),
    ]
    .into_iter()
    .take(usize::from(generation))
    {
        let receipt = tx
            .outcome(scope.tenant(), &RequestId::new(name.to_vec())?)?
            .ok_or("original snapshot receipt missing")?;
        assert_eq!(receipt.response(), expected_original(allocated)?);
        assert_eq!(receipt.intent().version(), 4);
        assert_eq!(
            (
                receipt.original_position(),
                receipt.original_time().get(),
                receipt.expires_at().get()
            ),
            (f::at(at), time, time + 1000)
        );
    }
    Ok(())
}

fn advance(dir: &Path, wire: &[u8], position: u64) -> TestResult {
    let (log, state) = f::open_pair(dir)?;
    let scope = f::scope()?;
    let (mut log, mut state) = purge_snapshot(
        log,
        state,
        scope.group(),
        scope.recovery(),
        caps()?,
        SnapshotLimits::default(),
    )?;
    assert_eq!(log.state()?.last_purged(), Some(f::at(position)));
    assert_eq!(
        state.snapshot_pin(SnapshotLimits::default())?,
        Some(Sha256::digest(wire).into())
    );
    artifact(&mut state, wire)?;
    assert_eq!(
        f::applied(&mut state)?
            .0
            .last_entry()
            .map(EntryIdentity::log),
        Some(f::at(position + 2))
    );
    Ok(())
}

#[test]
fn mixed_legacy_and_original_bases_join_then_replay() -> TestResult {
    for legacy5 in [false, true] {
        let source = f::directory("original-snapshot-reference")?;
        let (old, wire, position) = seed(&source, legacy5, true)?;
        let expected = cohort(&source)?;
        assert!(expected.iter().all(Option::is_some));
        let dir = f::directory("original-snapshot-replay")?;
        assert_eq!(
            seed(&dir, legacy5, false)?,
            (old.clone(), wire.clone(), position)
        );
        let logs = (f::log_rows(&dir)?, f::log_meta(&dir)?);
        let scope = f::scope()?;
        for _ in 0..2 {
            let (log, state) = f::open_pair(&dir)?;
            let (mut log, mut state) = recover_snapshotted(
                log,
                state,
                scope.group(),
                scope.recovery(),
                caps()?,
                SnapshotLimits::default(),
            )?;
            inspect(&mut state, 2, position, legacy5)?;
            artifact(&mut state, &old)?;
            artifact(&mut state, &wire)?;
            assert_eq!(log.state()?.last_purged(), Some(f::at(position - 1)));
            assert_eq!(
                f::applied(&mut state)?
                    .0
                    .last_entry()
                    .map(EntryIdentity::log),
                Some(f::at(position + 2))
            );
            drop((log, state));
            assert_eq!(cohort(&dir)?, expected);
            assert_eq!((f::log_rows(&dir)?, f::log_meta(&dir)?), logs);
        }
        advance(&source, &wire, position)?;
        advance(&dir, &wire, position)?;
        assert_eq!(cohort(&dir)?, cohort(&source)?);
        assert_eq!(
            (f::log_rows(&dir)?, f::log_meta(&dir)?),
            (f::log_rows(&source)?, f::log_meta(&source)?)
        );
        std::fs::remove_dir_all(dir)?;
        std::fs::remove_dir_all(source)?;
    }
    Ok(())
}
