//! Independent framing and complete physical cohorts; these maps are test owners.

use std::{collections::BTreeMap, io::Cursor, path::Path};

use redb::{Durability, ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::{Create, Operation};
use wf_ports::snapshot::{SnapshotLimits, StateSnapshotStore};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits, key, value};
use wf_storage::transaction::{RecordBatchTransactionOutcome, apply_record_batch_transaction};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{Fence, TimestampMillis};

use super::super::{bootstrap, control, recovery};
use super::{RawCohort, RedbSnapshotStage, TestResult, f};

pub(super) const META: &str = "wf_state_meta_v1";
pub(super) const ROWS: &str = "wf_state_rows_v1";
pub(super) const SNAP_META: &str = "wf_state_snapshot_meta_v1";
pub(super) const SNAP_ROWS: &str = "wf_state_snapshot_rows_v1";
pub(super) const NONE: &str =
    "465753440001009d43121445d1b3454f9c1006acb6e7103e4b9b348af955c9321e35fb1c29c189";
pub(super) const SOME: &str = "46575344000101000000000000000300000000000001451e4bc7621eeb5aba5567c8fc9c2470d758faf7cd7663e4eb4c156370dc7a8080f3a00fff0d1a91517f4eb7b017b1d9ae8b0073ae606fcd5c3f8e6396def7d2b8";
pub(super) const INITIAL: &str = "46575350000100016700017a00000000000000020000000000000001000000000000014500000008000000000000006e6d616e696665737446575353000300016700017a00000000000000000000000000000000000000000000000000000000000000000000b73e51559c5ba504491174aa43b682d142e85ebcbdefa7e8946f730b4617c70e04ca2dd63fd23dd6bba7ea51d2fa2860beac33da639d8d7a11502bb6bb3b190e0000000a000000000000002d6d656d6265727368697046574d56000100016700017a00ac790aa1bf45989cda350d1a756194a344fee55f0cf4169b6e72190759ebeecc0000000d00000000000000234657504b0001006700007a00004657534300010000000167000000017a0000000000000000000000000000000000000011971a22b41b765d3fe04fa4a569f1321b2dd174857b12dcccf0093c072cee43";

pub(super) fn hex(value: &str) -> TestResult<Vec<u8>> {
    control::fixtures::hex(value)
}

pub(super) fn empty(path: &Path) -> TestResult<RedbStateStore> {
    let s = f::scope()?;
    Ok(RedbStateStore::create(
        path,
        s.group().clone(),
        s.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &[],
            policies: &[],
        },
        StateStoreLimits::default(),
    )?)
}

pub(super) fn create(path: &Path) -> TestResult<RedbStateStore> {
    let s = f::scope()?;
    let placements = [
        (PartitionId::new(b"p".to_vec())?, s.group().clone()),
        (
            PartitionId::new(b"x".to_vec())?,
            GroupId::new(b"foreign".to_vec())?,
        ),
    ];
    let policies = [(s.tenant().clone(), f::rich_policy()?)];
    Ok(RedbStateStore::create(
        path,
        s.group().clone(),
        s.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placements,
            policies: &policies,
        },
        StateStoreLimits::default(),
    )?)
}

pub(super) fn raw(path: &Path) -> TestResult<RawCohort> {
    let db = redb::Builder::new().open(path)?;
    let read = db.begin_read()?;
    assert_eq!(read.list_multimap_tables()?.count(), 0);
    let mut result = BTreeMap::new();
    for name in read.list_tables()? {
        let name = name.name().to_owned();
        let table = read.open_table(TableDefinition::<&[u8], &[u8]>::new(&name))?;
        let mut rows = BTreeMap::new();
        for row in table.iter()? {
            let (k, v) = row?;
            assert!(
                rows.insert(k.value().to_vec(), v.value().to_vec())
                    .is_none()
            );
        }
        assert!(result.insert(name, rows).is_none());
    }
    Ok(result)
}

pub(super) fn replace(path: &Path, values: &RawCohort) -> TestResult {
    let db = redb::Builder::new().open(path)?;
    let mut tx = db.begin_write()?;
    tx.set_durability(Durability::Immediate)?;
    let names = tx
        .list_tables()?
        .map(|t| t.name().to_owned())
        .collect::<Vec<_>>();
    for name in names {
        if !values.contains_key(&name) {
            tx.delete_table(TableDefinition::<&[u8], &[u8]>::new(&name))?;
        }
    }
    for (name, rows) in values {
        let mut table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        table.retain(|_, _| false)?;
        for (k, v) in rows {
            table.insert(k.as_slice(), v.as_slice())?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub(super) fn authority(data: &RawCohort) -> TestResult<RawCohort> {
    let mut meta = data.get(META).ok_or("no live metadata")?.clone();
    assert!(meta.remove(b"snapshot".as_slice()).is_some());
    assert_eq!(meta.len(), 2);
    Ok(BTreeMap::from([
        (META.to_owned(), meta),
        (
            ROWS.to_owned(),
            data.get(ROWS).ok_or("no live rows")?.clone(),
        ),
    ]))
}

pub(super) fn frame(scope: (&[u8], &[u8]), data: &RawCohort) -> TestResult<Vec<u8>> {
    let meta = data.get(META).ok_or("no frame metadata")?;
    let rows = data.get(ROWS).ok_or("no frame rows")?;
    let count = meta.len() + rows.len();
    let length = 66
        + scope.0.len()
        + scope.1.len()
        + 12 * count
        + meta
            .iter()
            .chain(rows)
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>();
    let mut out = b"FWSP\0\x01".to_vec();
    for field in [scope.0, scope.1] {
        out.extend_from_slice(&u16::try_from(field.len())?.to_be_bytes());
        out.extend_from_slice(field);
    }
    for n in [meta.len(), rows.len(), length] {
        out.extend_from_slice(&u64::try_from(n)?.to_be_bytes());
    }
    for (k, v) in meta.iter().chain(rows) {
        out.extend_from_slice(&u32::try_from(k.len())?.to_be_bytes());
        out.extend_from_slice(&u64::try_from(v.len())?.to_be_bytes());
        out.extend_from_slice(k);
        out.extend_from_slice(v);
    }
    let sha = Sha256::digest(&out);
    out.extend_from_slice(&sha);
    assert_eq!(out.len(), length);
    Ok(out)
}

pub(super) fn descriptor(scope: (&[u8], &[u8]), count: usize, wire: &[u8]) -> TestResult<Vec<u8>> {
    let mut prefix = b"FWSD\0\x01\x01".to_vec();
    prefix.extend_from_slice(&u64::try_from(count)?.to_be_bytes());
    prefix.extend_from_slice(&u64::try_from(wire.len())?.to_be_bytes());
    prefix.extend_from_slice(&Sha256::digest(wire));
    let mut sha = Sha256::new();
    sha.update(b"wf-state-snapshot-descriptor-v1");
    for field in [scope.0, scope.1] {
        sha.update(u16::try_from(field.len())?.to_be_bytes());
        sha.update(field);
    }
    sha.update(&prefix);
    prefix.extend_from_slice(&sha.finalize());
    assert_eq!(prefix.len(), 87);
    Ok(prefix)
}

pub(super) fn installed(data: &RawCohort) -> TestResult<RawCohort> {
    let mut result = data.clone();
    let wire = frame((b"g", b"z"), data)?;
    let count = data.values().map(BTreeMap::len).sum();
    result.get_mut(META).ok_or("no live metadata")?.insert(
        b"snapshot".to_vec(),
        descriptor((b"g", b"z"), count, &wire)?,
    );
    result.insert(
        SNAP_META.to_owned(),
        data.get(META).ok_or("no metadata")?.clone(),
    );
    result.insert(
        SNAP_ROWS.to_owned(),
        data.get(ROWS).ok_or("no rows")?.clone(),
    );
    Ok(result)
}

pub(super) fn capture(path: &Path) -> TestResult<Vec<u8>> {
    let mut state = f::open(path)?;
    let mut out = Vec::new();
    let meta = state.capture_snapshot(&mut out, SnapshotLimits::default())?;
    assert_eq!(meta.id().as_slice(), Sha256::digest(&out).as_slice());
    assert_eq!(meta.encoded_bytes(), out.len());
    Ok(out)
}

pub(super) fn stage(path: &Path, wire: &[u8]) -> TestResult<RedbSnapshotStage> {
    let s = f::scope()?;
    Ok(RedbSnapshotStage::receive(
        path,
        s.group().clone(),
        s.recovery().clone(),
        &mut Cursor::new(wire),
        StateStoreLimits::default(),
        SnapshotLimits::default(),
    )?)
}

pub(super) fn assert_raw(path: &Path, expected: &RawCohort) -> TestResult {
    assert_eq!(raw(path)?, *expected);
    drop(f::open(path)?);
    assert_eq!(raw(path)?, *expected);
    Ok(())
}

pub(super) fn apply(state: &mut RedbStateStore, id: &[u8], index: u64, now: u64) -> TestResult {
    let frame = f::frame(id, now, vec![(id, f::create_op())])?;
    let result = apply_record_batch_transaction(
        state,
        &f::scope()?,
        frame.as_bytes(),
        f::log(index),
        f::driver_limits(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("create was not retained".into());
    };
    assert_eq!(outcome.response(), [0]);
    Ok(())
}

pub(super) fn seed(path: &Path) -> TestResult<RawCohort> {
    drop(create(path)?);
    let records = [
        recovery::running()?,
        f::record(b"b", 2, Some(20))?,
        f::record(b"c", 3, Some(30))?,
        recovery::altered(b"parallel", b"z", 4, Some(40))?,
        recovery::altered(b"terminal", b"completed", 5, Some(5))?,
    ];
    let mut rows = recovery::cohort(&f::scope()?, f::raw_rows(path)?, &records, true)?;
    let s = f::scope()?;
    let history = HistoryEntry {
        event_id: "8-1".into(),
        kind: EventKind::Created,
        occurred_at: TimestampMillis::new(1),
        version: 1,
        fence: Fence::new(0),
        attempts: 0,
        state: b"q".to_vec(),
        next_run_at: Some(TimestampMillis::new(10)),
        lease_deadline: TimestampMillis::new(0),
        worker: None,
        retry: None,
    };
    let p = PartitionId::new(b"p".to_vec())?;
    let gone = RecordId::new(b"gone".to_vec())?;
    rows.insert(
        key::encode_history_key(&s, &p, &gone, &history, f::MIB)?,
        value::encode_history(&s, &p, &gone, &history, f::MIB)?,
    );
    f::replace(path, &rows, &f::raw_manifest(path)?)?;
    let mut state = f::open(path)?;
    let member = control::fixtures::frame(true, control::fixtures::stamp(10, 3, 9)?)?;
    control::fixtures::apply(&mut state, &member, f::log(1))?;
    apply(&mut state, b"normal\0\xff", 2, 11)?;
    apply(&mut state, b"late-seed", 3, 12)?;
    drop(state);
    let data = raw(path)?;
    assert!((0..8).all(|family| data[ROWS].keys().any(|k| k.get(6) == Some(&family))));
    Ok(data)
}

pub(super) fn large(path: &Path) -> TestResult<StateStoreLimits> {
    let s = f::scope()?;
    let schema = wf_domain::policy::complete::PolicySchemaLimits::default();
    let policy = wf_domain::policy::complete::prepare_complete_policy_value_v2(
        bootstrap::candidate(b"r", Some(vec![0; 262_117]))?,
        schema,
        &mut wf_domain::policy::complete::PolicySchemaCharge::new(schema),
    )?;
    assert_eq!(policy.encoded().len(), 262_162);
    assert_eq!(
        Sha256::digest(policy.encoded()).as_slice(),
        hex("cb208b6f19986d3775ba67e2eba937b00e8bb356242567afcdb29c2cc0c0462a")?
    );
    let cap = StateStoreLimits::new(
        wf_domain::workflow::admission::AdmissionLimits::default(),
        schema,
        (1000, 128 * f::MIB),
        (1_000_000, 1024 * f::MIB, 10_000_000),
        512 * f::MIB,
        1024 * f::MIB,
    )?;
    let placement = f::placements()?;
    let policies = [(s.tenant().clone(), policy)];
    drop(RedbStateStore::create(
        path,
        s.group().clone(),
        s.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placement,
            policies: &policies,
        },
        cap,
    )?);
    let mut rows = f::raw_rows(path)?;
    f::step(
        &mut rows,
        b"large",
        1,
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: Some(vec![7; f::MIB]),
        }),
        0,
        1,
    )?;
    f::reindex(&mut rows)?;
    control::fixtures::set_control(
        &mut rows,
        ControlState::restored(
            f::clock(2)?,
            EntryIdentity::new(f::log(0), PreparedDigest::new([0x5a; 32])),
            EntryStamp::Stamped(f::clock(2)?),
        )?,
    )?;
    f::replace(path, &rows, &f::raw_manifest(path)?)?;
    Ok(cap)
}
