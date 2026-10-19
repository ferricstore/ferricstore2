//! Prospective wire/owner/I/O limits and actual target-profile revalidation.

use redb::ReadableDatabase;
use std::io::Cursor;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_ports::consensus::AppliedRaftMembership;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction, StateTransactionError};
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};

use super::super::control;
use super::{
    RedbSnapshotStage, SnapshotError, SnapshotLimits, SnapshotMetadata, StateSnapshotStore,
    TestResult, f, fixtures as s,
};

pub(super) fn profile(
    data: (usize, usize, usize),
    io: (usize, usize),
    owners: (usize, usize),
    workspace: usize,
) -> TestResult<SnapshotLimits> {
    Ok(SnapshotLimits::new(
        data,
        (8_000_000, 4_000_000_000, 100_000_000),
        io,
        owners,
        workspace,
    )?)
}

fn limited(error: SnapshotError) {
    assert!(
        matches!(
            error,
            SnapshotError::Limit
                | SnapshotError::Stage(StateTransactionError::Limit)
                | SnapshotError::State(StateTransactionError::Limit)
        ),
        "{error:?}"
    );
}

#[test]
fn limits_and_metadata_are_checked_mathematical_values() -> TestResult {
    for index in 0..11 {
        let mut n = [1; 11];
        n[index] = 0;
        assert!(matches!(
            SnapshotLimits::new(
                (n[0], n[1], n[2]),
                (n[3], n[4], n[5]),
                (n[6], n[7]),
                (n[8], n[9]),
                n[10]
            ),
            Err(SnapshotError::InvalidLimits)
        ));
    }
    let tiny = SnapshotLimits::new((1, 1, 1), (1, 1, 1), (1, 1), (1, 1), 1)?;
    assert_eq!(tiny.maximum_rows(), 1);
    assert_eq!(tiny.maximum_visited_rows(), 1);
    let cap = SnapshotLimits::default();
    assert_eq!(
        (
            cap.maximum_rows(),
            cap.maximum_encoded_bytes(),
            cap.maximum_row_bytes()
        ),
        (1_000_000, 1_073_741_824, 67_108_864)
    );
    assert_eq!(
        (
            cap.maximum_visited_rows(),
            cap.maximum_visited_bytes(),
            cap.maximum_steps()
        ),
        (8_000_000, 4_000_000_000, 100_000_000)
    );
    assert_eq!(
        (cap.maximum_io_calls(), cap.maximum_io_bytes()),
        (10_000_000, 65_536)
    );
    assert_eq!(
        (
            cap.maximum_metadata_bytes(),
            cap.maximum_temporary_bytes(),
            cap.workspace_bytes()
        ),
        (134_217_728, 134_217_728, 536_870_912)
    );
    let scope = f::scope()?;
    let initial = ControlState::initial(f::clock(0)?);
    let meta = SnapshotMetadata::new(
        (scope.group().clone(), scope.recovery().clone()),
        initial,
        AppliedRaftMembership::initial(),
        [3; 32],
        (3, 325),
    )?;
    assert_eq!(meta.control(), initial);
    assert_eq!(meta.row_count(), 3);
    assert_eq!(meta.encoded_bytes(), 325);
    assert_eq!(meta.id(), &[3; 32]);
    assert!(meta.into_membership().entry().is_none());
    let id = EntryIdentity::new(f::log(0), PreparedDigest::new([3; 32]));
    let impossible =
        AppliedRaftMembership::new(id, EntryStamp::Unstamped, control::fixtures::membership()?);
    assert!(
        SnapshotMetadata::new(
            (scope.group().clone(), scope.recovery().clone()),
            initial,
            impossible,
            [3; 32],
            (3, 325)
        )
        .is_err()
    );
    let control = ControlState::restored(f::clock(0)?, id, EntryStamp::Unstamped)?;
    let (_, _, empty) = AppliedRaftMembership::initial().into_parts();
    assert!(
        SnapshotMetadata::new(
            (scope.group().clone(), scope.recovery().clone()),
            control,
            AppliedRaftMembership::new(id, EntryStamp::Unstamped, empty),
            [3; 32],
            (3, 325)
        )
        .is_err()
    );
    assert!(
        SnapshotMetadata::new(
            (scope.group().clone(), scope.recovery().clone()),
            initial,
            AppliedRaftMembership::initial(),
            [3; 32],
            (0, 325)
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn exact_wire_metadata_workspace_and_actual_eof_attempts() -> TestResult {
    let dir = f::directory("snapshot-exact")?;
    let path = dir.join("source.redb");
    let mut state = s::empty(&path)?;
    let literal = s::hex(s::INITIAL)?;
    let c = f::MIB;
    let r = 2050;
    let w = 514 + 2048 + r + c;
    let exact = profile((3, 325, 118), (325, 1), (r, c), w)?;
    let mut output = Vec::new();
    let meta = state.capture_snapshot(&mut output, exact)?;
    assert_eq!(output, literal);
    assert_eq!(meta.row_count(), 3);
    let mut current = Vec::new();
    state
        .current_snapshot(&mut current, exact)?
        .ok_or("current absent")?;
    assert_eq!(current, literal);
    drop(state);
    let before = s::raw(&path)?;
    let scope = f::scope()?;
    for (index, (data, io, owners, workspace)) in [
        ((2, 325, 118), (325, 1), (r, c), w),
        ((3, 324, 118), (325, 1), (r, c), w),
        ((3, 325, 117), (325, 1), (r, c), w),
        ((3, 325, 118), (324, 1), (r, c), w),
        ((3, 325, 118), (325, 1), (r - 1, c), w),
        ((3, 325, 118), (325, 1), (r, c), w - 1),
        ((3, 325, 118), (325, 1), (r, 1), w),
    ]
    .into_iter()
    .enumerate()
    {
        let mut active = f::open(&path)?;
        let cap = profile(data, io, owners, workspace)?;
        let mut bytes = Vec::new();
        limited(
            active
                .capture_snapshot(&mut bytes, cap)
                .err()
                .ok_or("tight capture succeeded")?,
        );
        assert_eq!(control::fixtures::current(&mut active)?, meta.control());
        drop(active);
        s::assert_raw(&path, &before)?;
        let mut input = Cursor::new(&literal);
        let receive_cap = profile(data, (io.0 + 1, io.1), owners, workspace)?;
        let result = RedbSnapshotStage::receive(
            &dir.join(format!("tight-{index}.redb")),
            scope.group().clone(),
            scope.recovery().clone(),
            &mut input,
            StateStoreLimits::default(),
            receive_cap,
        );
        if index == 4 {
            let mut incoming_stage = result?;
            limited(
                incoming_stage
                    .metadata(receive_cap)
                    .err()
                    .ok_or("tight metadata owner succeeded")?,
            );
            drop(incoming_stage.into_store()?);
        } else {
            limited(result.err().ok_or("tight receive succeeded")?);
        }
        if index == 2 {
            assert_eq!(input.position(), 48, "pair bound must precede payload copy");
        }
        if index == 5 {
            assert_eq!(
                input.position(),
                0,
                "workspace admission must precede input"
            );
        }
    }
    exact_restore(&dir, &literal, &before, (r, c, w))?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn exact_restore(
    dir: &std::path::Path,
    literal: &[u8],
    before: &super::RawCohort,
    (r, c, w): (usize, usize, usize),
) -> TestResult {
    let scope = f::scope()?;
    let input_cap = profile((3, 325, 118), (326, 1), (r, c), w)?;
    let mut input = Cursor::new(&literal);
    let incoming_stage = RedbSnapshotStage::receive(
        &dir.join("exact.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        &mut input,
        StateStoreLimits::default(),
        input_cap,
    )?;
    assert_eq!(input.position(), 325);
    drop(incoming_stage.into_store()?);
    let install_cap = profile((3, 325, 118), (326, 1), (r, c), w + 514)?;
    let target = dir.join("target.redb");
    let mut target_store = RedbStateStore::create(
        &target,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &[],
            policies: &[],
        },
        StateStoreLimits::default().with_snapshot_limits(install_cap),
    )?;
    let incoming_stage = s::stage(&dir.join("larger-stage.redb"), literal)?;
    target_store.install_snapshot(incoming_stage, install_cap)?;
    drop(target_store);
    assert_eq!(s::raw(&target)?, *before);
    Ok(())
}

#[test]
fn whole_call_work_and_actual_target_content_are_bounded() -> TestResult {
    let dir = f::directory("snapshot-work")?;
    let path = dir.join("state.redb");
    s::seed(&path)?;
    s::capture(&path)?;
    let before = s::raw(&path)?;
    for work in [
        (1, 4_000_000_000, 100_000_000),
        (8_000_000, 1, 100_000_000),
        (8_000_000, 4_000_000_000, 1),
    ] {
        let cap = SnapshotLimits::new(
            (1_000_000, 1_073_741_824, 67_108_864),
            work,
            (10_000_000, 65_536),
            (f::MIB, 128 * f::MIB),
            256 * f::MIB,
        )?;
        let mut state = f::open(&path)?;
        limited(
            state
                .capture_snapshot(&mut Vec::new(), cap)
                .err()
                .ok_or("tight work succeeded")?,
        );
        control::fixtures::current(&mut state)?;
        drop(state);
        s::assert_raw(&path, &before)?;
    }
    let scope = f::scope()?;
    let wire = s::frame((b"g", b"z"), &s::authority(&before)?)?;
    let incoming = s::stage(&dir.join("stage.redb"), &wire)?;
    let cap = StateStoreLimits::new(
        AdmissionLimits::new(1, 16 * f::MIB, 1000)?,
        wf_domain::policy::complete::PolicySchemaLimits::default(),
        (1000, 128 * f::MIB),
        (1_000_000, 1024 * f::MIB, 10_000_000),
        128 * f::MIB,
        256 * f::MIB,
    )?;
    let placements = [
        (
            wf_types::identity::PartitionId::new(b"p".to_vec())?,
            scope.group().clone(),
        ),
        (
            wf_types::identity::PartitionId::new(b"x".to_vec())?,
            wf_types::identity::GroupId::new(b"foreign".to_vec())?,
        ),
    ];
    let policies = [(scope.tenant().clone(), f::rich_policy()?)];
    let target = dir.join("small-target.redb");
    let mut active = RedbStateStore::create(
        &target,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placements,
            policies: &policies,
        },
        cap,
    )?;
    drop(active);
    let target_before = s::raw(&target)?;
    active = RedbStateStore::open(
        &target,
        scope.group().clone(),
        scope.recovery().clone(),
        cap,
    )?;
    assert!(matches!(
        active.install_snapshot(incoming, SnapshotLimits::default()),
        Err(SnapshotError::Stage(StateTransactionError::Limit))
    ));
    let tx = active.begin_apply(
        scope.group(),
        scope.recovery(),
        cap.admission(),
        100,
        f::MIB,
        f::MIB,
    )?;
    assert_eq!(tx.control()?, ControlState::initial(f::clock(0)?));
    drop(tx);
    drop(active);
    s::assert_raw(&target, &target_before)?;
    captured_profile_refusal(&dir)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn captured_profile_refusal(dir: &std::path::Path) -> TestResult {
    let scope = f::scope()?;
    let cap = profile((3, 325, 118), (1000, 1), (2050, f::MIB), 2 * f::MIB)?;
    let path = dir.join("captured.redb");
    let mut active = RedbStateStore::create(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &[],
            policies: &[],
        },
        StateStoreLimits::default().with_snapshot_limits(cap),
    )?;
    let mut bytes = Vec::new();
    assert!(matches!(
        active.capture_snapshot(&mut bytes, SnapshotLimits::default()),
        Err(SnapshotError::ProfileMismatch)
    ));
    assert_eq!(bytes, [] as [u8; 0]);
    control::fixtures::current(&mut active)?;
    drop(active);
    let file = dir.join("unserved.redb");
    let db = redb::Builder::new().create(&file)?;
    let mut input = Cursor::new(s::hex(s::INITIAL)?);
    assert!(matches!(
        RedbSnapshotStage::receive_database(
            db,
            scope.group().clone(),
            scope.recovery().clone(),
            &mut input,
            StateStoreLimits::default().with_snapshot_limits(cap),
            SnapshotLimits::default()
        ),
        Err(SnapshotError::ProfileMismatch)
    ));
    assert_eq!(input.position(), 0);
    let db = redb::Builder::new().open(&file)?;
    assert_eq!(db.begin_read()?.list_tables()?.count(), 0);
    Ok(())
}

#[test]
fn old_artifact_uses_whole_call_work_and_its_captured_data_profile() -> TestResult {
    let dir = f::directory("snapshot-old-work")?;
    let path = dir.join("state.redb");
    drop(s::create(&path)?);
    let records = (0..200)
        .map(|n| {
            super::super::recovery::altered(format!("record-{n:03}").as_bytes(), b"z", n, Some(10))
        })
        .collect::<TestResult<Vec<_>>>()?;
    let rows = super::super::recovery::cohort(&f::scope()?, f::raw_rows(&path)?, &records, false)?;
    f::replace(&path, &rows, &f::raw_manifest(&path)?)?;
    s::capture(&path)?;
    let mut state = f::open(&path)?;
    let blank = control::fixtures::frame(false, control::fixtures::stamp(3, 0, 3)?)?;
    control::fixtures::apply(&mut state, &blank, f::log(1))?;
    drop(state);
    // A coherent post-cleanup fixture: immutable genesis and historical S remain.
    let mut rows = f::raw_rows(&path)?;
    rows.retain(|k, _| matches!(k.get(6), Some(0 | 6 | 7)));
    f::replace(&path, &rows, &f::raw_manifest(&path)?)?;
    let before = s::raw(&path)?;
    let authority = s::authority(&before)?;
    let expected = s::frame((b"g", b"z"), &authority)?;
    let n = authority
        .values()
        .map(std::collections::BTreeMap::len)
        .sum::<usize>();
    let pair = authority
        .values()
        .flat_map(|rows| rows.iter())
        .map(|(k, v)| k.len() + v.len())
        .max()
        .ok_or("no rows")?;
    let old = before[s::SNAP_ROWS].len() + 2;
    assert!(old > n);
    assert!(before[s::SNAP_ROWS].values().map(Vec::len).sum::<usize>() > expected.len());
    let data = (n, expected.len(), pair);
    let tight = SnapshotLimits::new(
        data,
        (old - 1, 4_000_000_000, 100_000_000),
        (10_000_000, 65_536),
        (f::MIB, 128 * f::MIB),
        256 * f::MIB,
    )?;
    state = f::open(&path)?;
    limited(
        state
            .capture_snapshot(&mut Vec::new(), tight)
            .err()
            .ok_or("old artifact work was free")?,
    );
    control::fixtures::current(&mut state)?;
    drop(state);
    s::assert_raw(&path, &before)?;
    let generous = profile(
        data,
        (10_000_000, 65_536),
        (f::MIB, 128 * f::MIB),
        256 * f::MIB,
    )?;
    state = f::open(&path)?;
    let mut wire = Vec::new();
    state.capture_snapshot(&mut wire, generous)?;
    assert_eq!(wire, expected);
    drop(state);
    s::assert_raw(&path, &s::installed(&authority)?)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn complete_membership_owner_is_reserved_before_output() -> TestResult {
    let dir = f::directory("snapshot-member-owner")?;
    let path = dir.join("state.redb");
    s::seed(&path)?;
    let actual_u = 1024 + 512 * 3 + 8 * 6 + 512 * 4 + 5;
    assert_eq!(actual_u, 4661);
    let r = 514 + 512 + actual_u;
    assert_eq!(r, 5687);
    let c = 16 * f::MIB;
    let w = 514 + 2048 + r + c;
    let data = (1_000_000, 1_073_741_824, 67_108_864);
    let mut state = f::open(&path)?;
    let mut wire = Vec::new();
    let meta =
        state.capture_snapshot(&mut wire, profile(data, (10_000_000, 65_536), (r, c), w)?)?;
    let member = control::fixtures::frame(true, control::fixtures::stamp(10, 3, 9)?)?;
    assert_eq!(
        meta.membership().entry(),
        Some(EntryIdentity::new(f::log(1), member.digest()))
    );
    assert_eq!(
        meta.membership().stamp(),
        Some(control::fixtures::stamp(10, 3, 9)?)
    );
    assert_eq!(
        meta.membership().membership().configs(),
        control::fixtures::membership()?.configs()
    );
    assert_eq!(
        meta.membership().membership().nodes(),
        control::fixtures::membership()?.nodes()
    );
    let mut forbidden = super::faults::WriteFault(Vec::new(), 0, false);
    assert!(matches!(
        state.capture_snapshot(
            &mut forbidden,
            profile(data, (10_000_000, 65_536), (r - 1, c), w)?
        ),
        Err(SnapshotError::Limit)
    ));
    assert_eq!(forbidden.0, [] as [u8; 0]);
    control::fixtures::current(&mut state)?;
    drop(state);
    let expected = s::installed(&s::authority(&s::raw(&path)?)?)?;
    s::assert_raw(&path, &expected)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
