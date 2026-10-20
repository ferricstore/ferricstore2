//! Actual frozen 3/5/6 cohorts, independent streams and received custody.
use super::{TestResult, f, w};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Cursor, path::Path};
use wf_domain::policy::complete::{PolicySchemaCharge, RawFlowPolicyValue as Raw};
use wf_domain::policy::definition::PolicyPatch;
use wf_domain::workflow::request::RetainedStatus;
use wf_ports::log::{RaftLogEntry, RaftLogLimits, RaftLogStore};
use wf_ports::snapshot::{
    SnapshotError, SnapshotLimits, SnapshotTransition, StateSnapshotStore,
    StateSnapshotTransitionStore,
};
use wf_ports::state::StateTransactionError;
use wf_storage::transaction::{
    RecordBatchTransactionOutcome, apply_complete_core_policy_patch_transaction,
};
use wf_storage::{log::RedbLogStore, snapshot::RedbSnapshotStage};
use wf_types::identity::RequestId;

fn minimal(generation: u64) -> TestResult<Vec<u8>> {
    let mut snapshot = b"FWPS\0\x02".to_vec();
    f::raw(&mut snapshot, b"job")?;
    snapshot.extend_from_slice(&[0; 4]);
    snapshot.extend_from_slice(&0_u32.to_be_bytes());
    snapshot.extend_from_slice(&[0; 3]);
    let mut bytes = b"FWPV\0\x02".to_vec();
    bytes.extend_from_slice(&generation.to_be_bytes());
    f::raw(&mut bytes, &snapshot)?;
    Ok(bytes)
}
fn seeded(path: &Path) -> TestResult<Vec<u8>> {
    let scope = f::scope()?;
    let mut store = f::create(path, &[(b"job".to_vec(), minimal(1)?)], 64)?;
    let row = f::owner(
        &scope,
        b"p",
        f::data(b"x", b"foreign", b"q", 0, Some(0))?,
        0,
        0,
    )?;
    f::seed(&mut store, &scope, &[row.clone()], 0, 0)?;
    drop(store);
    f::batch(
        &scope,
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
    )
    .map(|frame| frame.as_bytes().to_vec())
}
fn legacy_descriptor(facts: ([u8; 32], usize, usize)) -> TestResult<Vec<u8>> {
    let mut out = b"FWSD\0\x02\x01".to_vec();
    out.extend_from_slice(&u64::try_from(facts.1)?.to_be_bytes());
    out.extend_from_slice(&u64::try_from(facts.2)?.to_be_bytes());
    out.extend_from_slice(&facts.0);
    let mut hash = Sha256::new();
    hash.update(b"wf-state-snapshot-descriptor-v1");
    for field in [b"g".as_slice(), b"z"] {
        hash.update(u16::try_from(field.len())?.to_be_bytes());
        hash.update(field);
    }
    hash.update(&out);
    out.extend_from_slice(&hash.finalize());
    Ok(out)
}
fn freeze(expected: &mut f::Cohort, slot: usize) -> TestResult {
    let mut metadata = expected[0].as_ref().ok_or("missing live metadata")?.clone();
    metadata
        .remove(b"snapshot".as_slice())
        .ok_or("missing descriptor")?;
    expected[2 + 2 * slot] = Some(metadata);
    expected[3 + 2 * slot] = Some(f::rows(expected)?.clone());
    Ok(())
}
fn upgrade5(expected: &mut f::Cohort, descriptor: Vec<u8>) -> TestResult {
    let mut hash = Sha256::new();
    hash.update(b"wf-state-current-catalog-v1");
    let mut count = 0;
    for (key, value) in f::rows(expected)?
        .iter()
        .filter(|(key, _)| matches!(key.get(6), Some(6 | 7)))
    {
        if key[6] == 6 {
            count += 1;
        }
        hash.update(u64::try_from(key.len())?.to_be_bytes());
        hash.update(key);
        hash.update(u64::try_from(value.len())?.to_be_bytes());
        hash.update(value);
    }
    let origin = expected[0].as_ref().ok_or("missing metadata")?[b"manifest".as_slice()].clone();
    let mut manifest = f::manifest6(&origin, (1, count), hash.finalize().into())?;
    manifest.drain(110..114);
    manifest[4..6].copy_from_slice(&5_u16.to_be_bytes());
    f::rehash(&mut manifest)?;
    let metadata = expected[0].as_mut().ok_or("missing metadata")?;
    metadata.insert(b"origin".to_vec(), origin);
    metadata.insert(b"manifest".to_vec(), manifest);
    metadata.insert(b"snapshot".to_vec(), descriptor);
    expected[4] = Some(BTreeMap::new());
    expected[5] = Some(BTreeMap::new());
    Ok(())
}
fn append(log: &mut RedbLogStore, bytes: &[u8], at: u64) -> TestResult {
    log.append(vec![RaftLogEntry::new(f::log(at), bytes.to_vec())])?;
    log.save_committed(f::log(at))?;
    Ok(())
}
fn publish(
    expected: &mut f::Cohort,
    bytes: &[u8],
    at: u64,
    version: u16,
    body: &[u8],
    original: bool,
) -> TestResult {
    let intent = if original {
        f::frame_intent(bytes, at)?
    } else {
        w::core_intent(bytes, at)?
    };
    let request: &[u8] = match at {
        1 => b"core-v2",
        2 => b"set-v2",
        3 => b"core-etf",
        _ => return Err("unnamed snapshot request".into()),
    };
    let scope = f::scope()?;
    let metadata = f::metadata(&scope, request, at, at, intent, RetainedStatus::Accepted)?;
    f::rows_mut(expected)?.insert(f::key(6, &[b"g", b"z", b"t", b"job"]), body.to_vec());
    f::finish(
        expected,
        &scope,
        bytes,
        f::log(at),
        (f::clock(at)?, f::clock(at)?),
        Some((&metadata, version, body)),
    )
}
fn old_cohort_refuses_response2(dir: &Path, original: &f::Cohort, at: u64) -> TestResult {
    let path = dir.join(format!("old-layout-{at}"));
    drop(f::create(&path, &[(b"job".to_vec(), minimal(1)?)], 64)?);
    let mut cohort = original.clone();
    // Remove unrelated frozen artifacts so the LIVE parsed layout is the sole
    // discriminator. The healthy original cohort is actually opened first.
    for slot in &mut cohort[2..] {
        if slot.is_some() {
            *slot = Some(BTreeMap::new());
        }
    }
    let metadata = cohort[0].as_mut().ok_or("missing old metadata")?;
    if at == 1 {
        metadata.insert(
            b"snapshot".to_vec(),
            f::catalog(&f::scope()?, [None, None], None, None, None)?,
        );
    } else {
        let mut descriptor = b"FWSD\0\x01\0".to_vec();
        let mut hash = Sha256::new();
        hash.update(b"wf-state-snapshot-descriptor-v1");
        for field in [b"g".as_slice(), b"z"] {
            hash.update(u16::try_from(field.len())?.to_be_bytes());
            hash.update(field);
        }
        hash.update(&descriptor);
        descriptor.extend_from_slice(&hash.finalize());
        metadata.insert(b"snapshot".to_vec(), descriptor);
    }
    f::replace(&path, &cohort)?;
    drop(f::open(&path, 64)?);
    let scope = f::scope()?;
    let bytes = w::core_frame(b"receipt2", at, Some(1), PolicyPatch::default())?;
    let metadata = f::metadata(
        &scope,
        b"receipt2",
        at,
        at,
        w::core_intent(&bytes, at)?,
        RetainedStatus::Accepted,
    )?;
    let response = f::etf_policy(1, b"job", &[(b"states", &f::etf_map(&[])?)])?;
    let mut wire = b"FWSO\0\x01".to_vec();
    for field in [b"t".as_slice(), b"g", b"z", b"receipt2"] {
        f::raw(&mut wire, field)?;
    }
    wire.extend_from_slice(&2_u16.to_be_bytes());
    wire.extend_from_slice(metadata.intent.digest());
    for n in [1, 7, at, at, at + 1000] {
        wire.extend_from_slice(&n.to_be_bytes());
    }
    wire.extend_from_slice(&2_u16.to_be_bytes());
    f::raw(&mut wire, &response)?;
    f::rows_mut(&mut cohort)?.insert(f::key(2, &[b"g", b"z", b"t", b"receipt2"]), wire);
    let positive_path = dir.join(format!("response2-layout6-{at}"));
    drop(f::create(
        &positive_path,
        &[(b"job".to_vec(), minimal(1)?)],
        64,
    )?);
    let mut positive = cohort.clone();
    f::upgrade6(
        &mut positive,
        &scope,
        f::catalog(&scope, [None, None], None, None, None)?,
    )?;
    f::replace(&positive_path, &positive)?;
    drop(f::open(&positive_path, 64)?);
    assert_eq!(f::cohort(&positive_path)?, positive);
    f::replace(&path, &cohort)?;
    assert!(
        matches!(f::open(&path, 64), Err(error) if error.downcast_ref::<StateTransactionError>() == Some(&StateTransactionError::Corrupt))
    );
    assert_eq!(f::cohort(&path)?, cohort);
    Ok(())
}
fn lineage(dir: &Path) -> TestResult<(f::Cohort, Vec<u8>)> {
    let path = dir.join("source");
    let seed = seeded(&path)?;
    let scope = f::scope()?;
    let mut log = RedbLogStore::create(
        &dir.join("log"),
        scope.group().clone(),
        scope.recovery().clone(),
        RaftLogLimits::default(),
    )?;
    append(&mut log, &seed, 0)?;
    let mut expected = f::cohort(&path)?;
    let wire3 = f::snapshot_wire(&scope, &expected, 1)?;
    let facts3 = (
        Sha256::digest(&wire3).into(),
        2 + f::rows(&expected)?.len(),
        wire3.len(),
    );
    let mut store = f::open(&path, 64)?;
    let mut actual = Vec::new();
    assert_eq!(
        store
            .capture_snapshot(&mut actual, SnapshotLimits::default())?
            .id(),
        &facts3.0
    );
    assert_eq!(actual, wire3);
    store.pin_snapshot(facts3.0, SnapshotLimits::default())?;
    log.purge_through(f::log(0), facts3.0)?;
    drop(store);
    freeze(&mut expected, 0)?;
    expected[0]
        .as_mut()
        .ok_or("metadata")?
        .insert(b"snapshot".to_vec(), legacy_descriptor(facts3)?);
    assert_eq!(f::cohort(&path)?, expected);
    old_cohort_refuses_response2(dir, &expected, 0)?;
    let bytes = w::core_frame(b"core-v2", 1, Some(1), PolicyPatch::default())?;
    let caps = f::limits();
    store = f::open(&path, 64)?;
    assert!(
        matches!(apply_complete_core_policy_patch_transaction(&mut store, &scope, &bytes, f::log(1), caps)?, RecordBatchTransactionOutcome::Retained { outcome, .. } if outcome.response_version() == 1 && outcome.response() == minimal(2)?)
    );
    publish(&mut expected, &bytes, 1, 1, &minimal(2)?, false)?;
    upgrade5(
        &mut expected,
        f::catalog(&scope, [Some(facts3), None], Some(0), Some(0), None)?,
    )?;
    drop(store);
    assert_eq!(f::cohort(&path)?, expected);
    append(&mut log, &bytes, 1)?;
    let wire5 = f::snapshot_wire(&scope, &expected, 2)?;
    let facts5 = (
        Sha256::digest(&wire5).into(),
        3 + f::rows(&expected)?.len(),
        wire5.len(),
    );
    store = f::open(&path, 64)?;
    actual.clear();
    assert_eq!(
        store
            .capture_snapshot(&mut actual, SnapshotLimits::default())?
            .id(),
        &facts5.0
    );
    assert_eq!(actual, wire5);
    drop(store);
    freeze(&mut expected, 1)?;
    expected[0].as_mut().ok_or("metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&scope, [Some(facts3), Some(facts5)], Some(1), Some(0), None)?,
    );
    assert_eq!(f::cohort(&path)?, expected);
    old_cohort_refuses_response2(dir, &expected, 1)?;
    original_and_core(
        &path,
        &mut expected,
        &mut log,
        facts3,
        facts5,
        (&wire3, &wire5),
    )?;
    retire_and_capture(&path, expected, &mut log, facts3, facts5)
}
fn original_and_core(
    path: &Path,
    expected: &mut f::Cohort,
    log: &mut RedbLogStore,
    facts3: ([u8; 32], usize, usize),
    facts5: ([u8; 32], usize, usize),
    old: (&[u8], &[u8]),
) -> TestResult {
    let scope = f::scope()?;
    let caps = f::limits();
    let bytes = f::public_frame(
        &scope,
        &RequestId::new(b"set-v2".to_vec())?,
        2,
        b"job",
        Raw::Map(&[]),
        Raw::List(&[]),
        &mut PolicySchemaCharge::new(caps.schema),
    )?;
    let response = f::etf_policy(3, b"job", &[(b"states", &f::etf_map(&[])?)])?;
    let mut store = f::open(path, 64)?;
    assert!(
        matches!(f::apply(&mut store, &bytes, 2, caps, &mut PolicySchemaCharge::new(caps.schema))?, RecordBatchTransactionOutcome::Retained { outcome, .. } if outcome.response() == response)
    );
    publish(expected, &bytes, 2, 1, &response, true)?;
    f::upgrade6(
        expected,
        &scope,
        f::catalog(&scope, [Some(facts3), Some(facts5)], Some(1), Some(0), None)?,
    )?;
    drop(store);
    assert_eq!(f::cohort(path)?, *expected);
    append(log, &bytes, 2)?;
    let bytes = w::core_frame(b"core-etf", 3, Some(3), PolicyPatch::default())?;
    let response = f::etf_policy(4, b"job", &[(b"states", &f::etf_map(&[])?)])?;
    store = f::open(path, 64)?;
    assert!(
        matches!(apply_complete_core_policy_patch_transaction(&mut store, &scope, &bytes, f::log(3), caps)?, RecordBatchTransactionOutcome::Retained { outcome, .. } if outcome.response_version() == 2 && outcome.response() == response)
    );
    publish(expected, &bytes, 3, 2, &response, false)?;
    f::upgrade6(
        expected,
        &scope,
        f::catalog(&scope, [Some(facts3), Some(facts5)], Some(1), Some(0), None)?,
    )?;
    for (id, wanted) in [(facts3.0, old.0), (facts5.0, old.1)] {
        let mut bytes = Vec::new();
        assert!(
            store
                .snapshot_by_id(id, &mut bytes, SnapshotLimits::default())?
                .is_some()
        );
        assert_eq!(bytes, wanted);
    }
    drop(store);
    assert_eq!(f::cohort(path)?, *expected);
    append(log, &bytes, 3)
}
fn retire_and_capture(
    path: &Path,
    mut expected: f::Cohort,
    log: &mut RedbLogStore,
    facts3: ([u8; 32], usize, usize),
    facts5: ([u8; 32], usize, usize),
) -> TestResult<(f::Cohort, Vec<u8>)> {
    let scope = f::scope()?;
    let mut store = f::open(path, 64)?;
    let transition = SnapshotTransition::local(log.state()?, f::log(1), facts5.0)?;
    store.prepare_snapshot_transition(transition, SnapshotLimits::default())?;
    log.purge_through(f::log(1), facts5.0)?;
    store.finish_snapshot_transition(log.state()?, SnapshotLimits::default())?;
    expected[2] = Some(BTreeMap::new());
    expected[3] = Some(BTreeMap::new());
    expected[0].as_mut().ok_or("metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&scope, [None, Some(facts5)], Some(1), Some(1), None)?,
    );
    let wire = f::stream3(&expected)?;
    let facts = (
        Sha256::digest(&wire).into(),
        3 + f::rows(&expected)?.len(),
        wire.len(),
    );
    let mut actual = Vec::new();
    assert_eq!(
        store
            .capture_snapshot(&mut actual, SnapshotLimits::default())?
            .id(),
        &facts.0
    );
    assert_eq!(actual, wire);
    freeze(&mut expected, 0)?;
    expected[0].as_mut().ok_or("metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&scope, [Some(facts), Some(facts5)], Some(0), Some(1), None)?,
    );
    let transition = SnapshotTransition::local(log.state()?, f::log(3), facts.0)?;
    store.prepare_snapshot_transition(transition, SnapshotLimits::default())?;
    drop(store);
    expected[0].as_mut().ok_or("metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(
            &scope,
            [Some(facts), Some(facts5)],
            Some(0),
            Some(1),
            Some(transition),
        )?,
    );
    assert_eq!(f::cohort(path)?, expected);
    store = f::open(path, 64)?;
    assert_eq!(
        store.snapshot_transition(SnapshotLimits::default())?,
        Some(transition)
    );
    log.purge_through(f::log(3), facts.0)?;
    store.finish_snapshot_transition(log.state()?, SnapshotLimits::default())?;
    expected[4] = Some(BTreeMap::new());
    expected[5] = Some(BTreeMap::new());
    expected[0].as_mut().ok_or("metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&scope, [Some(facts), None], Some(0), Some(0), None)?,
    );
    for id in [facts3.0, facts5.0] {
        assert!(
            store
                .snapshot_by_id(id, &mut Vec::new(), SnapshotLimits::default())?
                .is_none()
        );
    }
    assert_eq!(
        store.snapshot_pin(SnapshotLimits::default())?,
        Some(facts.0)
    );
    drop(store);
    assert_eq!(f::cohort(path)?, expected);
    Ok((expected, wire))
}
fn stage(path: &Path, bytes: &[u8]) -> TestResult<RedbSnapshotStage> {
    let scope = f::scope()?;
    Ok(RedbSnapshotStage::receive(
        path,
        scope.group().clone(),
        scope.recovery().clone(),
        &mut Cursor::new(bytes),
        f::store_limits(64)?,
        SnapshotLimits::default(),
    )?)
}
#[test]
fn interop_mixed_snapshot_lineage_and_received_install_keep_old_formats() -> TestResult {
    let dir = f::directory("interop-lineage")?;
    let (expected, wire) = lineage(&dir)?;
    assert_eq!(&wire[..6], b"FWSP\0\x03");
    let scope = f::scope()?;
    let id: [u8; 32] = Sha256::digest(&wire).into();
    let target = dir.join("install");
    drop(f::create(&target, &[(b"job".to_vec(), minimal(1)?)], 64)?);
    let mut active = f::open(&target, 64)?;
    assert_eq!(
        active
            .install_snapshot(stage(&dir.join("stage"), &wire)?, SnapshotLimits::default())?
            .id(),
        &id
    );
    assert!(active.supports_snapshot_transitions()?);
    drop(active);
    let mut installed = expected.clone();
    installed[0].as_mut().ok_or("metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(
            &scope,
            [Some((id, 3 + f::rows(&expected)?.len(), wire.len())), None],
            Some(0),
            None,
            None,
        )?,
    );
    assert_eq!(f::cohort(&target)?, installed);
    drop(f::open(&target, 64)?);
    for cut in [6, wire.len() - 1] {
        let path = dir.join(format!("truncated-{cut}"));
        assert!(
            matches!(stage(&path, &wire[..cut]), Err(error) if error.downcast_ref::<SnapshotError>() == Some(&SnapshotError::Malformed))
        );
        use redb::ReadableDatabase;
        let database = redb::Builder::new().open(&path)?;
        assert_eq!(database.begin_read()?.list_tables()?.count(), 0);
    }
    super::faults::snapshot_cuts(&dir, &wire, &installed, &minimal(1)?)?;
    received(&dir, &expected, &wire)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
fn received(dir: &Path, expected: &f::Cohort, wire: &[u8]) -> TestResult {
    let scope = f::scope()?;
    let target = dir.join("received");
    let seed = seeded(&target)?;
    let mut active = f::open(&target, 64)?;
    active = active.upgrade_snapshot_layout(SnapshotLimits::default())?;
    let mut log = RedbLogStore::create(
        &dir.join("received-log"),
        scope.group().clone(),
        scope.recovery().clone(),
        RaftLogLimits::default(),
    )?;
    append(&mut log, &seed, 0)?;
    let id: [u8; 32] = Sha256::digest(wire).into();
    let facts = (id, 3 + f::rows(expected)?.len(), wire.len());
    let transition = SnapshotTransition::received(log.state()?, f::log(3), id)?;
    active.prepare_received_snapshot(
        stage(&dir.join("received-stage"), wire)?,
        transition,
        SnapshotLimits::default(),
    )?;
    drop(active);
    let mut pending = expected.clone();
    pending[0].as_mut().ok_or("metadata")?.insert(
        b"snapshot".to_vec(),
        f::catalog(&scope, [Some(facts), None], Some(0), None, Some(transition))?,
    );
    assert_eq!(f::cohort(&target)?, pending);
    active = f::open(&target, 64)?;
    assert_eq!(
        active.snapshot_transition(SnapshotLimits::default())?,
        Some(transition)
    );
    let bytes = w::core_frame(b"pending", 4, Some(4), PolicyPatch::default())?;
    assert!(matches!(
        apply_complete_core_policy_patch_transaction(
            &mut active,
            &scope,
            &bytes,
            f::log(4),
            f::limits()
        ),
        Err(
            wf_storage::transaction::CorePolicyTransactionError::Transaction(
                StateTransactionError::InvalidChanges
            )
        )
    ));
    let accepted_frame = w::core_frame(b"core-etf", 3, Some(3), PolicyPatch::default())?;
    log.install_snapshot_boundary(
        transition.before(),
        f::log(3),
        Sha256::digest(&accepted_frame).into(),
        id,
    )?;
    active.finish_snapshot_transition(log.state()?, SnapshotLimits::default())?;
    assert_eq!(active.snapshot_pin(SnapshotLimits::default())?, Some(id));
    let mut bytes = Vec::new();
    assert_eq!(
        active
            .current_snapshot(&mut bytes, SnapshotLimits::default())?
            .ok_or("received artifact missing")?
            .id(),
        &id
    );
    assert_eq!(bytes, wire);
    drop(active);
    assert_eq!(f::cohort(&target)?, *expected);
    drop(f::open(&target, 64)?);
    Ok(())
}
