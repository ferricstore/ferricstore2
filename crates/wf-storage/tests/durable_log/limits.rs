//! Independent logical owner goldens; these fees are not physical allocation sizes.

use std::ops::Bound::{Excluded, Included, Unbounded};
use std::path::Path;

use redb::ReadableDatabase;
use wf_domain::policy::complete::{
    PolicySchemaCharge, PolicySchemaLimits, decode_complete_policy_value_v2,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_domain::workflow::{CommittedCommand, Create, Operation};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{
    CodecLimits, RaftLogEntry, RaftLogError, RaftLogLimits, RaftLogStore, encode_record_batch,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, HlcTimestamp, TimestampMillis};

use super::{
    MIB, TestResult, assert_entries, create, directory, entries, expected_cohort, id, open,
    raw_cohort,
};

fn profile(
    entry: usize,
    append: (usize, usize),
    read: (usize, usize),
    recovery: (usize, usize),
    workspace: usize,
) -> TestResult<RaftLogLimits> {
    Ok(RaftLogLimits::new(
        entry, append, read, recovery, workspace,
    )?)
}

fn invalid_profiles() {
    for args in [
        (0, (1, 515), (1, 515), (1, 73), 10_000),
        (3, (0, 515), (1, 515), (1, 73), 10_000),
        (3, (1, 514), (1, 515), (1, 73), 10_000),
        (3, (1, 515), (0, 515), (1, 73), 10_000),
        (3, (1, 515), (1, 514), (1, 73), 10_000),
        (3, (1, 515), (1, 515), (0, 73), 10_000),
        (3, (1, 515), (1, 515), (1, 72), 10_000),
        (3, (1, 515), (1, 515), (1, 73), 0),
        (
            usize::MAX,
            (1, usize::MAX),
            (1, usize::MAX),
            (1, usize::MAX),
            usize::MAX,
        ),
    ] {
        assert!(matches!(
            RaftLogLimits::new(args.0, args.1, args.2, args.3, args.4),
            Err(RaftLogError::InvalidLimits)
        ));
    }
}

fn append_caps(dir: &Path) -> TestResult {
    // g=z=1: G514 + fixed2048 + A515 + wire73 =3150.
    for (label, work, accepted) in [
        ("append-exact", 3_150, true),
        ("append-tight", 3_149, false),
    ] {
        let path = dir.join(label);
        let mut store = create(&path, profile(3, (2, 1_030), (2, 1_030), (10, 730), work)?)?;
        let result = store.append(entries(&[(id(1, 7, 0), b"abc")]));
        if accepted {
            result?;
        } else {
            assert!(matches!(result, Err(RaftLogError::Limit)));
        }
        drop(store);
        let specs = if accepted {
            vec![(id(1, 7, 0), b"abc".as_slice())]
        } else {
            Vec::new()
        };
        assert_eq!(
            raw_cohort(&path)?,
            expected_cohort(b"g", b"z", &specs, None, None)?
        );
    }
    for (label, count, bytes, entry_cap) in [
        ("count", 1, 1_030, 3),
        ("bytes", 2, 1_029, 3),
        ("entry", 2, 1_030, 2),
    ] {
        let path = dir.join(label);
        let mut store = create(
            &path,
            profile(entry_cap, (count, bytes), (2, 1_030), (10, 730), 10_000)?,
        )?;
        assert!(matches!(
            store.append(entries(&[(id(1, 7, 0), b"abc"), (id(1, 7, 1), b"def")])),
            Err(RaftLogError::Limit)
        ));
        drop(store);
        assert_eq!(
            raw_cohort(&path)?,
            expected_cohort(b"g", b"z", &[], None, None)?
        );
    }
    let path = dir.join("two-exact");
    // A=1030; maximum single wire73: peak3665, no second wire owner.
    let mut store = create(&path, profile(3, (2, 1_030), (2, 1_030), (10, 730), 3_665)?)?;
    store.append(entries(&[(id(1, 7, 0), b"abc"), (id(1, 7, 1), b"def")]))?;
    assert_entries(&mut store, &[(id(1, 7, 0), b"abc"), (id(1, 7, 1), b"def")])?;
    Ok(())
}

fn read_caps(dir: &Path) -> TestResult {
    let path = dir.join("pages");
    let specs = [(id(1, 7, 0), b"abc".as_slice()), (id(1, 7, 1), b"def")];
    let mut store = create(&path, RaftLogLimits::default())?;
    store.append(entries(&specs))?;
    drop(store);
    for (count, bytes) in [(1, 1_030), (2, 515)] {
        let mut store = open(
            &path,
            profile(3, (2, 1_030), (count, bytes), (10, 730), 10_000)?,
        )?;
        let first = store.read_page(Unbounded, Unbounded)?;
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].body(), b"abc");
        let second = store.read_page(Excluded(first[0].id().index().get()), Unbounded)?;
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].body(), b"def");
        assert_eq!(store.read_page(Excluded(1), Unbounded)?.len(), 0);
    }
    for (work, accepted) in [(3_077, true), (3_076, false)] {
        // G514+2048+R515=3077, distinct from append's wire overlap.
        let mut store = open(&path, profile(3, (2, 1_030), (1, 515), (10, 730), work)?)?;
        let result = store.read_page(Included(0), Included(0));
        if accepted {
            let owned = result?;
            drop(store);
            assert_eq!(owned.len(), 1);
            assert_eq!(owned[0].body(), b"abc");
        } else {
            assert!(matches!(result, Err(RaftLogError::Limit)));
        }
    }
    let mut store = open(&path, RaftLogLimits::default())?;
    for (start, end) in [
        (Included(0), Excluded(0)),
        (Excluded(0), Included(0)),
        (Excluded(u64::MAX), Unbounded),
        (Included(u64::MAX), Included(u64::MAX)),
        (Unbounded, Excluded(0)),
    ] {
        assert_eq!(store.read_page(start, end)?.len(), 0);
    }
    for (start, end) in [
        (Included(2), Included(0)),
        (Excluded(1), Included(0)),
        (Excluded(u64::MAX), Excluded(u64::MAX)),
    ] {
        assert!(matches!(
            store.read_page(start, end),
            Err(RaftLogError::InvalidRange)
        ));
    }
    assert_entries(&mut store, &specs)?;
    Ok(())
}

fn prepared(payload: Vec<u8>) -> TestResult<wf_ports::log::PreparedBytes> {
    let request = PreparedRequest {
        scope: RequestScope::new(
            TenantId::new(b"t".to_vec())?,
            GroupId::new(b"g".to_vec())?,
            RecoveryId::new(b"z".to_vec())?,
        ),
        request: RequestId::new(b"q".to_vec())?,
        intent: ClientIntent::new(1, [0xaa; 32])?,
        stamp: CommittedTime::new(HlcTimestamp::new(1, 0)?, 1)?,
        operation: PreparedOperation::Batch(vec![PreparedBatchItem {
            partition: PartitionId::new(b"p".to_vec())?,
            command: CommittedCommand {
                id: RecordId::new(b"i".to_vec())?,
                now: TimestampMillis::new(1),
                operation: Operation::Create(Create {
                    workflow_type: b"r".to_vec(),
                    state: b"q".to_vec(),
                    run_at: None,
                    payload: Some(payload),
                }),
            },
            create_priority: 0,
        }]),
    };
    Ok(encode_record_batch(
        request,
        CodecLimits::new(AdmissionLimits::default()),
        256 * MIB,
    )?)
}

fn owned_transfer_and_large_rows(dir: &Path) -> TestResult {
    let frame = prepared(vec![0xa5; MIB])?;
    let pointer = frame.as_bytes().as_ptr();
    let length = frame.as_bytes().len();
    assert_eq!(length, MIB + 119);
    let bytes = frame.into_bytes();
    assert_eq!(bytes.as_ptr(), pointer);
    assert_eq!(bytes.len(), length);
    let capacity = bytes.capacity();
    let entry = RaftLogEntry::new(id(1, 7, 1), bytes);
    let (identity, bytes) = entry.into_parts();
    assert_eq!(identity, id(1, 7, 1));
    assert_eq!(bytes.as_ptr(), pointer);
    assert_eq!(bytes.len(), length);
    assert_eq!(bytes.capacity(), capacity);
    let mut policy = b"FWPV\0\x02\0\0\0\0\0\0\0\x07\0\x04\0\0FWPS\0\x02\0\0\0\x01r\x01\0".to_vec();
    policy.extend_from_slice(&262_117_u32.to_be_bytes());
    policy.resize(policy.len() + 262_117 + 10, 0);
    assert_eq!(policy.len(), 262_162);
    let schema = PolicySchemaLimits::default();
    let value =
        decode_complete_policy_value_v2(&policy, schema, &mut PolicySchemaCharge::new(schema))?;
    assert_eq!(value.encoded(), policy);
    let full = vec![0x5a; 16 * MIB];
    let path = dir.join("large");
    let specs = [
        (id(1, 7, 0), full.as_slice()),
        (id(1, 7, 1), bytes.as_slice()),
        (id(1, 7, 2), policy.as_slice()),
    ];
    let mut store = create(&path, RaftLogLimits::default())?;
    store.append(entries(&specs))?;
    let page = store.read_page(Unbounded, Unbounded)?;
    drop(store);
    assert_eq!(page.len(), 3);
    for (row, (identity, body)) in page.iter().zip(&specs) {
        assert_eq!(row.id(), *identity);
        assert_eq!(row.body(), *body);
    }
    let mut reopened = open(&path, RaftLogLimits::default())?;
    assert_entries(&mut reopened, &specs)?;
    drop(reopened);
    assert_eq!(
        raw_cohort(&path)?,
        expected_cohort(b"g", b"z", &specs, None, None)?
    );
    Ok(())
}

fn backend_entry_caps(dir: &Path) -> TestResult {
    // Exact redb 4.4.0 value ceiling, minus FWRL's 70-byte envelope.
    // These are scalar configuration checks; no large body is allocated.
    let maximum = 3 * 1_024 * MIB - 70;
    let exact = profile(
        maximum,
        (1, maximum + 512),
        (1, maximum + 512),
        (1, maximum + 70),
        10_000,
    )?;
    let oversized = profile(
        maximum + 1,
        (1, maximum + 513),
        (1, maximum + 513),
        (1, maximum + 71),
        10_000,
    )?;
    let path = dir.join("backend-ceiling");
    drop(create(&path, exact)?);
    drop(open(&path, exact)?);
    assert!(matches!(open(&path, oversized), Err(error)
        if error.downcast_ref::<RaftLogError>() == Some(&RaftLogError::InvalidLimits)));
    assert_eq!(
        raw_cohort(&path)?,
        expected_cohort(b"g", b"z", &[], None, None)?
    );

    let absent = dir.join("oversized-never-created");
    for initialize in [true, false] {
        let result = if initialize {
            create(&absent, oversized)
        } else {
            open(&absent, oversized)
        };
        assert!(matches!(result, Err(error)
            if error.downcast_ref::<RaftLogError>() == Some(&RaftLogError::InvalidLimits)));
        assert!(!absent.exists());
    }
    let supplied = dir.join("oversized-supplied");
    for initialize in [true, false] {
        let database = redb::Builder::new()
            .set_cache_size(16 * MIB)
            .create(&supplied)?;
        let group = GroupId::new(b"g".to_vec())?;
        let recovery = RecoveryId::new(b"z".to_vec())?;
        let result = if initialize {
            wf_storage::log::RedbLogStore::create_database(database, group, recovery, oversized)
        } else {
            wf_storage::log::RedbLogStore::open_database(database, group, recovery, oversized)
        };
        assert!(matches!(result, Err(RaftLogError::InvalidLimits)));
        let database = redb::Builder::new()
            .set_cache_size(16 * MIB)
            .open(&supplied)?;
        let transaction = database.begin_read()?;
        assert_eq!(transaction.list_tables()?.count(), 0);
        assert_eq!(transaction.list_multimap_tables()?.count(), 0);
    }
    Ok(())
}

#[test]
fn log_profiles_and_owned_pages() -> TestResult {
    let defaults = RaftLogLimits::default();
    assert_eq!(defaults.maximum_entry_bytes(), 16 * MIB);
    assert_eq!(defaults.maximum_batch_entries(), 1_000);
    assert_eq!(defaults.maximum_append_bytes(), 64 * MIB);
    assert_eq!(defaults.maximum_read_entries(), 1_000);
    assert_eq!(defaults.maximum_read_bytes(), 64 * MIB);
    assert_eq!(defaults.maximum_recovery_entries(), 1_000_000);
    assert_eq!(defaults.maximum_recovery_bytes(), 1_024 * MIB);
    assert_eq!(defaults.maximum_workspace_bytes(), 256 * MIB);
    invalid_profiles();
    let dir = directory("limits")?;
    append_caps(&dir)?;
    read_caps(&dir)?;
    owned_transfer_and_large_rows(&dir)?;
    backend_entry_caps(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
