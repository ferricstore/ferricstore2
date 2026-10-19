//! Independent persisted cohorts, complete physical identities and index authority.

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};
use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder};
use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::policy::complete::{
    PolicySchemaCharge, PolicySchemaLimits, decode_complete_policy_value_v2,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_ports::state::{
    GroupApplyStore, GroupApplyTransaction, StateTransactionError, StoredWorkflowRecord,
};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits, key, value};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, TenantId};
use wf_types::version::{Fence, TimestampMillis};

use super::{TestResult, fixtures};

pub(super) fn entry(owner: &StoredWorkflowRecord) -> ClaimRecord<'_> {
    ClaimRecord {
        record: &owner.record,
        partition_key: owner.partition.as_bytes(),
        priority: owner.ordering.priority,
        state_enter_seq: owner.ordering.state_enter_seq,
    }
}

pub(super) fn altered(
    id: &[u8],
    state: &[u8],
    sequence: u128,
    due: Option<u64>,
) -> TestResult<StoredWorkflowRecord> {
    let mut owner = fixtures::record(id, sequence, due)?;
    owner.record = restore_record(
        WorkflowRecordData {
            id: RecordId::new(id.to_vec())?,
            workflow_type: b"r".to_vec(),
            state: state.to_vec(),
            version: 1,
            fence: Fence::new(0),
            attempts: 0,
            created_at: TimestampMillis::new(1),
            updated_at: TimestampMillis::new(1),
            next_run_at: due.map(TimestampMillis::new),
            run_state: None,
            lease: None,
            payload: Some(vec![0, 255]),
            result: None,
            error: None,
            latest_kind: EventKind::Created,
            latest_version: 1,
            latest_fence: Fence::new(0),
        },
        AdmissionLimits::default(),
    )?;
    Ok(owner)
}

pub(super) fn running() -> TestResult<StoredWorkflowRecord> {
    let mut owner = altered(b"a", b"running", 99, Some(50))?;
    owner.record = restore_record(
        WorkflowRecordData {
            id: RecordId::new(b"a".to_vec())?,
            workflow_type: b"r".to_vec(),
            state: b"running".to_vec(),
            version: 2,
            fence: Fence::new(1),
            attempts: 1,
            created_at: TimestampMillis::new(1),
            updated_at: TimestampMillis::new(2),
            next_run_at: Some(TimestampMillis::new(50)),
            run_state: Some(b"q".to_vec()),
            lease: Some(Lease {
                worker: b"w".to_vec(),
                token: LeaseToken::new(b"token".to_vec())?,
                deadline: TimestampMillis::new(50),
            }),
            payload: Some(vec![0, 255]),
            result: None,
            error: None,
            latest_kind: EventKind::Claimed,
            latest_version: 2,
            latest_fence: Fence::new(1),
        },
        AdmissionLimits::default(),
    )?;
    Ok(owner)
}

pub(super) fn cohort(
    scope: &RequestScope,
    mut rows: BTreeMap<Vec<u8>, Vec<u8>>,
    records: &[StoredWorkflowRecord],
    fifo_q: bool,
) -> TestResult<BTreeMap<Vec<u8>, Vec<u8>>> {
    rows.retain(|key, _| matches!(key.get(6), Some(0 | 6 | 7)));
    let control = ControlState::restored(
        fixtures::clock(2)?,
        EntryIdentity::new(fixtures::log(0), PreparedDigest::new([0x5a; 32])),
        EntryStamp::Stamped(fixtures::clock(2)?),
    )?;
    rows.insert(
        key::encode_control_key(scope.group(), scope.recovery(), usize::MAX)?,
        value::encode_control(scope.group(), scope.recovery(), control, usize::MAX)?,
    );
    for owner in records {
        rows.insert(
            record_key(scope, owner)?,
            value::encode_record_fields(
                scope,
                &owner.partition,
                &owner.record,
                owner.ordering,
                AdmissionLimits::default(),
                usize::MAX,
            )?,
        );
        let row = entry(owner);
        let Some(lane) = LaneKey::for_record(&row) else {
            continue;
        };
        if fifo_q && lane.state == b"q" {
            rows.insert(
                key::encode_fifo_key(scope, lane, LaneOrder::for_record(&row), usize::MAX)?,
                vec![],
            );
            let head = !records.iter().any(|other| {
                let other = entry(other);
                LaneKey::for_record(&other) == Some(lane)
                    && LaneOrder::for_record(&other) < LaneOrder::for_record(&row)
            });
            if head && let Some(due) = DueOrder::for_record(&row) {
                rows.insert(key::encode_due_key(scope, due, usize::MAX)?, vec![1]);
            }
        } else if let Some(due) = DueOrder::for_record(&row) {
            rows.insert(key::encode_due_key(scope, due, usize::MAX)?, vec![0]);
        }
    }
    Ok(rows)
}

pub(super) fn seed(path: &Path, records: &[StoredWorkflowRecord]) -> TestResult {
    if !path.exists() {
        drop(fixtures::create(path)?);
    }
    let rows = cohort(
        &fixtures::scope()?,
        fixtures::raw_rows(path)?,
        records,
        true,
    )?;
    fixtures::replace(path, &rows, &fixtures::raw_manifest(path)?)
}

fn record_key(scope: &RequestScope, owner: &StoredWorkflowRecord) -> TestResult<Vec<u8>> {
    Ok(key::encode_record_key(
        scope,
        &owner.partition,
        owner.record.id(),
        usize::MAX,
    )?)
}

fn due_key(scope: &RequestScope, owner: &StoredWorkflowRecord) -> TestResult<Vec<u8>> {
    Ok(key::encode_due_key(
        scope,
        DueOrder::for_record(&entry(owner)).ok_or("no due")?,
        usize::MAX,
    )?)
}

fn member_key(scope: &RequestScope, owner: &StoredWorkflowRecord) -> TestResult<Vec<u8>> {
    let row = entry(owner);
    Ok(key::encode_fifo_key(
        scope,
        LaneKey::for_record(&row).ok_or("no lane")?,
        LaneOrder::for_record(&row),
        usize::MAX,
    )?)
}

fn rejected(
    path: &Path,
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
    manifest: &[u8],
    label: &str,
) -> TestResult {
    fixtures::replace(path, rows, manifest)?;
    assert!(
        matches!(fixtures::open(path), Err(error)
        if error.downcast_ref::<StateTransactionError>() == Some(&StateTransactionError::Corrupt)),
        "{label}"
    );
    assert_eq!(
        fixtures::raw_rows(path)?,
        *rows,
        "recovery must not repair {label}"
    );
    assert_eq!(fixtures::raw_manifest(path)?, manifest);
    Ok(())
}

fn orphan_history(scope: &RequestScope, rows: &mut BTreeMap<Vec<u8>, Vec<u8>>) -> TestResult {
    let id = RecordId::new(b"gone".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let history = HistoryEntry {
        event_id: "5-1".to_owned(),
        kind: EventKind::Created,
        occurred_at: TimestampMillis::new(99),
        version: 1,
        fence: Fence::new(0),
        attempts: 0,
        state: b"q".to_vec(),
        next_run_at: Some(TimestampMillis::new(10)),
        lease_deadline: TimestampMillis::new(0),
        worker: None,
        retry: None,
    };
    rows.insert(
        key::encode_history_key(scope, &partition, &id, &history, usize::MAX)?,
        value::encode_history(scope, &partition, &id, &history, usize::MAX)?,
    );
    Ok(())
}

fn corrupt_rows(
    path: &Path,
    rows: &BTreeMap<Vec<u8>, Vec<u8>>,
    manifest: &[u8],
    records: &[StoredWorkflowRecord],
) -> TestResult {
    let scope = fixtures::scope()?;
    let a = record_key(&scope, &records[0])?;
    let ma = member_key(&scope, &records[0])?;
    let mb = member_key(&scope, &records[1])?;
    let da = due_key(&scope, &records[0])?;
    let dc = due_key(&scope, &records[2])?;
    let mut trailing = a.clone();
    trailing.push(0);
    let mut unknown = a.clone();
    unknown[6] = 8;
    let absent = altered(b"lost", b"z", 3, Some(15))?;
    let cases = [
        ("missing canonical", a.clone(), None),
        (
            "orphan FIFO follower",
            record_key(&scope, &records[1])?,
            None,
        ),
        ("missing FIFO follower", mb, None),
        ("missing FIFO head due", da.clone(), None),
        ("missing parallel due", dc.clone(), None),
        ("FIFO nonempty value", ma, Some(vec![1])),
        ("FIFO due wrong tag", da, Some(vec![0])),
        ("parallel due wrong tag", dc, Some(vec![1])),
        ("orphan due", due_key(&scope, &absent)?, Some(vec![0])),
        ("trailing point key", trailing, rows.get(&a).cloned()),
        ("unknown family", unknown, rows.get(&a).cloned()),
        (
            "wrong record value identity",
            a,
            rows.get(&record_key(&scope, &records[1])?).cloned(),
        ),
        (
            "missing immutable foreign placement",
            key::encode_placement_key(
                scope.recovery(),
                &PartitionId::new(b"x".to_vec())?,
                usize::MAX,
            )?,
            None,
        ),
    ];
    for (label, key, value) in cases {
        let mut changed = rows.clone();
        if let Some(value) = value {
            changed.insert(key, value);
        } else {
            changed.remove(&key);
        }
        rejected(path, &changed, manifest, label)?;
    }
    let mut wrong = rows.clone();
    wrong.remove(&due_key(&scope, &records[0])?);
    wrong.insert(due_key(&scope, &records[1])?, vec![1]);
    rejected(path, &wrong, manifest, "follower masquerades as head")?;
    let mut wrong = rows.clone();
    wrong.remove(&member_key(&scope, &records[1])?);
    let mut shifted = records[1].clone();
    shifted.ordering.state_enter_seq += 1;
    wrong.insert(member_key(&scope, &shifted)?, vec![]);
    rejected(path, &wrong, manifest, "member sequence mismatches record")?;
    let mut wrong = rows.clone();
    let control = key::encode_control_key(scope.group(), scope.recovery(), usize::MAX)?;
    wrong.insert(
        control,
        value::encode_control(
            scope.group(),
            scope.recovery(),
            ControlState::initial(fixtures::clock(0)?),
            usize::MAX,
        )?,
    );
    rejected(
        path,
        &wrong,
        manifest,
        "initial control beside applied rows",
    )
}

fn history_binding(path: &Path, rows: &BTreeMap<Vec<u8>, Vec<u8>>, manifest: &[u8]) -> TestResult {
    let old = rows
        .keys()
        .find(|key| key.get(6) == Some(&5))
        .ok_or("no history")?;
    let mut changed = rows.clone();
    let wire = changed.remove(old).ok_or("no history value")?;
    let mut key = old.clone();
    let prefix = key::encode_history_prefix(
        &fixtures::scope()?,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(b"gone".to_vec())?,
        usize::MAX,
    )?;
    key[prefix.len()..prefix.len() + 8].copy_from_slice(&6_u64.to_be_bytes());
    let event = key.len() - 5;
    key[event] = b'6';
    changed.insert(key, wire);
    rejected(
        path,
        &changed,
        manifest,
        "history key event differs from value",
    )
}

fn blockers(dir: &Path) -> TestResult {
    for (name, head) in [
        ("unscheduled", fixtures::record(b"a", 1, None)?),
        ("running", running()?),
    ] {
        let path = dir.join(format!("{name}.redb"));
        let records = [head, fixtures::record(b"b", 2, Some(20))?];
        seed(&path, &records)?;
        drop(fixtures::open(&path)?);
        let mut rows = fixtures::raw_rows(&path)?;
        if let Some(due) = DueOrder::for_record(&entry(&records[0])) {
            rows.remove(&key::encode_due_key(&fixtures::scope()?, due, usize::MAX)?);
        }
        rows.insert(due_key(&fixtures::scope()?, &records[1])?, vec![1]);
        rejected(&path, &rows, &fixtures::raw_manifest(&path)?, name)?;
    }
    Ok(())
}

fn large_record() -> TestResult<StoredWorkflowRecord> {
    let mut owner = altered(b"large", b"q", 1, Some(10))?;
    owner.record = restore_record(
        WorkflowRecordData {
            id: owner.record.id().clone(),
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            version: 1,
            fence: Fence::new(0),
            attempts: 0,
            created_at: TimestampMillis::new(1),
            updated_at: TimestampMillis::new(1),
            next_run_at: Some(TimestampMillis::new(10)),
            run_state: None,
            lease: None,
            payload: Some(vec![7; fixtures::MIB]),
            result: None,
            error: None,
            latest_kind: EventKind::Created,
            latest_version: 1,
            latest_fence: Fence::new(0),
        },
        AdmissionLimits::default(),
    )?;
    Ok(owner)
}

fn large_profile(dir: &Path) -> TestResult {
    let path = dir.join("large.redb");
    let scope = fixtures::scope()?;
    let mut snapshot = b"FWPS\0\x02".to_vec();
    snapshot.extend_from_slice(&1_u32.to_be_bytes());
    snapshot.push(b'r');
    snapshot.extend_from_slice(&[1, 0]);
    snapshot.extend_from_slice(&262_117_u32.to_be_bytes());
    snapshot.resize(snapshot.len() + 262_117, 0);
    snapshot.extend_from_slice(&[0, 0, 0]);
    snapshot.extend_from_slice(&0_u32.to_be_bytes());
    snapshot.extend_from_slice(&[0, 0, 0]);
    assert_eq!(snapshot.len(), 262_144);
    let mut wire = b"FWPV\0\x02".to_vec();
    wire.extend_from_slice(&7_u64.to_be_bytes());
    wire.extend_from_slice(&262_144_u32.to_be_bytes());
    wire.extend_from_slice(&snapshot);
    assert_eq!(wire.len(), 262_162);
    assert_eq!(
        Sha256::digest(&wire).as_slice(),
        b"\xcb\x20\x8b\x6f\x19\x98\x6d\x37\x75\xba\x67\xe2\xeb\xa9\x37\xb0\x0e\x8b\xb3\x56\x24\x25\x67\xaf\xcd\xb2\x9c\x2c\xc0\xc0\x46\x2a"
    );
    let schema = PolicySchemaLimits::default();
    let policy =
        decode_complete_policy_value_v2(&wire, schema, &mut PolicySchemaCharge::new(schema))?;
    let policies = [(scope.tenant().clone(), policy)];
    let placements = fixtures::placements()?;
    let limits = StateStoreLimits::new(
        AdmissionLimits::default(),
        schema,
        (1_000, 128 * fixtures::MIB),
        (1_000_000, 1_024 * fixtures::MIB, 10_000_000),
        512 * fixtures::MIB,
        1_024 * fixtures::MIB,
    )?;
    drop(RedbStateStore::create(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: fixtures::clock(0)?,
            placements: &placements,
            policies: &policies,
        },
        limits,
    )?);
    let owner = large_record()?;
    let rows = cohort(&scope, fixtures::raw_rows(&path)?, &[owner], false)?;
    let manifest = fixtures::raw_manifest(&path)?;
    fixtures::replace(&path, &rows, &manifest)?;
    let mut store = RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        limits,
    )?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        2,
        64 * fixtures::MIB,
        fixtures::MIB,
    )?;
    let retained_record = tx
        .record(
            scope.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"large".to_vec())?,
        )?
        .ok_or("large missing")?;
    let retained_policy = tx
        .installed_policy(
            scope.tenant(),
            b"r",
            schema,
            &mut PolicySchemaCharge::new(schema),
        )?
        .ok_or("large policy missing")?;
    drop(tx);
    drop(store);
    drop(policies);
    drop(wire);
    drop(snapshot);
    assert_eq!(
        retained_record.record.payload(),
        Some(vec![7; fixtures::MIB].as_slice())
    );
    assert_eq!(retained_policy.generation(), 7);
    assert_eq!(retained_policy.encoded().len(), 262_162);
    assert_eq!(
        Sha256::digest(retained_policy.encoded()).as_slice(),
        b"\xcb\x20\x8b\x6f\x19\x98\x6d\x37\x75\xba\x67\xe2\xeb\xa9\x37\xb0\x0e\x8b\xb3\x56\x24\x25\x67\xaf\xcd\xb2\x9c\x2c\xc0\xc0\x46\x2a"
    );
    assert_eq!(fixtures::raw_rows(&path)?, rows);
    Ok(())
}

fn binary_maxima(dir: &Path) -> TestResult {
    let path = dir.join("binary.redb");
    let scope = RequestScope::new(
        TenantId::new(vec![0; 8_192])?,
        GroupId::new(vec![0; 256])?,
        RecoveryId::new(vec![255; 256])?,
    );
    let partition = PartitionId::new(vec![0; 65_535])?;
    let placements = [(partition.clone(), scope.group().clone())];
    let policies = [(scope.tenant().clone(), fixtures::rich_policy()?)];
    drop(RedbStateStore::create(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: fixtures::clock(0)?,
            placements: &placements,
            policies: &policies,
        },
        StateStoreLimits::default(),
    )?);
    let mut owner = fixtures::record(&vec![0; 65_535], u128::MAX - 7, Some(10))?;
    owner.tenant = scope.tenant().clone();
    owner.partition = partition.clone();
    let encoded_key = record_key(&scope, &owner)?;
    assert!(encoded_key.len() > 16_384);
    let rows = cohort(&scope, fixtures::raw_rows(&path)?, &[owner], true)?;
    fixtures::replace(&path, &rows, &fixtures::raw_manifest(&path)?)?;
    let mut store = RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreLimits::default(),
    )?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        100,
        fixtures::MIB,
        4 * fixtures::MIB,
    )?;
    let retained_record = tx
        .record(scope.tenant(), &partition, &RecordId::new(vec![0; 65_535])?)?
        .ok_or("binary missing")?;
    drop(tx);
    drop(store);
    drop(policies);
    drop(placements);
    assert_eq!(retained_record.tenant.as_bytes(), vec![0; 8_192]);
    assert_eq!(retained_record.partition.as_bytes(), vec![0; 65_535]);
    assert_eq!(retained_record.record.id().as_bytes(), vec![0; 65_535]);
    assert_eq!(retained_record.ordering.state_enter_seq, u128::MAX - 7);
    assert_eq!(fixtures::raw_rows(&path)?, rows);
    Ok(())
}

#[test]
fn physical_recovery_binds_keys_and_complete_indexes() -> TestResult {
    let dir = fixtures::directory("recovery")?;
    let path = dir.join("state.redb");
    let scope = fixtures::scope()?;
    let placements = [
        (PartitionId::new(b"p".to_vec())?, scope.group().clone()),
        (
            PartitionId::new(b"x".to_vec())?,
            GroupId::new(b"other".to_vec())?,
        ),
    ];
    let policies = [(scope.tenant().clone(), fixtures::rich_policy()?)];
    drop(RedbStateStore::create(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: fixtures::clock(0)?,
            placements: &placements,
            policies: &policies,
        },
        StateStoreLimits::default(),
    )?);
    let records = [
        fixtures::record(b"a", 1, Some(10))?,
        fixtures::record(b"b", 2, Some(20))?,
        altered(b"c", b"z", 3, Some(15))?,
        altered(b"terminal", b"completed", 4, Some(5))?,
    ];
    let mut rows = cohort(&scope, fixtures::raw_rows(&path)?, &records, true)?;
    orphan_history(&scope, &mut rows)?;
    let manifest = fixtures::raw_manifest(&path)?;
    fixtures::replace(&path, &rows, &manifest)?;
    drop(fixtures::open(&path)?);
    assert_eq!(fixtures::raw_rows(&path)?, rows);
    corrupt_rows(&path, &rows, &manifest, &records)?;
    let mut foreign = fixtures::record(b"foreign", 1, Some(10))?;
    foreign.partition = PartitionId::new(b"x".to_vec())?;
    let foreign_rows = cohort(&scope, rows.clone(), &[foreign], true)?;
    rejected(&path, &foreign_rows, &manifest, "foreign placement")?;
    history_binding(&path, &rows, &manifest)?;
    fixtures::replace(&path, &rows, &manifest)?;
    drop(fixtures::open(&path)?);
    blockers(&dir)?;
    binary_maxima(&dir)?;
    large_profile(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
