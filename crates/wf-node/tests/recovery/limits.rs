use super::{
    CommittedRecoveryError as Error, CommittedRecoveryLimits as Limits, TestResult, assert_refusal,
    fixtures, recover_committed,
};
use wf_domain::workflow::{admission::AdmissionLimits, control::EntryStamp};
use wf_ports::{
    consensus::RaftMembershipLimits,
    log::{CodecLimits, RaftLogError, RaftLogStore},
};

fn profile(
    state: (usize, usize, usize, usize),
    work: (usize, usize),
    entry: usize,
    workspace: usize,
) -> TestResult<Limits> {
    Ok(Limits::new(
        RaftMembershipLimits::default(),
        state,
        fixtures::driver_limits(),
        work,
        entry,
        workspace,
    )?)
}

#[test]
fn committed_recovery_limits_validate_profiles_and_checked_aggregates() -> TestResult {
    let state = (100, 1536, 1024, 131);
    for (configured, work, entry, workspace) in [
        ((0, 1536, 1024, 131), (1, 13), 525, 5268),
        ((100, 0, 1024, 131), (1, 13), 525, 5268),
        ((100, 1536, 0, 131), (1, 13), 525, 5268),
        ((100, 1536, 1024, 0), (1, 13), 525, 5268),
        (state, (0, 13), 525, 5268),
        (state, (1, 0), 525, 5268),
        (state, (1, 13), 511, 5268),
        (state, (1, 13), 525, 0),
    ] {
        assert!(
            matches!(profile(configured, work, entry, workspace), Err(error)
            if error.downcast_ref::<Error>() == Some(&Error::InvalidLimits))
        );
    }
    for field in 0..6 {
        let mut normal = fixtures::driver_limits();
        match field {
            0 => normal.maximum_read_steps = 0,
            1 => normal.maximum_read_bytes = 0,
            2 => normal.maximum_staging_bytes = 0,
            3 => normal.maximum_adapter_bytes = 0,
            4 => normal.workspace_bytes = 0,
            _ => normal.outcome_ttl_ms = 0,
        }
        assert!(matches!(
            Limits::new(
                RaftMembershipLimits::default(),
                state,
                normal,
                (1, 13),
                525,
                5268
            ),
            Err(Error::InvalidLimits)
        ));
    }
    let cap = profile(state, (1, 13), 525, 5268)?;
    assert_eq!(cap.maximum_state_read_steps(), 100);
    assert_eq!(cap.maximum_applied_state_bytes(), 1536);
    assert_eq!(cap.maximum_control_apply_bytes(), 1024);
    assert_eq!(cap.maximum_temporary_bytes(), 131);
    assert_eq!(cap.maximum_log_points(), 1);
    assert_eq!(cap.maximum_log_body_bytes(), 13);
    assert_eq!(cap.maximum_owned_entry_bytes(), 525);
    assert_eq!(cap.workspace_bytes(), 5268);
    assert_eq!(cap.membership_limits(), RaftMembershipLimits::default());
    assert_eq!(cap.normal_limits().outcome_ttl_ms, 1000);
    Ok(())
}

#[test]
fn committed_recovery_point_wire_and_actual_guard_caps_are_prospective() -> TestResult {
    let state_caps = (100, 1536, 1024, 131);
    let dir = fixtures::directory("budget")?;
    let (mut log, state) = fixtures::create_pair(&dir)?;
    let blank = fixtures::blank(EntryStamp::Unstamped)?;
    assert_eq!(blank.as_bytes().len(), 13);
    fixtures::append(&mut log, 0, blank.as_bytes())?;
    log.save_committed(fixtures::at(0))?;
    drop((log, state));
    for (state, work, entry, workspace, expected) in [
        (state_caps, (1, 13), 525, 5267, Error::Limit),
        (
            state_caps,
            (1, 12),
            525,
            5268,
            Error::Log(RaftLogError::Limit),
        ),
        (
            state_caps,
            (1, 13),
            524,
            5268,
            Error::Log(RaftLogError::Limit),
        ),
        (
            (100, 1535, 1024, 131),
            (1, 13),
            525,
            5268,
            Error::State(wf_ports::state::StateTransactionError::Limit),
        ),
        (
            (100, 1536, 1023, 131),
            (1, 13),
            525,
            5268,
            Error::State(wf_ports::state::StateTransactionError::Limit),
        ),
        (
            (100, 1536, 1024, 130),
            (1, 13),
            525,
            5268,
            Error::State(wf_ports::state::StateTransactionError::Limit),
        ),
        (
            (100, usize::MAX, 1024, 131),
            (1, 13),
            525,
            usize::MAX,
            Error::Limit,
        ),
    ] {
        assert_refusal(&dir, profile(state, work, entry, workspace)?, expected)?;
    }
    let (log, state) = fixtures::open_pair(&dir)?;
    let s = fixtures::scope()?;
    let (log, mut state) = recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        profile(state_caps, (1, 13), 525, 5268)?,
    )?;
    assert_eq!(
        fixtures::applied(&mut state)?
            .0
            .last_entry()
            .map(wf_domain::workflow::control::EntryIdentity::log),
        Some(fixtures::at(0))
    );
    drop((log, state));
    // Historical audit and suffix share one wire allowance, without refunds.
    let (mut log, state) = fixtures::open_pair(&dir)?;
    fixtures::append(&mut log, 1, blank.as_bytes())?;
    log.save_committed(fixtures::at(1))?;
    drop((log, state));
    assert_refusal(&dir, profile(state_caps, (1, 26), 525, 5268)?, Error::Limit)?;
    assert_refusal(
        &dir,
        profile(state_caps, (2, 25), 525, 5268)?,
        Error::Log(RaftLogError::Limit),
    )?;
    let (log, state) = fixtures::open_pair(&dir)?;
    drop(recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        profile(state_caps, (2, 26), 525, 5268)?,
    )?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn committed_recovery_audit_decoder_and_external_large_scope_are_paid() -> TestResult {
    let dir = fixtures::directory("audit-decoder")?;
    let (mut log, mut state) = fixtures::create_pair(&dir)?;
    let frame = fixtures::frame(5, b"a", 1, vec![(b"i", fixtures::create_op())])?;
    let bytes = frame.as_bytes().len();
    fixtures::append(&mut log, 0, frame.as_bytes())?;
    drop(fixtures::apply_normal(&mut state, 0, frame.as_bytes(), 5)?);
    log.save_committed(fixtures::at(0))?;
    drop((log, state));
    let mut normal = fixtures::driver_limits();
    normal.codec = CodecLimits::new(AdmissionLimits::new(1024, 128, 1)?);
    let exact = 3588 + 512 + 4 * bytes + 128 + 1024 + 2048;
    let make = |workspace| {
        Limits::new(
            RaftMembershipLimits::default(),
            (100, 1536, 1024, 131),
            normal,
            (1, bytes),
            512 + bytes,
            workspace,
        )
    };
    assert_refusal(
        &dir,
        make(exact - 1)?,
        Error::Normal(wf_storage::transaction::RecordBatchTransactionError::Apply(
            wf_ports::log::RecordBatchApplyError::Workspace {
                attempted: 4 * bytes + 128 + 1024 + 2048,
                maximum: exact - 1 - 3588 - 512,
            },
        )),
    )?;
    let (log, state) = fixtures::open_pair(&dir)?;
    let s = fixtures::scope()?;
    drop(recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        make(exact)?,
    )?);
    std::fs::remove_dir_all(dir)?;
    large_scope_peak()
}

fn large_scope_peak() -> TestResult {
    let dir = fixtures::directory("large-scope")?;
    let s = fixtures::scope_for(&[0xff; 4096], b"g", b"z")?;
    let (mut log, state) = fixtures::create_pair_for(&dir, &s)?;
    let frame = fixtures::frame_for(&s, 5, b"a", 1, vec![(b"i", fixtures::create_op())])?;
    fixtures::append(&mut log, 0, frame.as_bytes())?;
    log.save_committed(fixtures::at(0))?;
    drop((log, state));
    let rows = fixtures::state_rows(&dir)?;
    let meta = fixtures::state_meta(&dir)?;
    let mut normal = fixtures::driver_limits();
    normal.workspace_bytes = 1;
    let q = 512 + s.tenant().as_bytes().len() + 1 + 1;
    let workspace = 3588 + 512 + frame.as_bytes().len() + q;
    let make = |workspace| {
        Limits::new(
            RaftMembershipLimits::default(),
            (100, 1536, 1024, 131),
            normal,
            (1, frame.as_bytes().len()),
            512 + frame.as_bytes().len(),
            workspace,
        )
    };
    for (workspace, expected) in [
        (workspace - 1, Error::Limit),
        (
            workspace,
            Error::Normal(wf_storage::transaction::RecordBatchTransactionError::Apply(
                wf_ports::log::RecordBatchApplyError::Workspace {
                    attempted: 160 * fixtures::MIB,
                    maximum: 1,
                },
            )),
        ),
    ] {
        let (log, state) = fixtures::open_pair_for(&dir, &s)?;
        let Err(error) = recover_committed(log, state, s.group(), s.recovery(), make(workspace)?)
        else {
            return Err("unpaid large external scope accepted".into());
        };
        assert_eq!(error, expected);
        drop(fixtures::open_pair_for(&dir, &s)?);
        assert_eq!(fixtures::state_rows(&dir)?, rows);
        assert_eq!(fixtures::state_meta(&dir)?, meta);
    }
    large_scope_success(&dir, &s)?;
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

fn large_scope_success(
    dir: &std::path::Path,
    s: &wf_domain::workflow::request::RequestScope,
) -> TestResult {
    let (mut log, state) = fixtures::open_pair_for(dir, s)?;
    let other = fixtures::scope_for(b"other\0\xff", b"g", b"z")?;
    let second = fixtures::frame_for(&other, 5, b"a", 2, vec![(b"i", fixtures::create_op())])?;
    fixtures::append(&mut log, 1, second.as_bytes())?;
    log.save_committed(fixtures::at(1))?;
    let (log, mut state) =
        recover_committed(log, state, s.group(), s.recovery(), super::limits()?)?;
    {
        use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
        let tx = state.begin_apply(
            s.group(),
            s.recovery(),
            AdmissionLimits::default(),
            100,
            16 * fixtures::MIB,
            16 * fixtures::MIB,
        )?;
        for tenant in [s.tenant(), other.tenant()] {
            let record = tx
                .record(
                    tenant,
                    &wf_types::identity::PartitionId::new(b"p".to_vec())?,
                    &wf_types::identity::RecordId::new(b"i".to_vec())?,
                )?
                .ok_or("missing tenant record")?;
            assert_eq!(record.tenant, *tenant);
            assert_eq!(record.record.version(), 1);
            let outcome = tx
                .outcome(tenant, &wf_types::identity::RequestId::new(b"a".to_vec())?)?
                .ok_or("missing tenant outcome")?;
            assert_eq!(outcome.scope().tenant(), tenant);
            assert_eq!(outcome.response(), [0]);
        }
    }
    drop((log, state));
    Ok(())
}

#[test]
fn committed_recovery_membership_observation_has_real_captured_profiles() -> TestResult {
    let dir = fixtures::directory("member-budget")?;
    let (mut log, mut state) = fixtures::create_pair(&dir)?;
    let member = fixtures::member(EntryStamp::Unstamped, 4)?;
    assert_eq!(member.as_bytes().len(), 134);
    fixtures::append(&mut log, 0, member.as_bytes())?;
    fixtures::apply_control(&mut state, 0, member.as_bytes())?;
    log.save_committed(fixtures::at(0))?;
    drop((log, state));
    let make = |membership, read| {
        Limits::new(
            membership,
            (100, read, 1024, 131),
            fixtures::driver_limits(),
            (1, 134),
            646,
            8892,
        )
    };
    for fields in [
        (2, 6, 4, 2, 134, 4661),
        (3, 5, 4, 2, 134, 4661),
        (3, 6, 3, 2, 134, 4661),
        (3, 6, 4, 1, 134, 4661),
        (3, 6, 4, 2, 133, 4661),
        (3, 6, 4, 2, 134, 4660),
    ] {
        let p =
            RaftMembershipLimits::new(fields.0, fields.1, fields.2, fields.3, fields.4, fields.5)?;
        assert_refusal(
            &dir,
            make(p, 5173)?,
            Error::State(wf_ports::state::StateTransactionError::Limit),
        )?;
    }
    let exact = RaftMembershipLimits::new(3, 6, 4, 2, 134, 4661)?;
    assert_refusal(
        &dir,
        make(exact, 5172)?,
        Error::State(wf_ports::state::StateTransactionError::Limit),
    )?;
    let larger =
        RaftMembershipLimits::new(65, 1_000_000, 65_536, 1_048_576, 16_777_216, 67_108_864)?;
    assert_refusal(
        &dir,
        make(larger, 5173)?,
        Error::State(wf_ports::state::StateTransactionError::InvalidChanges),
    )?;
    let (log, state) = fixtures::open_pair(&dir)?;
    let s = fixtures::scope()?;
    drop(recover_committed(
        log,
        state,
        s.group(),
        s.recovery(),
        make(exact, 5173)?,
    )?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
