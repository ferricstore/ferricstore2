//! Real authenticated size observations preserve bytes, expiry and ownership.

use std::io::Cursor;

use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{OriginalValueRefKind, WorkflowValueReadFailure, parse_original_value_ref};
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_ports::state::StateTransactionError;
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_types::identity::TenantId;
use wf_types::version::{HlcTimestamp, TimestampMillis};

use super::{TestResult, fixtures as f};

mod fixtures;
mod projection;
use fixtures::{
    assert_history, assert_scope_refusals, assert_source_catalog, assert_values, metadata, publish,
    publish_steps, read_clock, reference, setup, unsupported_refs,
};

#[test]
fn sized_value_reads_authenticate_before_omitting_at_threshold() -> TestResult {
    let (directory, mut store, scope) = setup("sized-threshold")?;
    let references = publish(&mut store, &scope, &[(b"i", b"123456789")])?;
    let refs = [references[0].as_slice()];
    let mut clock = read_clock(201)?;
    let mut tiny = f::read_limits();
    tiny.maximum_stored_bytes = 1;
    let first = store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 8, tiny);
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let first = first?;
    assert_values(&first, &[], &[(0, 9)])?;
    assert_eq!(first.slots.as_slice(), [Ok(Some(0))]);
    let exact = store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 9, tiny)?;
    assert_values(&exact, &[], &[(0, 9)])?;
    assert!(matches!(
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 10, tiny),
        Err(StateTransactionError::Limit)
    ));
    assert!(matches!(
        store.read_workflow_value_batch(&mut clock, &scope, &refs, tiny),
        Err(StateTransactionError::Limit)
    ));
    for threshold in [10, usize::MAX] {
        let copied = store.read_workflow_value_batch_with_sizes(
            &mut clock,
            &scope,
            &refs,
            threshold,
            f::read_limits(),
        )?;
        assert_values(&copied, &[(0, b"123456789")], &[])?;
    }
    assert_eq!(clock.calls, 6);
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn sized_value_reads_preserve_order_duplicates_and_mixed_outcomes() -> TestResult {
    let (directory, mut store, scope) = setup("sized-mixed")?;
    let owned = publish(
        &mut store,
        &scope,
        &[
            (b"large", b"123456789"),
            (b"small", b"xy"),
            (b"third", b"z"),
        ],
    )?;
    let unsupported = unsupported_refs()?;
    let missing = reference(b"missing", OriginalValueRefKind::Payload, 1)?;
    let zero = reference(b"large", OriginalValueRefKind::Payload, 0)?;
    let refs = [
        owned[0].as_slice(),
        &owned[1],
        &missing,
        b"invalid",
        &owned[0],
        &owned[1],
        &unsupported[0],
        &unsupported[1],
        &zero,
        &owned[2],
        &owned[2],
    ];
    let mut limits = f::read_limits();
    limits.maximum_stored_bytes = 3;
    let mut clock = read_clock(201)?;
    let first = store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 5, limits);
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let first = first?;
    assert_values(&first, &[(1, b"xy"), (2, b"z")], &[(0, 9)])?;
    assert_eq!(
        first.slots.as_slice(),
        [
            Ok(Some(0)),
            Ok(Some(1)),
            Ok(None),
            Ok(None),
            Ok(Some(0)),
            Ok(Some(1)),
            Err(WorkflowValueReadFailure::UnsupportedProfile),
            Err(WorkflowValueReadFailure::UnsupportedProfile),
            Ok(None),
            Ok(Some(2)),
            Ok(Some(2))
        ]
    );
    assert_eq!(clock.calls, 1);
    limits.maximum_stored_bytes = 2;
    assert!(matches!(
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 5, limits),
        Err(StateTransactionError::Limit)
    ));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn sized_value_reads_report_exact_stored_fsv2_sizes() -> TestResult {
    let (directory, mut store, scope) = setup("sized-fsv2")?;
    let owned = publish(
        &mut store,
        &scope,
        &[(b"magic", b"FSV2\x02opaque"), (b"empty", b"")],
    )?;
    let refs = [owned[0].as_slice(), &owned[1]];
    let mut clock = read_clock(201)?;
    let first =
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 16, f::read_limits());
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let first = first?;
    assert_values(&first, &[(1, b"")], &[(0, 16)])?;
    assert_eq!(first.slots.as_slice(), [Ok(Some(0)), Ok(Some(1))]);
    let copied = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &refs,
        17,
        f::read_limits(),
    )?;
    assert_values(&copied, &[(0, b"FSV2\x01FSV2\x02opaque"), (1, b"")], &[])?;
    let zero_cap = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &refs,
        1,
        f::read_limits(),
    )?;
    assert_values(&zero_cap, &[(1, b"")], &[(0, 16)])?;
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn sized_value_reads_keep_expiry_drift_and_clock_failures_distinct() -> TestResult {
    let (directory, mut store, scope) = setup("sized-expiry")?;
    let owned = publish_steps(&mut store, &scope, true)?;
    let refs = [owned[0].as_slice(), &owned[1], &owned[1]];
    let mut clock = read_clock(249)?;
    let first =
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 1, f::read_limits());
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let first = first?;
    assert_values(&first, &[], &[(0, 16), (1, 1)])?;
    assert_eq!(
        first.slots.as_slice(),
        [Ok(Some(0)), Ok(Some(1)), Ok(Some(1))]
    );
    clock.hlc = HlcTimestamp::new(250, 0)?;
    clock.wall = TimestampMillis::new(250);
    let expired = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &refs,
        1,
        f::read_limits(),
    )?;
    assert_values(&expired, &[], &[(0, 16)])?;
    assert_eq!(expired.slots.as_slice(), [Ok(Some(0)), Ok(None), Ok(None)]);
    clock.hlc = HlcTimestamp::new(1_500, 0)?;
    clock.wall = TimestampMillis::new(249);
    let drift = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &refs,
        1,
        f::read_limits(),
    )?;
    assert_values(&drift, &[], &[(0, 16)])?;
    assert_eq!(
        drift.slots.as_slice(),
        [
            Ok(Some(0)),
            Err(WorkflowValueReadFailure::UnsafeDrift),
            Err(WorkflowValueReadFailure::UnsafeDrift)
        ]
    );
    clock.failure = Some(StateTransactionError::Unavailable);
    assert!(matches!(
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 1, f::read_limits()),
        Err(StateTransactionError::Unavailable)
    ));
    let skipped = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &[b"invalid"],
        1,
        f::read_limits(),
    )?;
    assert_eq!(skipped.slots.as_slice(), [Ok(None)]);
    assert_eq!(clock.calls, 4);
    clock.failure = None;
    let healthy = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &[&owned[0]],
        17,
        f::read_limits(),
    )?;
    assert_values(&healthy, &[(0, b"FSV2\x01FSV2\x02opaque")], &[])?;
    assert_eq!(clock.calls, 5);
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn sized_value_reads_keep_scope_profiles_and_limits_operational() -> TestResult {
    let (directory, mut store, scope) = setup("sized-profiles")?;
    let references = publish(&mut store, &scope, &[(b"i", b"ok")])?;
    drop(store);
    let rows = crate::fixtures::raw_rows(&directory.join("state"))?;
    let meta = metadata(&directory.join("state"))?;
    let store = RedbStateStore::open(
        &directory.join("state"),
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    let mut clock = read_clock(201)?;
    let refs = [references[0].as_slice()];
    let first =
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 3, f::read_limits());
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    assert_values(&first?, &[(0, b"ok")], &[])?;
    assert_scope_refusals(&store, &mut clock, &scope, &refs)?;
    for axis in 0..4 {
        let mut limits = f::read_limits();
        match axis {
            0 => limits.maximum_references = 0,
            1 => limits.maximum_stored_bytes = 0,
            2 => limits.maximum_work = 0,
            _ => limits.maximum_temporary_bytes = 0,
        }
        assert!(matches!(
            store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 3, limits),
            Err(StateTransactionError::Limit)
        ));
    }
    let mut oversized = b"f:{f}:v:p:".to_vec();
    oversized.resize(65_534, b'x');
    oversized.extend_from_slice(b":1");
    assert_eq!(oversized.len(), 65_536);
    assert!(parse_original_value_ref(&oversized).is_some());
    clock.failure = Some(StateTransactionError::Unavailable);
    for invalid in [
        vec![],
        vec![b"".as_slice(), b"invalid", oversized.as_slice()],
    ] {
        let skipped = store.read_workflow_value_batch_with_sizes(
            &mut clock,
            &scope,
            &invalid,
            1,
            f::read_limits(),
        )?;
        assert!(skipped.values.is_empty());
        assert_eq!(skipped.slots, vec![Ok(None); invalid.len()]);
    }
    assert_eq!(clock.calls, 1);
    clock.failure = None;
    let foreign = RequestScope::new(
        TenantId::new(b"other".to_vec())?,
        scope.group().clone(),
        scope.recovery().clone(),
    );
    let isolated = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &foreign,
        &refs,
        1,
        f::read_limits(),
    )?;
    assert_eq!(isolated.slots.as_slice(), [Ok(None)]);
    let legacy = crate::fixtures::create(&directory.join("legacy"))?;
    assert!(matches!(
        legacy.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 1, f::read_limits()),
        Err(StateTransactionError::UnsupportedLayout)
    ));
    let unsupported = unsupported_refs()?;
    let refused = store.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &[&unsupported[0], &unsupported[1]],
        1,
        f::read_limits(),
    )?;
    assert_eq!(
        refused.slots.as_slice(),
        [Err(WorkflowValueReadFailure::UnsupportedProfile); 2]
    );
    drop((store, legacy));
    assert_eq!(crate::fixtures::raw_rows(&directory.join("state"))?, rows);
    assert_eq!(metadata(&directory.join("state"))?, meta);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn sized_value_reads_owned_results_survive_store_and_reference_drop() -> TestResult {
    let (directory, mut store, scope) = setup("sized-owners")?;
    let owned = publish(
        &mut store,
        &scope,
        &[(b"large", b"123456789"), (b"small", b"xyz")],
    )?;
    let mut clock = read_clock(201)?;
    let refs = [owned[0].as_slice(), &owned[1], &owned[0], &owned[1]];
    let first =
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 5, f::read_limits());
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let first = first?;
    let old = store.read_workflow_value_batch(&mut clock, &scope, &refs, f::read_limits())?;
    drop((store, owned, scope));
    assert_values(&first, &[(1, b"xyz")], &[(0, 9)])?;
    assert_eq!(
        first.slots.as_slice(),
        [Ok(Some(0)), Ok(Some(1)), Ok(Some(0)), Ok(Some(1))]
    );
    f::assert_read(
        &old,
        &[b"123456789", b"xyz"],
        &[Some(0), Some(1), Some(0), Some(1)],
    )?;
    assert_eq!(clock.calls, 2);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn sized_value_reads_preserve_durable_and_snapshot_catalogs() -> TestResult {
    let (directory, mut store, scope) = setup("sized-snapshot")?;
    let references = publish_steps(&mut store, &scope, false)?;
    assert_history(&store, &scope)?;
    drop(store);
    let path = directory.join("state");
    let rows = crate::fixtures::raw_rows(&path)?;
    let initial_meta = metadata(&path)?;
    let mut store = RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    let refs = [references[0].as_slice(), &references[1], &references[0]];
    let mut clock = read_clock(201)?;
    let first =
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 16, f::read_limits());
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let first = first?;
    assert_values(&first, &[(1, b"")], &[(0, 16)])?;
    assert_eq!(
        first.slots.as_slice(),
        [Ok(Some(0)), Ok(Some(1)), Ok(Some(0))]
    );
    let mut wire = Vec::new();
    let captured = store.capture_snapshot(&mut wire, SnapshotLimits::default())?;
    assert!(wire.starts_with(b"FWSP\0\x04"));
    assert_eq!(captured.row_count(), rows.len() + 3);
    store.pin_snapshot(*captured.id(), SnapshotLimits::default())?;
    assert_eq!(
        store.snapshot_pin(SnapshotLimits::default())?,
        Some(*captured.id())
    );
    let mut retained = Vec::new();
    assert_eq!(
        store
            .current_snapshot(&mut retained, SnapshotLimits::default())?
            .ok_or("missing artifact")?
            .id(),
        captured.id()
    );
    assert_eq!(retained, wire);
    drop(store);
    assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
    let source_meta = metadata(&path)?;
    assert_source_catalog(&rows, &source_meta, &initial_meta);
    let stage = RedbSnapshotStage::receive(
        &directory.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?;
    let target = directory.join("target");
    let fresh = f::fresh(&target)?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let mut restored = fresh?;
    assert_eq!(
        restored
            .install_snapshot(stage, SnapshotLimits::default())?
            .id(),
        captured.id()
    );
    assert_history(&restored, &scope)?;
    let read = restored.read_workflow_value_batch_with_sizes(
        &mut clock,
        &scope,
        &refs,
        17,
        f::read_limits(),
    )?;
    let mut installed_wire = Vec::new();
    assert_eq!(
        restored
            .current_snapshot(&mut installed_wire, SnapshotLimits::default())?
            .ok_or("installed artifact missing")?
            .id(),
        captured.id()
    );
    assert_eq!(installed_wire, wire);
    assert_eq!(restored.snapshot_pin(SnapshotLimits::default())?, None);
    drop((restored, references, scope));
    assert_values(&first, &[(1, b"")], &[(0, 16)])?;
    assert_values(&read, &[(0, b"FSV2\x01FSV2\x02opaque"), (1, b"")], &[])?;
    assert_eq!(
        read.slots.as_slice(),
        [Ok(Some(0)), Ok(Some(1)), Ok(Some(0))]
    );
    assert_eq!(crate::fixtures::raw_rows(&target)?, rows);
    let target_meta = metadata(&target)?;
    for name in [b"manifest".as_slice(), b"membership", b"origin"] {
        assert_eq!(target_meta.get(name), source_meta.get(name));
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn sized_value_reads_enforce_verified_work_and_temporary_boundaries() -> TestResult {
    let (directory, mut store, scope) = setup("sized-resources")?;
    let payload = vec![b'L'; crate::fixtures::MIB];
    let owned = publish(&mut store, &scope, &[(b"large", &payload)])?;
    let refs = [owned[0].as_slice(), &owned[0]];
    let mut clock = read_clock(201)?;
    let mut limits = f::read_limits();
    limits.maximum_stored_bytes = 1;
    limits.maximum_temporary_bytes = payload.len() + payload.len() / 2;
    let first = store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 1, limits);
    assert!(first.is_ok(), "{:?}", first.as_ref().err());
    let first = first?;
    assert_values(&first, &[], &[(0, crate::fixtures::MIB)])?;
    assert_eq!(first.slots.as_slice(), [Ok(Some(0)), Ok(Some(0))]);
    for axis in 0..3 {
        let mut small = limits;
        match axis {
            0 => small.maximum_work = payload.len() - 1,
            1 => small.maximum_temporary_bytes = payload.len() - 1,
            _ => small.maximum_references = 1,
        }
        assert!(matches!(
            store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, 1, small),
            Err(StateTransactionError::Limit)
        ));
    }
    let mut copy = limits;
    copy.maximum_stored_bytes = payload.len();
    assert!(matches!(
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, usize::MAX, copy),
        Err(StateTransactionError::Limit)
    ));
    copy.maximum_temporary_bytes = f::read_limits().maximum_temporary_bytes;
    let copied =
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, usize::MAX, copy)?;
    assert_values(&copied, &[(0, &payload)], &[])?;
    copy.maximum_stored_bytes -= 1;
    assert!(matches!(
        store.read_workflow_value_batch_with_sizes(&mut clock, &scope, &refs, usize::MAX, copy),
        Err(StateTransactionError::Limit)
    ));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
