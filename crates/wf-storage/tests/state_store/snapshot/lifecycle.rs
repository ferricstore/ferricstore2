//! Independent complete canonical bytes and immutable state after ordinary apply.

use std::io::Cursor;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::caller::{MutationFieldValue, MutationInput, normalize_core_mutation};
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_ports::log::encode_public_mutation;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_independent_record_batch_transaction,
    apply_public_mutation_transaction, apply_record_batch_transaction,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};

use super::super::{control, recovery};
use super::{
    RedbSnapshotStage, SnapshotLimits, StateSnapshotStore, TestResult, f, fixtures as s, limits,
};

#[test]
fn literal_initial_frame_descriptor_and_fresh_restore() -> TestResult {
    let dir = f::directory("snapshot-literal")?;
    let path = dir.join("source.redb");
    drop(s::empty(&path)?);
    let before = s::raw(&path)?;
    assert_eq!(before.len(), 4);
    assert_eq!(before[s::META].len(), 3);
    assert_eq!(before[s::META][b"snapshot".as_slice()], s::hex(s::NONE)?);
    assert!(before[s::SNAP_META].is_empty());
    assert!(before[s::SNAP_ROWS].is_empty());
    let authority = s::authority(&before)?;
    let literal = s::hex(s::INITIAL)?;
    assert_eq!(literal.len(), 325);
    assert_eq!(s::frame((b"g", b"z"), &authority)?, literal);
    assert_eq!(s::descriptor((b"g", b"z"), 3, &literal)?, s::hex(s::SOME)?);
    let mut state = f::open(&path)?;
    let mut none = b"existing".to_vec();
    assert!(
        state
            .current_snapshot(&mut none, SnapshotLimits::default())?
            .is_none()
    );
    assert_eq!(none, b"existing");
    let mut wire = Vec::new();
    let meta = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert_eq!(wire, literal);
    assert_eq!(
        meta.id().as_slice(),
        s::hex("1e4bc7621eeb5aba5567c8fc9c2470d758faf7cd7663e4eb4c156370dc7a8080")?
    );
    assert_eq!(meta.row_count(), 3);
    assert_eq!(meta.encoded_bytes(), 325);
    assert_eq!(meta.group().as_bytes(), b"g");
    assert_eq!(meta.recovery().as_bytes(), b"z");
    assert!(meta.control().last_entry().is_none());
    assert!(meta.membership().entry().is_none());
    let mut current = Vec::new();
    let saved = state
        .current_snapshot(&mut current, SnapshotLimits::default())?
        .ok_or("snapshot absent")?;
    assert_eq!(current, literal);
    assert_eq!(saved.id(), meta.id());
    assert_eq!(saved.control(), meta.control());
    assert_eq!(saved.membership().entry(), None);
    drop(state);
    let expected = s::installed(&authority)?;
    s::assert_raw(&path, &expected)?;
    let restored = dir.join("restored.redb");
    let mut incoming_stage = s::stage(&restored, &literal)?;
    let observed = incoming_stage.metadata(SnapshotLimits::default())?;
    assert_eq!(observed.id(), meta.id());
    assert_eq!(observed.row_count(), 3);
    let mut state = incoming_stage.into_store()?;
    let mut restored_wire = Vec::new();
    assert!(
        state
            .current_snapshot(&mut restored_wire, SnapshotLimits::default())?
            .is_some()
    );
    assert_eq!(restored_wire, literal);
    drop(state);
    s::assert_raw(&restored, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn mixed_replies(state: &mut RedbStateStore) -> TestResult {
    let frame = f::independent_frame(
        b"independent",
        13,
        vec![(b"b", f::cancel_op(1)), (b"c", f::cancel_op(9))],
    )?;
    let result = apply_independent_record_batch_transaction(
        state,
        &f::scope()?,
        frame.as_bytes(),
        f::log(4),
        f::driver_limits(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("independent reply absent".into());
    };
    assert_eq!(outcome.response(), [2, 0, 0, 0, 2, 0, 1, 0, 3]);
    let fields = [
        ("type", MutationFieldValue::Bytes(b"r")),
        ("state", MutationFieldValue::Bytes(b"q")),
    ];
    let caller = normalize_core_mutation(
        MutationInput {
            operation: "create",
            id: b"public",
            fields: &fields,
        },
        &[],
        Some(b"p"),
        AdmissionLimits::default(),
        16 * f::MIB,
    )?;
    let request = RequestId::new(b"public".to_vec())?;
    let (public, _) = encode_public_mutation(
        &f::scope()?,
        &request,
        &caller,
        f::clock(14)?,
        f::driver_limits().codec,
        f::driver_limits().workspace_bytes,
    )?;
    let result = apply_public_mutation_transaction(
        state,
        &f::scope()?,
        public.as_bytes(),
        f::log(5),
        f::driver_limits(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("public reply absent".into());
    };
    assert_eq!(outcome.response(), [0]);
    assert_eq!(outcome.response_version(), 2);
    let original = f::frame(b"late-seed", 12, vec![(b"late-seed", f::create_op())])?;
    let replay = apply_record_batch_transaction(
        state,
        &f::scope()?,
        original.as_bytes(),
        f::log(6),
        f::driver_limits(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = replay else {
        return Err("original replay absent".into());
    };
    assert_eq!(outcome.original_position(), f::log(3));
    assert_eq!(outcome.original_time().get(), 12);
    assert_eq!(outcome.expires_at().get(), 1012);
    assert_eq!(outcome.response(), [0]);
    let rejected = f::frame(b"reject", 15, vec![(b"public", f::create_op())])?;
    let result = apply_record_batch_transaction(
        state,
        &f::scope()?,
        rejected.as_bytes(),
        f::log(7),
        f::driver_limits(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("rejected reply absent".into());
    };
    assert_ne!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.response(), [1, 0, 1]);
    s::apply(state, b"advance", 8, 2000)?;
    assert!(matches!(
        apply_record_batch_transaction(
            state,
            &f::scope()?,
            original.as_bytes(),
            f::log(9),
            f::driver_limits()
        )?,
        RecordBatchTransactionOutcome::Expired(_)
    ));
    Ok(())
}

#[test]
fn immutable_capture_live_install_and_original_full_authority() -> TestResult {
    let dir = f::directory("snapshot-authority")?;
    let source = dir.join("source.redb");
    s::seed(&source)?;
    let mut state = f::open(&source)?;
    mixed_replies(&mut state)?;
    drop(state);
    let authority = s::authority(&s::raw(&source)?)?;
    let expected_wire = s::frame((b"g", b"z"), &authority)?;
    let wire = s::capture(&source)?;
    assert_eq!(wire, expected_wire);
    let expected = s::installed(&authority)?;
    s::assert_raw(&source, &expected)?;
    state = f::open(&source)?;
    s::apply(&mut state, b"after-capture", 10, 2001)?;
    let mut current = Vec::new();
    let meta = state
        .current_snapshot(&mut current, SnapshotLimits::default())?
        .ok_or("saved artifact absent")?;
    assert_eq!(current, wire);
    assert_eq!(meta.control().position().get(), 9);
    assert_eq!(meta.membership().log_id(), Some(f::log(1)));
    assert_eq!(
        meta.membership().membership().configs(),
        control::fixtures::membership()?.configs()
    );
    assert_eq!(
        meta.membership().membership().nodes(),
        control::fixtures::membership()?.nodes()
    );
    drop(state);
    let later = s::raw(&source)?;
    assert_eq!(later[s::SNAP_META], expected[s::SNAP_META]);
    assert_eq!(later[s::SNAP_ROWS], expected[s::SNAP_ROWS]);
    assert_ne!(later[s::ROWS], expected[s::ROWS]);
    let target = dir.join("target.redb");
    let mut active = s::create(&target)?;
    s::apply(&mut active, b"target-only", 0, 1)?;
    active.capture_snapshot(&mut Vec::new(), SnapshotLimits::default())?;
    let received = s::stage(&dir.join("stage.redb"), &wire)?;
    let installed = active.install_snapshot(received, SnapshotLimits::default())?;
    assert_eq!(installed.control(), meta.control());
    assert_eq!(installed.id(), meta.id());
    drop(active);
    s::assert_raw(&target, &expected)?;
    let mut source_state = f::open(&source)?;
    let mut newer = Vec::new();
    let new_meta = source_state.capture_snapshot(&mut newer, SnapshotLimits::default())?;
    assert_eq!(new_meta.control().position().get(), 10);
    assert_ne!(newer, wire);
    assert_eq!(newer, s::frame((b"g", b"z"), &s::authority(&later)?)?);
    let one = limits::profile(
        (1_000_000, 1_073_741_824, 67_108_864),
        (10_000_000, 1),
        (134_217_728, 134_217_728),
        536_870_912,
    )?;
    let mut one_byte = Vec::new();
    source_state.capture_snapshot(&mut one_byte, one)?;
    assert_eq!(one_byte, newer);
    drop(source_state);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn maximum_inline_policy_and_binary_scope_snapshot_roundtrip() -> TestResult {
    let dir = f::directory("snapshot-large")?;
    let path = dir.join("large.redb");
    let cap = s::large(&path)?;
    let scope = f::scope()?;
    let bulk_profile = limits::profile(
        (1_000_000, 1_073_741_824, 67_108_864),
        (10_000_000, 65_536),
        (f::MIB, 512 * f::MIB),
        1024 * f::MIB,
    )?;
    let before = s::raw(&path)?;
    let authority = s::authority(&before)?;
    let mut state =
        RedbStateStore::open(&path, scope.group().clone(), scope.recovery().clone(), cap)?;
    let mut wire = Vec::new();
    state.capture_snapshot(&mut wire, bulk_profile)?;
    assert_eq!(wire, s::frame((b"g", b"z"), &authority)?);
    assert!(wire.len() > f::MIB + 262_162);
    drop(state);
    let restored = dir.join("large-restored.redb");
    let incoming_stage = RedbSnapshotStage::receive(
        &restored,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        cap,
        bulk_profile,
    )?;
    let mut restored_state = incoming_stage.into_store()?;
    let tx = restored_state.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        100,
        128 * f::MIB,
        512 * f::MIB,
    )?;
    let record = tx
        .record(
            scope.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"large".to_vec())?,
        )?
        .ok_or("large record absent")?;
    assert_eq!(record.record.payload(), Some(vec![7; f::MIB].as_slice()));
    drop(tx);
    drop(restored_state);
    assert_eq!(s::raw(&restored)?, s::installed(&authority)?);
    binary_roundtrip(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn binary_roundtrip(dir: &std::path::Path) -> TestResult {
    let path = dir.join("binary.redb");
    let scope = RequestScope::new(
        TenantId::new(vec![0; 8192])?,
        GroupId::new(vec![0; 256])?,
        RecoveryId::new(vec![255; 256])?,
    );
    let partition = PartitionId::new(vec![0; 65_535])?;
    let placements = [(partition.clone(), scope.group().clone())];
    drop(RedbStateStore::create(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        StateStoreLimits::default(),
    )?);
    let mut record = f::record(&vec![0; 65_535], u128::MAX - 7, Some(10))?;
    record.tenant = scope.tenant().clone();
    record.partition = partition;
    let rows = recovery::cohort(&scope, f::raw_rows(&path)?, &[record], false)?;
    f::replace(&path, &rows, &f::raw_manifest(&path)?)?;
    let data = s::authority(&s::raw(&path)?)?;
    let namespace = (scope.group().as_bytes(), scope.recovery().as_bytes());
    let expected = s::frame(namespace, &data)?;
    let mut state = RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    let mut wire = Vec::new();
    let meta = state.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert_eq!(wire, expected);
    assert_eq!(meta.group(), scope.group());
    assert_eq!(meta.recovery(), scope.recovery());
    drop(state);
    let restored = dir.join("binary-restored.redb");
    let incoming_stage = RedbSnapshotStage::receive(
        &restored,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    drop(incoming_stage.into_store()?);
    let result = s::raw(&restored)?;
    assert_eq!(s::authority(&result)?, data);
    assert_eq!(result[s::SNAP_META], data[s::META]);
    assert_eq!(result[s::SNAP_ROWS], data[s::ROWS]);
    assert_eq!(
        result[s::META][b"snapshot".as_slice()],
        s::descriptor(namespace, meta.row_count(), &wire)?
    );
    Ok(())
}
