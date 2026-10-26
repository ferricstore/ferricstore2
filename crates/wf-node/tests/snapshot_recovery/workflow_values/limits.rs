//! Independent fixed headers, outer owners, codec cohorts and value ceilings.

use std::mem::size_of;

use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::EntryStamp;
use wf_node::recovery::SnapshotRecoveryError;
use wf_ports::consensus::{
    PreparedBatchItem, PreparedWorkflowValueBatch, RaftMembershipLimits, WorkflowValueItemInput,
    WorkflowValuePolicyWitness,
};
use wf_ports::log::{
    CodecError, CodecLimits, DecodedRequest, RaftLogStore, prepare_workflow_value_batch,
};
use wf_ports::state::StateTransactionError;
use wf_storage::transaction::{
    RecordBatchTransactionError, RecordBatchTransactionLimits, WorkflowValueTransactionError,
    WorkflowValueTransactionLimits,
};

use super::{Error, Limits, TestResult, f, fixtures as v, recover_committed};

#[test]
fn value_recovery_limits_admit_headers_decoders_and_outer_scope() -> TestResult {
    let enabled =
        v::limits()?.with_workflow_value_limits(1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB);
    assert!(
        enabled.is_ok(),
        "explicit value recovery opt-in must succeed"
    );
    let limits = enabled?;
    assert!(v::limits()?.workflow_value_limits().is_none());
    let selected = limits
        .workflow_value_limits()
        .ok_or("enabled limits missing")?;
    assert_eq!(selected.maximum_value_rows, 1_024);
    assert_eq!(selected.maximum_value_bytes, 64 * f::MIB);
    assert_eq!(selected.maximum_value_work, 64 * f::MIB);
    assert_eq!(selected.maximum_value_temporary_bytes, 64 * f::MIB);
    assert_eq!(selected.record_batch.workspace_bytes, 512 * f::MIB);
    assert_eq!(selected.record_batch.outcome_ttl_ms, 1_000);
    for (rows, bytes, work, temporary) in [(0, 1, 1, 1), (1, 0, 1, 1), (1, 1, 0, 1), (1, 1, 1, 0)] {
        assert!(matches!(
            v::limits()?.with_workflow_value_limits(rows, bytes, work, temporary),
            Err(Error::InvalidLimits)
        ));
    }
    let overflow = profile(
        limits.normal_limits(),
        usize::MAX,
        (usize::MAX, 1, 512),
        (100, 1536, 1024, 131),
    )?;
    assert!(matches!(
        overflow.with_workflow_value_limits(1, 1, 2, 1),
        Err(Error::Limit)
    ));
    disabled_blank_headers()?;
    audit_owners_and_work()?;
    replay_value_caps(&limits)?;
    external_scope(&limits)?;
    Ok(())
}

fn fixed() -> usize {
    size_of::<Option<[usize; 4]>>() + size_of::<Error>().max(size_of::<SnapshotRecoveryError>())
}

fn base() -> usize {
    // Two namespace owners for literal group g/recovery z plus fixed phases.
    512 + 1 + 1 + 512 + 1 + 1 + 2_048 + 512 + fixed()
}

fn profile(
    normal: RecordBatchTransactionLimits,
    workspace: usize,
    points: (usize, usize, usize),
    state: (usize, usize, usize, usize),
) -> TestResult<Limits> {
    Ok(Limits::new(
        RaftMembershipLimits::default(),
        state,
        normal,
        (points.0, points.1),
        points.2,
        workspace,
    )?)
}

fn disabled_blank_headers() -> TestResult {
    let dir = f::directory("value-disabled-fixed-header")?;
    let (mut log, state) = f::create_pair(&dir)?;
    let blank = f::blank(EntryStamp::Unstamped)?;
    assert_eq!(blank.as_bytes().len(), 13);
    f::append(&mut log, 0, blank.as_bytes())?;
    log.save_committed(f::at(0))?;
    drop((log, state));
    let before = super::super::fixtures::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let metadata = f::log_meta(&dir)?;
    let make = |outer| {
        profile(
            f::driver_limits(),
            outer,
            (1, 13, 525),
            (100, 1536, 1024, 131),
        )
    };
    for outer in [5_268, 5_268 + fixed() - 1] {
        let scope = f::scope()?;
        let (log, state) = f::open_pair(&dir)?;
        assert_eq!(
            recover_committed(log, state, scope.group(), scope.recovery(), make(outer)?).err(),
            Some(Error::Limit),
        );
        drop(f::open_pair(&dir)?);
        assert_eq!(super::super::fixtures::cohort(&dir)?, before);
        assert_eq!(f::log_rows(&dir)?, logs);
        assert_eq!(f::log_meta(&dir)?, metadata);
    }
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        make(5_268 + fixed())?,
    )?);
    drop(f::open_pair(&dir)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn audit_owners_and_work() -> TestResult {
    let dir = f::directory("value-audit-owner-caps")?;
    let (mut log, mut state) = v::pair(&dir)?;
    let batch = v::batch(
        &f::scope()?,
        5,
        b"a",
        100,
        vec![v::create(b"i", 100, &[0x41; 2_048])?],
        v::witness(None)?,
    )?;
    let frame = v::encode(&batch)?;
    f::append(&mut log, 0, frame.as_bytes())?;
    v::accepted(v::apply(&mut state, &frame, 0)?, 0, 100, 5)?;
    log.save_committed(f::at(0))?;
    let encoded = frame.as_bytes().len();
    let owned = size_of::<PreparedWorkflowValueBatch>()
        + size_of::<DecodedRequest<'static>>()
        + size_of::<PreparedBatchItem>()
        + size_of::<WorkflowValueItemInput>()
        + size_of::<WorkflowValuePolicyWitness>()
        + 2 * encoded
        + size_of::<Vec<u8>>()
        + 62;
    let work = 65_536 + 4_096 + 1_024 + 128 * encoded + 64 * 62;
    let normal = v::limits()?.normal_limits();
    let (decoded, spent) =
        prepare_workflow_value_batch(frame.as_bytes(), normal.codec, owned, work)?;
    assert_eq!(spent, work);
    assert_eq!(decoded.isolation_scope().len(), 21);
    assert_eq!(decoded.state_key(0).ok_or("state key absent")?.len(), 41);
    drop((decoded, log, state, batch));
    audit_boundaries(&dir, normal, encoded, owned, work)?;
    drop(frame);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn audit_boundaries(
    dir: &std::path::Path,
    normal: RecordBatchTransactionLimits,
    encoded: usize,
    owned: usize,
    work: usize,
) -> TestResult {
    let qv = size_of::<Option<WorkflowValueTransactionLimits>>();
    let j = 512 + encoded;
    let get = base() + j + qv;
    let make = |outer| profile(normal, outer, (1, encoded, j), (100, 1536, 1024, 131));
    v::refused(dir, &make(get - 1)?, Error::Limit)?;
    v::refused(dir, &make(get)?, Error::UnsupportedEntry)?;
    let outer = get + owned;
    let enabled = |maximum, value_work| -> TestResult<Limits> {
        Ok(make(maximum)?.with_workflow_value_limits(
            1_024,
            64 * f::MIB,
            value_work,
            64 * f::MIB,
        )?)
    };
    v::refused(
        dir,
        &enabled(outer - 1, work)?,
        Error::Codec(CodecError::OwnedLimit),
    )?;
    v::refused(
        dir,
        &enabled(outer, work - 1)?,
        Error::Codec(CodecError::WorkLimit),
    )?;
    let mut short = normal;
    short.codec = CodecLimits::new(AdmissionLimits::new(4_096, encoded - 1, 1)?);
    let cap = profile(short, outer, (1, encoded, j), (100, 1536, 1024, 131))?
        .with_workflow_value_limits(1_024, 64 * f::MIB, work, 64 * f::MIB)?;
    v::refused(dir, &cap, Error::Codec(CodecError::ByteLimit))?;
    let before = v::cohort(dir)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(dir)?;
    drop(recover_committed(
        log,
        state,
        scope.group(),
        scope.recovery(),
        enabled(outer, work)?,
    )?);
    assert_eq!(v::cohort(dir)?, before);
    drop(f::open_pair(dir)?);
    Ok(())
}

fn replay_value_caps(limits: &Limits) -> TestResult {
    let dir = f::directory("value-driver-cap-stages")?;
    let (mut log, state) = v::pair(&dir)?;
    let frame = v::encode(&v::batch(
        &f::scope()?,
        5,
        b"caps",
        100,
        vec![v::create(b"i", 100, b"A")?],
        v::witness(None)?,
    )?)?;
    f::append(&mut log, 0, frame.as_bytes())?;
    log.save_committed(f::at(0))?;
    drop((log, state));
    let expected = Error::Value(WorkflowValueTransactionError::Record(
        RecordBatchTransactionError::Transaction(StateTransactionError::Limit),
    ));
    for (rows, bytes, work, temporary) in [
        (3, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB),
        (4, 1, 64 * f::MIB, 64 * f::MIB),
        (4, 64 * f::MIB, 1, 64 * f::MIB),
        (4, 64 * f::MIB, 64 * f::MIB, 1),
    ] {
        let cap = v::limits()?.with_workflow_value_limits(rows, bytes, work, temporary)?;
        v::refused(&dir, &cap, expected)?;
    }
    let positive =
        (*limits).with_workflow_value_limits(4, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?;
    let scope = f::scope()?;
    let (log, state) = f::open_pair(&dir)?;
    let (log, state) = recover_committed(log, state, scope.group(), scope.recovery(), positive)?;
    drop((log, state));
    let cohort = v::cohort(&dir)?;
    for family in 8..=11 {
        assert_eq!(
            cohort[1]
                .keys()
                .filter(|key| key.get(6) == Some(&family))
                .count(),
            1
        );
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn external_scope(limits: &Limits) -> TestResult {
    let dir = f::directory("value-external-large-scope")?;
    let scope = f::scope_for(&[0xff; 4_096], b"g", b"z")?;
    let (mut log, state) = v::pair_for(&dir, &scope)?;
    let frame = v::encode(&v::batch(
        &scope,
        5,
        b"large",
        100,
        vec![v::create(b"i", 100, b"A")?],
        v::witness(None)?,
    )?)?;
    f::append(&mut log, 0, frame.as_bytes())?;
    log.save_committed(f::at(0))?;
    drop((log, state));
    let before = v::cohort(&dir)?;
    let logs = f::log_rows(&dir)?;
    let metadata = f::log_meta(&dir)?;
    let mut normal = limits.normal_limits();
    normal.workspace_bytes = 1;
    let q = 512 + 4_096 + 1 + 1;
    let exact = base()
        + size_of::<Option<WorkflowValueTransactionLimits>>()
        + 512
        + frame.as_bytes().len()
        + q
        + 1;
    for (outer, expected) in [
        (exact - 1, Error::Limit),
        (
            exact,
            Error::Value(WorkflowValueTransactionError::Record(
                RecordBatchTransactionError::Transaction(StateTransactionError::Limit),
            )),
        ),
    ] {
        let cap = profile(
            normal,
            outer,
            (1, frame.as_bytes().len(), 512 + frame.as_bytes().len()),
            (100, 1536, 1024, 131),
        )?
        .with_workflow_value_limits(1_024, 64 * f::MIB, 64 * f::MIB, 64 * f::MIB)?;
        let (log, state) = f::open_pair_for(&dir, &scope)?;
        assert_eq!(
            recover_committed(log, state, scope.group(), scope.recovery(), cap).err(),
            Some(expected)
        );
        drop(f::open_pair_for(&dir, &scope)?);
        assert_eq!(v::cohort(&dir)?, before);
        assert_eq!(f::log_rows(&dir)?, logs);
        assert_eq!(f::log_meta(&dir)?, metadata);
    }
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
