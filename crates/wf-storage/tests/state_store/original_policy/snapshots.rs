//! Exact birth/current/frozen bytes and real received custody across formats.
mod received;
use super::super::faults::fault_database;
use super::{TestResult, fixtures as f};
use redb::ReadableDatabase;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Cursor,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput, PolicySchemaCharge,
    PolicySchemaLimits, RawFlowPolicyValue as Raw,
};
use wf_domain::policy::definition::PolicyPatch;
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::request::RetainedStatus;
use wf_ports::log::{
    RaftLogEntry, RaftLogLimits, RaftLogStore, prepare_complete_core_policy_patch_frame,
    verify_complete_core_policy_patch_frame,
};
use wf_ports::snapshot::{
    SnapshotError, SnapshotLimits, SnapshotTransition, StateSnapshotStore,
    StateSnapshotTransitionStore,
};
use wf_ports::state::{GroupApplyStore, StateTransactionError};
use wf_storage::transaction::{
    OriginalPolicyTransactionError, RecordBatchTransactionOutcome,
    apply_complete_core_policy_patch_transaction, apply_original_policy_set_transaction,
};
use wf_storage::{
    log::RedbLogStore,
    snapshot::RedbSnapshotStage,
    state::{RedbStateStore, StateStoreGenesis, StateStoreLimits},
};
use wf_types::identity::{PartitionId, RequestId};

fn published(path: &Path, big: bool) -> TestResult<f::Cohort> {
    drop(f::create(path, &[], 64)?);
    let mut expected = f::cohort(path)?;
    let caps = f::limits();
    let s = f::scope()?;
    let magnitude = vec![0xff; 8192];
    let attrs = if big {
        vec![(
            Raw::Atom("version"),
            Raw::BigInteger {
                negative: false,
                little_endian: &magnitude,
            },
        )]
    } else {
        Vec::new()
    };
    let frame = f::public_frame(
        &s,
        &RequestId::new(b"publish".to_vec())?,
        100,
        b"r",
        Raw::Map(&attrs),
        Raw::List(&[]),
        &mut PolicySchemaCharge::new(caps.schema),
    )?;
    let mut version = vec![111];
    version.extend_from_slice(&8192_u32.to_be_bytes());
    version.push(0);
    version.extend_from_slice(&magnitude);
    let states = f::etf_map(&[])?;
    let mut entries = vec![(b"states".as_slice(), states.as_slice())];
    if big {
        entries.push((b"version", version.as_slice()));
    }
    let response = f::etf_policy(1, b"r", &entries)?;
    assert_eq!(response.len(), if big { 8258 } else { 51 });
    let metadata = f::metadata(
        &s,
        b"publish",
        0,
        100,
        f::frame_intent(&frame, 0)?,
        RetainedStatus::Accepted,
    )?;
    f::rows_mut(&mut expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"r"]), response.clone());
    f::finish(
        &mut expected,
        &s,
        &frame,
        f::log(0),
        (f::clock(100)?, f::clock(100)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade6(
        &mut expected,
        &s,
        f::catalog(&s, [None, None], None, None, None)?,
    )?;
    let mut store = f::open(path, 64)?;
    let result = f::apply(
        &mut store,
        &frame,
        0,
        caps,
        &mut PolicySchemaCharge::new(caps.schema),
    )?;
    assert!(
        matches!(result,RecordBatchTransactionOutcome::Retained{outcome,..}if outcome.status()==RetainedStatus::Accepted&&outcome.response()==response)
    );
    drop(store);
    assert_eq!(f::cohort(path)?, expected);
    drop(f::open(path, 64)?);
    Ok(expected)
}

fn frozen(expected: &mut f::Cohort, slot: usize) -> TestResult {
    let mut meta = expected[0].as_ref().ok_or("no live metadata")?.clone();
    meta.remove(b"snapshot".as_slice()).ok_or("no descriptor")?;
    expected[2 + 2 * slot] = Some(meta);
    expected[3 + 2 * slot] = Some(f::rows(expected)?.clone());
    Ok(())
}

fn capture(path: &Path, mut expected: f::Cohort) -> TestResult<(f::Cohort, Vec<u8>)> {
    let s = f::scope()?;
    let wanted = f::stream3(&expected)?;
    let id = Sha256::digest(&wanted).into();
    let facts = (id, 3 + f::rows(&expected)?.len(), wanted.len());
    let mut store = f::open(path, 64)?;
    let mut observed = Vec::new();
    let metadata = store.capture_snapshot(&mut observed, SnapshotLimits::default())?;
    assert_eq!(observed, wanted);
    assert_eq!(metadata.id(), &id);
    assert_eq!(metadata.row_count(), facts.1);
    frozen(&mut expected, 0)?;
    expected[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&s, [Some(facts), None], Some(0), None, None)?,
    );
    drop(store);
    assert_eq!(f::cohort(path)?, expected);
    drop(f::open(path, 64)?);
    Ok((expected, wanted))
}

fn stage(path: &Path, wire: &[u8]) -> TestResult<RedbSnapshotStage> {
    let s = f::scope()?;
    Ok(RedbSnapshotStage::receive(
        path,
        s.group().clone(),
        s.recovery().clone(),
        &mut Cursor::new(wire),
        f::store_limits(64)?,
        SnapshotLimits::default(),
    )?)
}

fn assert_empty_stage(path: &Path) -> TestResult {
    let database = redb::Builder::new().open(path)?;
    let tx = database.begin_read()?;
    assert_eq!(tx.list_tables()?.count(), 0);
    assert_eq!(tx.list_multimap_tables()?.count(), 0);
    Ok(())
}

#[test]
fn mixed_birth_current_and_frozen_formats_capture_receive_install() -> TestResult {
    let dir = f::directory("original-mixed-snapshots")?;
    mixed_lineage(&dir)?;
    let source = dir.join("source");
    let (expected, wire) = capture(&source, published(&source, false)?)?;
    let meta = expected[0].as_ref().ok_or("missing metadata")?;
    assert_eq!(meta.len(), 4);
    assert_eq!(
        (
            meta[b"manifest".as_slice()].len(),
            &meta[b"manifest".as_slice()][..6]
        ),
        (146, b"FWSS\0\x06".as_slice())
    );
    assert_eq!(&meta[b"origin".as_slice()][..6], b"FWSS\0\x03");
    assert_eq!(
        &meta[b"manifest".as_slice()][110..114],
        &1_u32.to_be_bytes()
    );
    assert_eq!((&wire[..6], wire[19]), (b"FWSP\0\x03".as_slice(), 3));
    let target = dir.join("target");
    let mut active = f::create(&target, &[], 64)?;
    assert_eq!(
        active
            .install_snapshot(stage(&dir.join("stage"), &wire)?, SnapshotLimits::default())?
            .id()
            .as_slice(),
        Sha256::digest(&wire).as_slice()
    );
    assert!(active.supports_snapshot_transitions()?);
    drop(active);
    assert_eq!(f::cohort(&target)?, expected);
    let before = f::cohort(&target)?;
    drop(f::open(&target, 64)?.upgrade_snapshot_layout(SnapshotLimits::default())?);
    assert_eq!(
        f::cohort(&target)?,
        before,
        "layout6 no-op rewrote artifacts"
    );
    received::downgrade(&dir, &target, &expected)?;
    received::stage_and_commit_faults(&dir, &wire, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn core_frame() -> TestResult<wf_ports::log::PreparedBytes> {
    let caps = f::limits();
    Ok(prepare_complete_core_policy_patch_frame(
        CompleteCorePolicyPatchRequestInput::new(
            f::scope()?,
            RequestId::new(b"core".to_vec())?,
            f::clock(1)?,
            CompleteCorePolicyPatchInput {
                workflow_type: b"complete".to_vec(),
                expected_generation: Some(0),
                patch: PolicyPatch::default(),
            },
        ),
        caps.schema,
        caps.codec.admission(),
        &mut PolicySchemaCharge::new(caps.schema),
    )?)
}

fn upgrade5(expected: &mut f::Cohort, descriptor: Vec<u8>) -> TestResult {
    let mut digest = Sha256::new();
    digest.update(b"wf-state-current-catalog-v1");
    let mut policies = 0;
    for (key, value) in f::rows(expected)?
        .iter()
        .filter(|(key, _)| matches!(key.get(6), Some(6 | 7)))
    {
        if key[6] == 6 {
            policies += 1;
        }
        digest.update(u64::try_from(key.len())?.to_be_bytes());
        digest.update(key);
        digest.update(u64::try_from(value.len())?.to_be_bytes());
        digest.update(value);
    }
    let origin = expected[0].as_ref().ok_or("no metadata")?[b"manifest".as_slice()].clone();
    let mut manifest = f::manifest6(&origin, (1, policies), digest.finalize().into())?;
    manifest.drain(110..114);
    manifest[4..6].copy_from_slice(&5_u16.to_be_bytes());
    f::rehash(&mut manifest)?;
    let meta = expected[0].as_mut().ok_or("no metadata")?;
    meta.insert(b"origin".to_vec(), origin);
    meta.insert(b"manifest".to_vec(), manifest);
    meta.insert(b"snapshot".to_vec(), descriptor);
    expected[4] = Some(BTreeMap::new());
    expected[5] = Some(BTreeMap::new());
    Ok(())
}

fn mixed_lineage(dir: &Path) -> TestResult {
    let path = dir.join("lineage");
    let s = f::scope()?;
    let mut state = f::create(&path, &[], 64)?;
    let row = f::owner(&s, b"p", f::data(b"i", b"foreign", b"q", 0, Some(0))?, 0, 0)?;
    f::seed(&mut state, &s, std::slice::from_ref(&row), 0, 0)?;
    drop(state);
    let seed = f::batch(
        &s,
        b"seed-0",
        0,
        vec![wf_ports::consensus::PreparedBatchItem {
            partition: row.partition.clone(),
            create_priority: 0,
            command: wf_domain::workflow::CommittedCommand {
                id: row.record.id().clone(),
                now: wf_types::version::TimestampMillis::new(0),
                operation: wf_domain::workflow::Operation::Create(wf_domain::workflow::Create {
                    workflow_type: b"foreign".to_vec(),
                    state: b"q".to_vec(),
                    run_at: Some(wf_types::version::TimestampMillis::new(0)),
                    payload: None,
                }),
            },
        }],
    )?;
    let mut log = RedbLogStore::create(
        &dir.join("lineage-log"),
        s.group().clone(),
        s.recovery().clone(),
        RaftLogLimits::default(),
    )?;
    log.append(vec![RaftLogEntry::new(f::log(0), seed.as_bytes().to_vec())])?;
    log.save_committed(f::log(0))?;
    let mut expected = f::cohort(&path)?;
    let old3 = f::snapshot_wire(&s, &expected, 1)?;
    let facts3 = (
        Sha256::digest(&old3).into(),
        2 + f::rows(&expected)?.len(),
        old3.len(),
    );
    state = f::open(&path, 64)?;
    let mut bytes = Vec::new();
    assert_eq!(
        state
            .capture_snapshot(&mut bytes, SnapshotLimits::default())?
            .id(),
        &facts3.0
    );
    assert_eq!(bytes, old3);
    state.pin_snapshot(facts3.0, SnapshotLimits::default())?;
    log.purge_through(f::log(0), facts3.0)?;
    drop(state);
    // The old descriptor and frozen data are independently checked below after
    // the consuming core publication translates only their scalar facts.
    expected = f::cohort(&path)?;
    let frozen3 = (expected[2].clone(), expected[3].clone());
    lineage_complete(&path, &mut expected, &mut log, facts3)?;
    let old5 = f::snapshot_wire(&s, &expected, 2)?;
    let facts5 = (
        Sha256::digest(&old5).into(),
        3 + f::rows(&expected)?.len(),
        old5.len(),
    );
    state = f::open(&path, 64)?;
    bytes.clear();
    assert_eq!(
        state
            .capture_snapshot(&mut bytes, SnapshotLimits::default())?
            .id(),
        &facts5.0
    );
    assert_eq!(bytes, old5);
    drop(state);
    frozen(&mut expected, 1)?;
    expected[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&s, [Some(facts3), Some(facts5)], Some(1), Some(0), None)?,
    );
    assert_eq!(f::cohort(&path)?, expected);
    let frozen5 = (expected[4].clone(), expected[5].clone());
    lineage_original(
        &path,
        &mut expected,
        &mut log,
        (facts3, facts5),
        (&old3, &old5),
    )?;
    assert_eq!((expected[2].clone(), expected[3].clone()), frozen3);
    assert_eq!((expected[4].clone(), expected[5].clone()), frozen5);
    lineage_transitions(&path, &mut expected, &mut log, (facts3, facts5))?;
    drop(log);
    Ok(())
}

fn lineage_original(
    path: &Path,
    expected: &mut f::Cohort,
    log: &mut RedbLogStore,
    facts: (([u8; 32], usize, usize), ([u8; 32], usize, usize)),
    old: (&[u8], &[u8]),
) -> TestResult {
    let s = f::scope()?;
    let caps = f::limits();
    let frame = f::public_frame(
        &s,
        &RequestId::new(b"original".to_vec())?,
        2,
        b"job",
        Raw::Map(&[]),
        Raw::List(&[]),
        &mut PolicySchemaCharge::new(caps.schema),
    )?;
    let response = f::etf_policy(1, b"job", &[(b"states", &f::etf_map(&[])?)])?;
    f::rows_mut(expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), response.clone());
    let metadata = f::metadata(
        &s,
        b"original",
        2,
        2,
        f::frame_intent(&frame, 2)?,
        RetainedStatus::Accepted,
    )?;
    f::finish(
        expected,
        &s,
        &frame,
        f::log(2),
        (f::clock(2)?, f::clock(2)?),
        Some((&metadata, 1, &response)),
    )?;
    f::upgrade6(
        expected,
        &s,
        f::catalog(&s, [Some(facts.0), Some(facts.1)], Some(1), Some(0), None)?,
    )?;
    let mut state = f::open(path, 64)?;
    assert!(
        matches!(f::apply(&mut state,&frame,2,caps,&mut PolicySchemaCharge::new(caps.schema))?,
        RecordBatchTransactionOutcome::Retained{outcome,..}if outcome.response()==response)
    );
    for (id, wanted) in [(facts.0.0, old.0), (facts.1.0, old.1)] {
        let mut actual = Vec::new();
        assert!(
            state
                .snapshot_by_id(id, &mut actual, SnapshotLimits::default())?
                .is_some()
        );
        assert_eq!(actual, wanted);
    }
    drop(state);
    assert_eq!(f::cohort(path)?, *expected);
    log.append(vec![RaftLogEntry::new(f::log(2), frame)])?;
    log.save_committed(f::log(2))?;
    Ok(())
}

fn lineage_transitions(
    path: &Path,
    expected: &mut f::Cohort,
    log: &mut RedbLogStore,
    facts: (([u8; 32], usize, usize), ([u8; 32], usize, usize)),
) -> TestResult {
    let s = f::scope()?;
    let mut state = f::open(path, 64)?;
    let transition = SnapshotTransition::local(log.state()?, f::log(1), facts.1.0)?;
    state.prepare_snapshot_transition(transition, SnapshotLimits::default())?;
    log.purge_through(f::log(1), facts.1.0)?;
    state.finish_snapshot_transition(log.state()?, SnapshotLimits::default())?;
    drop(state);
    expected[2] = Some(BTreeMap::new());
    expected[3] = Some(BTreeMap::new());
    expected[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&s, [None, Some(facts.1)], Some(1), Some(1), None)?,
    );
    assert_eq!(f::cohort(path)?, *expected);
    let wire = f::stream3(expected)?;
    let current = (
        Sha256::digest(&wire).into(),
        3 + f::rows(expected)?.len(),
        wire.len(),
    );
    state = f::open(path, 64)?;
    let mut bytes = Vec::new();
    assert_eq!(
        state
            .capture_snapshot(&mut bytes, SnapshotLimits::default())?
            .id(),
        &current.0
    );
    assert_eq!(bytes, wire);
    frozen(expected, 0)?;
    expected[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&s, [Some(current), Some(facts.1)], Some(0), Some(1), None)?,
    );
    let transition = SnapshotTransition::local(log.state()?, f::log(2), current.0)?;
    state.prepare_snapshot_transition(transition, SnapshotLimits::default())?;
    expected[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(
            &s,
            [Some(current), Some(facts.1)],
            Some(0),
            Some(1),
            Some(transition),
        )?,
    );
    drop(state);
    assert_eq!(f::cohort(path)?, *expected);
    state = f::open(path, 64)?;
    refuse_pending_publication(&mut state, transition)?;
    log.purge_through(f::log(2), current.0)?;
    state.finish_snapshot_transition(log.state()?, SnapshotLimits::default())?;
    assert_eq!(
        state.snapshot_pin(SnapshotLimits::default())?,
        Some(current.0)
    );
    for id in [facts.0.0, facts.1.0] {
        assert!(
            state
                .snapshot_by_id(id, &mut Vec::new(), SnapshotLimits::default())?
                .is_none()
        );
    }
    drop(state);
    expected[4] = Some(BTreeMap::new());
    expected[5] = Some(BTreeMap::new());
    expected[0].as_mut().ok_or("no metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&s, [Some(current), None], Some(0), Some(0), None)?,
    );
    assert_eq!(f::cohort(path)?, *expected);
    drop(f::open(path, 64)?);
    Ok(())
}

fn lineage_complete(
    path: &Path,
    expected: &mut f::Cohort,
    log: &mut RedbLogStore,
    facts3: ([u8; 32], usize, usize),
) -> TestResult {
    let s = f::scope()?;
    let caps = f::limits();
    let core = core_frame()?;
    let mut snapshot = b"FWPS\0\x02".to_vec();
    f::raw(&mut snapshot, b"complete")?;
    snapshot.extend_from_slice(&[0; 4]);
    snapshot.extend_from_slice(&0_u32.to_be_bytes());
    snapshot.extend_from_slice(&[0; 3]);
    let mut value = b"FWPV\0\x02".to_vec();
    value.extend_from_slice(&1_u64.to_be_bytes());
    f::raw(&mut value, &snapshot)?;
    f::rows_mut(expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"complete"]), value.clone());
    let intent = verify_complete_core_policy_patch_frame(
        core.as_bytes(),
        caps.schema,
        caps.codec.admission(),
        &mut PolicySchemaCharge::new(caps.schema),
    )?
    .header(f::log(1))
    .intent;
    let metadata = f::metadata(&s, b"core", 1, 1, intent, RetainedStatus::Accepted)?;
    f::finish(
        expected,
        &s,
        core.as_bytes(),
        f::log(1),
        (f::clock(1)?, f::clock(1)?),
        Some((&metadata, 1, &value)),
    )?;
    upgrade5(
        expected,
        f::catalog(&s, [Some(facts3), None], Some(0), Some(0), None)?,
    )?;
    let mut state = f::open(path, 64)?;
    assert!(
        matches!(apply_complete_core_policy_patch_transaction(&mut state,&s,core.as_bytes(),f::log(1),caps)?,
        RecordBatchTransactionOutcome::Retained{outcome,..}if outcome.response()==value)
    );
    drop(state);
    assert_eq!(f::cohort(path)?, *expected);
    log.append(vec![RaftLogEntry::new(f::log(1), core.as_bytes().to_vec())])?;
    log.save_committed(f::log(1))?;
    Ok(())
}

fn refuse_pending_publication(
    state: &mut RedbStateStore,
    transition: SnapshotTransition,
) -> TestResult {
    let s = f::scope()?;
    let caps = f::limits();
    let pending = f::public_frame(
        &s,
        &RequestId::new(b"pending".to_vec())?,
        3,
        b"job",
        Raw::Map(&[]),
        Raw::List(&[]),
        &mut PolicySchemaCharge::new(caps.schema),
    )?;
    assert_eq!(
        apply_original_policy_set_transaction(
            state,
            &s,
            &pending,
            f::log(3),
            caps,
            &mut PolicySchemaCharge::new(caps.schema)
        ),
        Err(OriginalPolicyTransactionError::Transaction(
            StateTransactionError::InvalidChanges
        ))
    );
    assert_eq!(
        state.snapshot_transition(SnapshotLimits::default())?,
        Some(transition)
    );
    Ok(())
}
