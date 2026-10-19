//! Prospective cumulative read/scratch caps and authentic same-writer traversal.

use std::path::Path;

use wf_domain::claim::apply::{ClaimQuery, ClaimStates};
use wf_domain::claim::{DueOrder, LaneKey, PartitionFilter};
use wf_domain::policy::complete::{PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_ports::state::{
    DueCursorStep, GroupApplyStore, GroupApplyTransaction, GroupDueCursor, StateTransactionError,
};
use wf_storage::state::{RedbStateStore, StateStoreLimits};
use wf_types::identity::{PartitionId, RecordId};

use super::{TestResult, fixtures, recovery};

mod profiles;

fn query() -> ClaimQuery<'static> {
    ClaimQuery {
        workflow_type: b"r",
        partitions: PartitionFilter::Any,
        states: ClaimStates::Any,
        exclude_states: &[],
        priority: None,
        worker: b"worker",
        lease_ms: 100,
        limit: 10,
    }
}

fn control_budgets(store: &mut RedbStateStore) -> TestResult {
    let scope = fixtures::scope()?;
    // Each control call has one step, R512, and its actual13-byte point key in C.
    for (steps, read, scratch, two) in [
        (2, 1_024, 26, true),
        (1, 1_024, 26, false),
        (2, 1_023, 26, false),
        (2, 1_024, 25, false),
    ] {
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            AdmissionLimits::default(),
            steps,
            read,
            scratch,
        )?;
        let first = tx.control()?;
        let second = tx.control();
        if two {
            assert_eq!(second?, first);
        } else {
            assert!(matches!(second, Err(StateTransactionError::Limit)));
        }
    }
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        1,
        512,
        13,
    )?;
    assert_eq!(
        tx.control()?
            .last_entry()
            .ok_or("no applied control")?
            .log(),
        fixtures::log(0)
    );
    Ok(())
}

fn point_budgets(store: &mut RedbStateStore) -> TestResult {
    let scope = fixtures::scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RecordId::new(b"a".to_vec())?;
    // R=512+t+p+id+type+state+payload(2)=519. C=scope515+key22=537.
    for (read, scratch, success) in [(519, 537, true), (518, 537, false), (519, 536, false)] {
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            AdmissionLimits::default(),
            1,
            read,
            scratch,
        )?;
        let result = tx.record(scope.tenant(), &partition, &id);
        if success {
            assert_eq!(
                result?.ok_or("record absent")?.record.payload(),
                Some([0, 255].as_slice())
            );
        } else {
            assert!(matches!(result, Err(StateTransactionError::Limit)));
        }
    }
    for (read, scratch, two) in [
        (1_038, 1_074, true),
        (1_037, 1_074, false),
        (1_038, 1_073, false),
    ] {
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            AdmissionLimits::default(),
            2,
            read,
            scratch,
        )?;
        drop(
            tx.record(scope.tenant(), &partition, &id)?
                .ok_or("record absent")?,
        );
        let second = tx.record(scope.tenant(), &partition, &id);
        if two {
            assert!(second?.is_some());
        } else {
            assert!(matches!(second, Err(StateTransactionError::Limit)));
        }
    }
    let tiny = AdmissionLimits::new(1, 16 * fixtures::MIB, 1_000)?;
    let tx = store.begin_apply(scope.group(), scope.recovery(), tiny, 1, 519, 537)?;
    assert!(matches!(
        tx.record(scope.tenant(), &partition, &id),
        Err(StateTransactionError::Limit)
    ));
    Ok(())
}

fn policy_budgets(store: &mut RedbStateStore) -> TestResult {
    let scope = fixtures::scope()?;
    assert_eq!(fixtures::rich_policy()?.encoded().len(), 134);
    // P=2048+216*134=30992; scope515+four-component policy key19=C534.
    for (read, scratch, success) in [
        (30_992, 534, true),
        (30_991, 534, false),
        (30_992, 533, false),
    ] {
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            AdmissionLimits::default(),
            1,
            read,
            scratch,
        )?;
        let schema = PolicySchemaLimits::default();
        let mut charge = PolicySchemaCharge::new(schema);
        let result = tx.installed_policy(scope.tenant(), b"r", schema, &mut charge);
        if success {
            assert!(result?.ok_or("policy absent")? == fixtures::rich_policy()?);
        } else {
            assert!(matches!(result, Err(StateTransactionError::Limit)));
        }
    }
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        1,
        30_992,
        534,
    )?;
    let schema = PolicySchemaLimits::new(PolicySchemaLimits::default().policy_limits(), 1)?;
    assert!(matches!(
        tx.installed_policy(
            scope.tenant(),
            b"r",
            schema,
            &mut PolicySchemaCharge::new(schema)
        ),
        Err(StateTransactionError::Limit)
    ));
    Ok(())
}

fn ceilings(store: &mut RedbStateStore) -> TestResult {
    let scope = fixtures::scope()?;
    for admission in [
        AdmissionLimits::new(2 * fixtures::MIB, 16 * fixtures::MIB, 1_000)?,
        AdmissionLimits::new(fixtures::MIB, 16 * fixtures::MIB + 1, 1_000)?,
        AdmissionLimits::new(fixtures::MIB, 16 * fixtures::MIB, 1_001)?,
    ] {
        assert!(matches!(
            store.begin_apply(scope.group(), scope.recovery(), admission, 10, 1_024, 1_024),
            Err(StateTransactionError::InvalidChanges)
        ));
    }
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        1,
        512,
        13,
    )?;
    assert!(tx.control()?.last_entry().is_some());
    Ok(())
}

fn cursor_budgets(store: &mut RedbStateStore) -> TestResult {
    let scope = fixtures::scope()?;
    // One due key costs132 locally. Global R also owns canonical519 and true-head519.
    // C: open547 +candidate712 +canonical539 +head1277=3075; seven real steps.
    for (steps, read, scratch, local, row) in [
        (7, 1_170, 3_075, 132, true),
        (7, 1_169, 3_075, 132, false),
        (7, 1_170, 3_074, 132, false),
        (6, 1_170, 3_075, 132, false),
        (7, 1_170, 3_075, 131, false),
    ] {
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            AdmissionLimits::default(),
            steps,
            read,
            scratch,
        )?;
        let mut cursor = tx.due_cursor(scope.tenant(), query(), None, 1, local)?;
        if row {
            match cursor.next()? {
                DueCursorStep::Row(due) => assert_eq!(due.id, b"a"),
                _ => return Err("admitted first row missing".into()),
            }
            assert_eq!(cursor.position().ok_or("position absent")?.id, b"a");
            assert!(matches!(cursor.next()?, DueCursorStep::BudgetEnd));
            assert_eq!(cursor.position().ok_or("position lost")?.id, b"a");
        } else {
            assert!(matches!(cursor.next()?, DueCursorStep::BudgetEnd));
            assert!(cursor.position().is_none());
            assert!(matches!(cursor.next()?, DueCursorStep::BudgetEnd));
        }
    }
    Ok(())
}

fn filtered_continuation(path: &Path) -> TestResult {
    let records = [
        fixtures::record(b"a", 1, Some(10))?,
        fixtures::record(b"b", 2, Some(20))?,
        recovery::altered(b"c", b"z", 3, Some(15))?,
    ];
    recovery::seed(path, &records)?;
    let mut store = fixtures::open(path)?;
    let scope = fixtures::scope()?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        100,
        65_536,
        65_536,
    )?;
    let states = [b"z".as_slice()];
    let query = ClaimQuery {
        states: ClaimStates::Only(&states),
        ..query()
    };
    let mut cursor = tx.due_cursor(scope.tenant(), query, None, 1, 132)?;
    assert!(matches!(cursor.next()?, DueCursorStep::BudgetEnd));
    let position = cursor
        .position()
        .ok_or("filtered candidate did not advance")?;
    assert_eq!((position.id, position.due.get()), (b"a".as_slice(), 10));
    drop(cursor);
    let after = DueOrder {
        priority: 0,
        due: wf_types::version::TimestampMillis::new(10),
        id: b"a",
        partition_key: b"p",
        state: b"q",
        workflow_type: b"r",
    };
    let mut cursor = tx.due_cursor(scope.tenant(), query, Some(after), 10, 132)?;
    match cursor.next()? {
        DueCursorStep::Row(row) => assert_eq!(row.id, b"c"),
        _ => return Err("resumed row missing".into()),
    }
    assert!(matches!(cursor.next()?, DueCursorStep::RangeEnd));
    assert_eq!(
        cursor.position().ok_or("resumed position missing")?.id,
        b"c"
    );
    assert!(matches!(cursor.next()?, DueCursorStep::RangeEnd));
    Ok(())
}

fn authentic_blockers(dir: &Path) -> TestResult {
    for (name, head) in [
        ("unscheduled", fixtures::record(b"a", 1, None)?),
        ("running", recovery::running()?),
    ] {
        let path = dir.join(format!("cursor-{name}.redb"));
        let records = [head, fixtures::record(b"b", 2, Some(20))?];
        recovery::seed(&path, &records)?;
        let mut store = fixtures::open(&path)?;
        let scope = fixtures::scope()?;
        let tx = store.begin_apply(
            scope.group(),
            scope.recovery(),
            AdmissionLimits::default(),
            100,
            65_536,
            65_536,
        )?;
        let lane = LaneKey {
            workflow_type: b"r",
            state: b"q",
            partition_key: b"p",
        };
        assert_eq!(
            tx.fifo_head(scope.tenant(), lane)?.ok_or("head absent")?,
            records[0]
        );
        let states = [b"q".as_slice()];
        let mut cursor = tx.due_cursor(
            scope.tenant(),
            ClaimQuery {
                states: ClaimStates::Only(&states),
                ..query()
            },
            None,
            10,
            1_024,
        )?;
        assert!(matches!(cursor.next()?, DueCursorStep::RangeEnd));
        if name == "running" {
            assert_eq!(
                cursor.position().ok_or("running head not consumed")?.id,
                b"a"
            );
        } else {
            assert!(cursor.position().is_none());
        }
    }
    Ok(())
}

fn owned_after_donors(path: &Path) -> TestResult {
    recovery::seed(path, &[fixtures::record(b"a", 1, Some(10))?])?;
    let scope = fixtures::scope()?;
    let mut store = fixtures::open(path)?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        AdmissionLimits::default(),
        2,
        31_511,
        1_071,
    )?;
    let record = tx
        .record(
            scope.tenant(),
            &PartitionId::new(b"p".to_vec())?,
            &RecordId::new(b"a".to_vec())?,
        )?
        .ok_or("record absent")?;
    let schema = PolicySchemaLimits::default();
    let policy = tx
        .installed_policy(
            scope.tenant(),
            b"r",
            schema,
            &mut PolicySchemaCharge::new(schema),
        )?
        .ok_or("policy absent")?;
    drop(tx);
    drop(store);
    drop(scope);
    assert_eq!(record.record.id().as_bytes(), b"a");
    assert_eq!(record.record.payload(), Some([0, 255].as_slice()));
    assert!(policy == fixtures::rich_policy()?);
    assert_eq!(policy.encoded().len(), 134);
    Ok(())
}

fn healthy_reopen_profiles(path: &Path) -> TestResult {
    drop(fixtures::open(path)?);
    let rows = fixtures::raw_rows(path)?;
    let manifest = fixtures::raw_manifest(path)?;
    let scope = fixtures::scope()?;
    let admission = AdmissionLimits::default();
    let ordinary = (1_000_000, 1_024 * fixtures::MIB, 10_000_000);
    let tiny = AdmissionLimits::new(1, 16 * fixtures::MIB, 1_000)?;
    for (label, profile, recovery) in [
        ("healthy two-byte payload under inline1", tiny, ordinary),
        ("recovery row count", admission, (1, ordinary.1, ordinary.2)),
        (
            "recovery wire bytes",
            admission,
            (ordinary.0, 1, ordinary.2),
        ),
        ("recovery steps", admission, (ordinary.0, ordinary.1, 1)),
    ] {
        let limits = StateStoreLimits::new(
            profile,
            PolicySchemaLimits::default(),
            (1_000, 128 * fixtures::MIB),
            recovery,
            128 * fixtures::MIB,
            256 * fixtures::MIB,
        )?;
        assert!(
            matches!(
                RedbStateStore::open(
                    path,
                    scope.group().clone(),
                    scope.recovery().clone(),
                    limits
                ),
                Err(StateTransactionError::Limit)
            ),
            "{label}",
        );
        assert_eq!(fixtures::raw_rows(path)?, rows, "{label} changed rows");
        assert_eq!(
            fixtures::raw_manifest(path)?,
            manifest,
            "{label} changed manifest"
        );
    }
    drop(fixtures::open(path)?);
    assert_eq!(fixtures::raw_rows(path)?, rows);
    assert_eq!(fixtures::raw_manifest(path)?, manifest);
    Ok(())
}

#[test]
fn owned_reads_cursor_and_profiles() -> TestResult {
    let dir = fixtures::directory("cursor")?;
    let path = dir.join("state.redb");
    recovery::seed(
        &path,
        &[
            fixtures::record(b"a", 1, Some(10))?,
            fixtures::record(b"b", 2, Some(20))?,
        ],
    )?;
    let expected = fixtures::raw_rows(&path)?;
    let mut store = fixtures::open(&path)?;
    control_budgets(&mut store)?;
    point_budgets(&mut store)?;
    policy_budgets(&mut store)?;
    ceilings(&mut store)?;
    cursor_budgets(&mut store)?;
    drop(store);
    assert_eq!(fixtures::raw_rows(&path)?, expected);
    healthy_reopen_profiles(&path)?;
    profiles::genesis_policy_shape_profile(&dir)?;
    profiles::outcome_count_profile(&dir)?;
    filtered_continuation(&path)?;
    authentic_blockers(&dir)?;
    owned_after_donors(&path)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
