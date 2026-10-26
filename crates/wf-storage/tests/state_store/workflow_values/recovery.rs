//! Retained birth provenance and real complete snapshot custody.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::Path;

use redb::Durability;
use wf_domain::policy::definition::{FieldPatch, PolicyPatch, StateChange, StatePolicyPatch};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::{Create, Operation, Transition};
use wf_ports::blob::{OriginalValuePartition, OriginalValueRefKind, render_original_value_ref};
use wf_ports::consensus::PreparedOperation;
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_ports::state::StateTransactionError;
use wf_storage::snapshot::RedbSnapshotStage;
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_storage::transaction::RecordBatchTransactionOutcome;
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::{Fence, HlcTimestamp, TimestampMillis};

use super::{TestResult, fixtures as f};

#[test]
fn value_binary_reopen_preserves_capture_after_policy_change() -> TestResult {
    let directory = crate::fixtures::directory("value-policy-reopen")?;
    let path = directory.join("state");
    let fresh = f::fresh(&path)?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let Ok(mut store) = fresh else {
        return Err("expected fresh value capability".into());
    };
    let scope = crate::fixtures::scope()?;
    f::install_complete(&mut store, &scope, recovery_policy(false), None, 0, 10)?;
    let installed = f::load_policy(&mut store, &scope)?;
    let witness = f::witness(b"r", installed.as_ref())?;
    assert_eq!(witness.generation, 1);
    assert_eq!(witness.profile, 1);
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"reopen".to_vec()),
    });
    let operation = PreparedOperation::Batch(vec![f::item(b"birth", 20, create)?]);
    let policies = vec![f::witness(b"r", installed.as_ref())?];
    let prepared = f::batch(5, b"birth", 20, operation, vec![f::input(0, 0)], policies)?;
    f::apply_value(&mut store, &scope, &prepared, 1)?;
    f::install_complete(&mut store, &scope, recovery_policy(true), Some(1), 2, 30)?;
    let replay = f::apply_value(&mut store, &scope, &prepared, 3)?;
    assert!(matches!(
        &replay,
        RecordBatchTransactionOutcome::Retained { .. }
    ));
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = replay else {
        return Err("expected original retained replay".into());
    };
    assert_eq!(
        outcome.status(),
        wf_domain::workflow::request::RetainedStatus::Accepted
    );
    assert_eq!(outcome.original_position(), crate::fixtures::log(1));
    assert_eq!(outcome.original_time().get(), 20);
    assert_eq!(outcome.response(), [0]);
    drop(prepared);
    drop(installed);
    drop(store);
    let manifest = f::raw_manifest(&path)?;
    assert!(manifest.starts_with(b"FWSS\0\x07"));
    let store = RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    let binding = store.workflow_value_binding(
        &scope,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(b"birth".to_vec())?,
        None,
        f::read_limits(),
    );
    assert!(binding.is_ok(), "{:?}", binding.as_ref().err());
    f::assert_binding(
        &binding?,
        (b"p", b"birth"),
        1_u128 << 32,
        [Some(1), None, None],
        (Some(5_000), 22_000, 2, 40, Some(22_000), None),
        &witness,
    )?;
    let reference = render_original_value_ref(
        b"birth",
        OriginalValueRefKind::Payload,
        1,
        OriginalValuePartition::Explicit(b"p"),
        65_535,
    )
    .map_err(|error| format!("value reference fixture: {error:?}"))?;
    let mut clock = f::ValueReadClock {
        hlc: HlcTimestamp::new(40, 0)?,
        wall: TimestampMillis::new(40),
        calls: 0,
        failure: None,
    };
    let read = store.read_workflow_value_batch(&mut clock, &scope, &[&reference], f::read_limits());
    assert!(read.is_ok(), "{:?}", read.as_ref().err());
    let read = read?;
    drop(store);
    drop(reference);
    f::assert_read(&read, &[b"reopen"], &[Some(0)])?;
    assert_eq!(clock.calls, 1);
    assert_eq!(f::raw_manifest(&path)?, manifest);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn value_binary_snapshot_roundtrip_preserves_alias_and_immutable_bytes() -> TestResult {
    let directory = crate::fixtures::directory("value-snapshot")?;
    let source = directory.join("source");
    let target = directory.join("target");
    let fresh = f::fresh(&source)?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let Ok(mut store) = fresh else {
        return Err("expected fresh value capability".into());
    };
    let scope = crate::fixtures::scope()?;
    publish_snapshot_values(&mut store, &scope)?;
    let mut wire = Vec::new();
    let captured = store.capture_snapshot(&mut wire, SnapshotLimits::default());
    assert!(captured.is_ok(), "{:?}", captured.as_ref().err());
    let captured = captured?;
    assert!(wire.starts_with(b"FWSP\0\x04"));
    drop(store);
    let rows = crate::fixtures::raw_rows(&source)?;
    assert_eq!(captured.row_count(), rows.len() + 3);
    for (family, count) in [(8, 2), (9, 2), (10, 1), (11, 2)] {
        let prefix = [b'F', b'W', b'P', b'K', 0, 1, family];
        assert_eq!(
            rows.keys().filter(|key| key.starts_with(&prefix)).count(),
            count
        );
    }
    let stage = RedbSnapshotStage::receive(
        &directory.join("stage"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(&wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    );
    assert!(stage.is_ok(), "{:?}", stage.as_ref().err());
    let fresh = f::fresh(&target)?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let Ok(mut restored) = fresh else {
        return Err("expected same-lineage value target".into());
    };
    let installed = restored.install_snapshot(stage?, SnapshotLimits::default());
    assert!(installed.is_ok(), "{:?}", installed.as_ref().err());
    assert_eq!(installed?.id(), captured.id());
    let partition = OriginalValuePartition::Explicit(b"p");
    let references = [1, 2]
        .into_iter()
        .map(|version| {
            render_original_value_ref(
                b"snap",
                OriginalValueRefKind::Payload,
                version,
                partition,
                65_535,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("value reference fixture: {error:?}"))?;
    let refs = [
        references[0].as_slice(),
        references[1].as_slice(),
        references[0].as_slice(),
    ];
    let mut clock = f::ValueReadClock {
        hlc: HlcTimestamp::new(200, 0)?,
        wall: TimestampMillis::new(200),
        calls: 0,
        failure: None,
    };
    let read = restored.read_workflow_value_batch(&mut clock, &scope, &refs, f::read_limits());
    assert!(read.is_ok(), "{:?}", read.as_ref().err());
    let read = read?;
    drop(restored);
    drop(references);
    f::assert_read(
        &read,
        &[b"FSV2\x01FSV2snapshot", b""],
        &[Some(0), Some(1), Some(0)],
    )?;
    assert_eq!(clock.calls, 1);
    assert_eq!(crate::fixtures::raw_rows(&target)?, rows);
    assert_eq!(f::raw_manifest(&target)?, f::raw_manifest(&source)?);
    remove_required_immutable(&target)?;
    assert!(matches!(
        RedbStateStore::open(
            &target,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreLimits::default()
        ),
        Err(StateTransactionError::Corrupt)
    ));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn recovery_policy(updated: bool) -> PolicyPatch {
    let state = StatePolicyPatch {
        retention: FieldPatch::Set(RetentionOverride {
            ttl_ms: Some(if updated { 88_000 } else { 22_000 }),
            history_max_events: Some(if updated { 80 } else { 40 }),
        }),
        ..Default::default()
    };
    PolicyPatch {
        max_active_ms: if updated {
            FieldPatch::Clear
        } else {
            FieldPatch::Set(5_000)
        },
        retention: if updated {
            FieldPatch::Unchanged
        } else {
            FieldPatch::Set(RetentionOverride {
                ttl_ms: Some(11_000),
                history_max_events: Some(30),
            })
        },
        states: BTreeMap::from([(b"q".to_vec(), StateChange::Patch(state))]),
        ..Default::default()
    }
}

fn publish_snapshot_values(store: &mut RedbStateStore, scope: &RequestScope) -> TestResult {
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"FSV2snapshot".to_vec()),
    });
    let transition = Operation::Transition(Transition {
        from: b"q".to_vec(),
        to: b"waiting".to_vec(),
        fence: Fence::new(0),
        lease_token: None,
        run_at: None,
        payload: Some(Vec::new()),
    });
    let operation = PreparedOperation::Batch(vec![
        f::item(b"snap", 100, create)?,
        f::item(b"snap", 100, transition)?,
    ]);
    let inputs = vec![f::input(0, 0), f::input(1, 0)];
    let policies = vec![f::witness(b"r", None)?];
    let prepared = f::batch(6, b"snapshot", 100, operation, inputs, policies)?;
    f::apply_value(store, scope, &prepared, 0)?;
    Ok(())
}

fn remove_required_immutable(path: &Path) -> TestResult {
    let mut expected = crate::fixtures::raw_rows(path)?;
    let manifest = f::raw_manifest(path)?;
    let key = expected
        .keys()
        .find(|key| key.starts_with(b"FWPK\0\x01\x08"))
        .ok_or("successful snapshot has no immutable value row")?
        .clone();
    assert!(expected.remove(&key).is_some());
    let database = redb::Builder::new().open(path)?;
    let mut write = database.begin_write()?;
    write.set_durability(Durability::Immediate)?;
    let mut table = write.open_table(crate::fixtures::ROWS)?;
    assert!(table.remove(key.as_slice())?.is_some());
    drop(table);
    write.commit()?;
    drop(database);
    assert_eq!(crate::fixtures::raw_rows(path)?, expected);
    assert_eq!(f::raw_manifest(path)?, manifest);
    Ok(())
}
