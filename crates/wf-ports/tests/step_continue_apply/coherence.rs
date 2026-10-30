use super::{fixtures as f, *};
use wf_domain::history::EventKind;
use wf_domain::value::DomainError;
use wf_ports::state::{GroupApplyChanges, StateTransactionError};

fn changes<'scope, 'current>(
    scope: &'scope RequestScope,
    plan: RecordBatchApplyPlan<'current>,
) -> TestResult<GroupApplyChanges<'scope, 'current>> {
    let result = GroupApplyChanges::new(scope, f::genesis()?, plan);
    assert!(
        result.is_ok(),
        "healthy Step transaction changes: {:?}",
        result.as_ref().err()
    );
    Ok(result?)
}

#[test]
fn step_changes_preserve_complete_accepted_units_and_refuse_crossed_authority() -> TestResult {
    let scope = f::scope()?;
    let record = f::record()?;
    for payload in [None, Some(b"".as_slice()), Some(b"new".as_slice())] {
        let frame = f::frame(2, b"charge", payload)?;
        let plan = f::plan(&frame, &record, StateMode::Parallel)?;
        let changes = changes(&scope, plan)?;
        assert_eq!(changes.expected(), f::genesis()?);
        let control = changes.control().ok_or("new control")?;
        assert_eq!(control.position(), AppliedPosition::new(1));
        assert_eq!(changes.scope(), &scope);
        let business = changes.business().ok_or("accepted business")?;
        assert_eq!(business.batch().items().len(), 1);
        let item = business.batch().items()[0].plan();
        let write = item.write().ok_or("accepted postimage")?;
        assert_eq!(write.expected_version, Some(2));
        assert_eq!(write.record.run_state(), Some(b"charge".as_slice()));
        assert_eq!(write.record.payload(), Some(payload.unwrap_or(b"old")));
        let history = item.history().ok_or("matching history")?;
        assert_eq!(history.kind, EventKind::StepContinued);
        assert_eq!((history.version, history.fence), (3, Fence::new(2)));
        assert_eq!(
            business
                .metadata()
                .next()
                .ok_or("ordering")?
                .state_enter_seq,
            1_u128 << 32
        );
        let outcome = changes.outcome().ok_or("retained outcome")?;
        assert_eq!(
            (outcome.intent().version(), outcome.response_version()),
            (10, 9)
        );
        assert_eq!(outcome.original_position(), f::log(1));
        assert_eq!(outcome.original_time(), TimestampMillis::new(2));
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        assert_eq!(
            changes
                .plan()
                .value_effects()
                .ok_or("value provenance")?
                .steps()
                .len(),
            1
        );
        assert!(changes.independent_business().is_none());
    }
    let frame = f::frame(2, b"charge", None)?;
    let foreign = RequestScope::new(
        TenantId::new(b"foreign".to_vec())?,
        scope.group().clone(),
        scope.recovery().clone(),
    );
    for expected_scope in [&scope, &foreign] {
        let predecessor = if expected_scope == &scope {
            ControlState::initial(f::clock(0)?)
        } else {
            f::genesis()?
        };
        assert_eq!(
            GroupApplyChanges::new(
                expected_scope,
                predecessor,
                f::plan(&frame, &record, StateMode::Parallel)?
            )
            .err(),
            Some(StateTransactionError::InvalidChanges)
        );
    }
    assert_eq!(
        (record.version(), record.payload()),
        (2, Some(b"old".as_slice()))
    );
    Ok(())
}

#[test]
fn step_changes_rejections_retain_only_original_outcome_and_control() -> TestResult {
    let scope = f::scope()?;
    let record = f::record()?;
    let healthy = f::frame(2, b"charge", None)?;
    drop(changes(
        &scope,
        f::plan(&healthy, &record, StateMode::Parallel)?,
    )?);
    for fifo in [false, true] {
        let mut request = f::request(2, b"charge", None)?;
        let PreparedOperation::Batch(items) = &mut request.operation else {
            return Err("fixture batch".into());
        };
        let Operation::StepContinue(step) = &mut items[0].command.operation else {
            return Err("fixture Step".into());
        };
        if !fifo {
            step.guard.token = LeaseToken::new(b"stale".to_vec())?;
        }
        let frame = encode_step_continue(request, f::limits()?, WORKSPACE)?;
        let mode = if fifo {
            StateMode::Fifo
        } else {
            StateMode::Parallel
        };
        let changes = changes(&scope, f::plan(&frame, &record, mode)?)?;
        let reason = if fifo {
            IndexRejection::DirectRunningFifoEntry
        } else {
            IndexRejection::Batch(BatchRejection::Domain(DomainError::StaleLease))
        };
        assert!(changes.business().is_none());
        assert!(changes.independent_business().is_none());
        assert!(changes.plan().value_effects().is_none());
        let outcome = changes.outcome().ok_or("rejected retained outcome")?;
        assert_eq!(outcome.status(), RetainedStatus::IndexedRejected(reason));
        assert_eq!(outcome.original_position(), f::log(1));
        assert_eq!(
            changes.control().ok_or("rejection control")?.position(),
            AppliedPosition::new(1)
        );
    }
    assert_eq!(record.version(), 2);
    Ok(())
}

#[test]
fn step_changes_replay_conflict_expiry_and_log_replay_need_no_current_record() -> TestResult {
    let scope = f::scope()?;
    let record = f::record()?;
    let frame = f::frame(2, b"charge", None)?;
    let original = changes(&scope, f::plan(&frame, &record, StateMode::Parallel)?)?;
    let predecessor = original.control().ok_or("original control")?;
    let (_, RecordBatchApplyDecision::Fresh { outcome, .. }) = original.into_plan().into_parts()
    else {
        return Err("original Fresh".into());
    };
    drop(record);
    for (case, frame, index) in [
        (0, f::frame(3, b"charge", None)?, 2),
        (1, f::frame(3, b"charge", Some(b"different"))?, 2),
        (2, f::frame(102, b"charge", None)?, 2),
        (3, frame, 1),
    ] {
        let inspection = inspect_step_continue_point(
            prepare_step_continue(frame.as_bytes(), f::limits()?, WORKSPACE)?,
            f::log(index),
            PointGateView {
                scope: &scope,
                control: predecessor,
                outcome: Some(&outcome),
                retained_nonempty: true,
            },
            f::limits()?,
            WORKSPACE,
        )?;
        let RecordBatchInspection::Complete(plan) = inspection else {
            return Err("unexpected Fresh".into());
        };
        let changes = GroupApplyChanges::new(&scope, predecessor, plan)?;
        match (case, changes.plan().decision()) {
            (0, RecordBatchApplyDecision::Replay(retained)) => {
                assert_eq!(*retained, outcome.as_ref());
                assert_eq!(retained.original_position(), f::log(1));
            }
            (1, RecordBatchApplyDecision::IntentConflict)
            | (2, RecordBatchApplyDecision::Expired)
            | (3, RecordBatchApplyDecision::LogReplay) => {}
            _ => return Err("wrong retained classification".into()),
        }
        assert!(changes.business().is_none());
        assert!(changes.independent_business().is_none());
        assert!(changes.outcome().is_none());
        assert!(changes.plan().value_effects().is_none());
        assert_eq!(changes.control().is_none(), case == 3);
    }
    Ok(())
}
