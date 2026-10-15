//! Caller projection follows the real plan and preserves retained ETF identity.

use super::{
    AdmissionLimits, ApplyError, BTreeMap, ControlState, DispatchDecision, GateView,
    OriginalEtfSafety, PolicySchemaCharge, PolicySchemaLimits, Raw, ReadValue, RequestId,
    RetainedOutcome, RetainedStatus, TestResult, accepted, apply, empty_orders, frame, frame_with,
    minimal, profile, scope, stamp, view,
};
use wf_domain::policy::complete::{OriginalPolicyReplyContext, present_original_policy_set_reply};
use wf_domain::retention::ResolvedRetention;

#[test]
fn public_set_caller_projects_fresh_and_retained_replay_on_current_defaults() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let namespace = scope()?;
    let no_outcomes = BTreeMap::new();
    let policies = BTreeMap::new();
    let marks = BTreeMap::new();
    let orders = empty_orders();
    let bytes = frame(Raw::List(&[]), &orders)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let plan = apply(
        &view(
            GateView {
                scope: &namespace,
                control: ControlState::initial(stamp(0)?),
                outcomes: &no_outcomes,
            },
            &policies,
            &marks,
            &marks,
            super::safety(),
        ),
        &bytes,
        &orders,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    accepted(&plan, 0, 1, &minimal(1)?)?;
    let (_, _, control, decision) = plan.into_parts();
    let DispatchDecision::Fresh((Some(install), outcome)) = decision else {
        return Err("real Fresh installation and retained envelope".into());
    };
    drop(bytes);
    let additional_live =
        header_live(&outcome) + install.policy_key().len() + install.encoded().len();
    assert_eq!(additional_live, 581);
    assert_public_reply(
        &outcome,
        ResolvedRetention {
            ttl_ms: 53,
            history_max_events: 17,
        },
        additional_live,
        3_073,
        limits,
        &mut charge,
    )?;
    assert_eq!(outcome.response(), minimal(1)?);
    drop(install);
    assert_replay(control.next(), outcome, limits, &mut charge)
}

fn assert_replay(
    control: ControlState,
    outcome: RetainedOutcome,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let namespace = scope()?;
    let request = RequestId::new(b"q".to_vec())?;
    let outcomes = BTreeMap::from([(request.clone(), outcome)]);
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Other)]);
    let failed = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    let orders = empty_orders();
    let bytes = frame_with(Raw::List(&[]), &orders, b"q", 1)?;
    let mut supplied = view(
        GateView {
            scope: &namespace,
            control,
            outcomes: &outcomes,
        },
        &policies,
        &failed,
        &failed,
        OriginalEtfSafety::new(&["duplicate", "duplicate"], &[], None, &[]),
    );
    supplied.history_max_events_limit = Err(ApplyError::InvalidAuthority);
    supplied.outcome_retention_ms = Err(ApplyError::AllocationFailed);
    let plan = apply(
        &supplied,
        &bytes,
        &orders,
        1,
        limits,
        AdmissionLimits::default(),
        charge,
    )?;
    drop(bytes);
    let DispatchDecision::Replay(replayed) = plan.decision() else {
        return Err("retained Replay before poisoned current and context".into());
    };
    assert!(std::ptr::eq(
        std::ptr::from_ref(*replayed),
        std::ptr::from_ref(&outcomes[&request])
    ));
    let additional_live = header_live(replayed);
    assert_eq!(additional_live, 516);
    assert_public_reply(
        replayed,
        ResolvedRetention {
            ttl_ms: 61,
            history_max_events: 23,
        },
        additional_live,
        3_008,
        limits,
        charge,
    )?;
    assert_eq!(outcomes[&request].response(), minimal(1)?);
    assert_eq!(outcomes[&request].expires_at().get(), 100);
    Ok(())
}

fn assert_public_reply(
    outcome: &RetainedOutcome,
    defaults: ResolvedRetention,
    additional_live: usize,
    resident: u64,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.response_version(), 1);
    assert_eq!(outcome.intent().version(), 4);
    let reply = present_original_policy_set_reply(
        b"r",
        outcome.response(),
        None,
        OriginalPolicyReplyContext::new(super::safety(), defaults, None, None, additional_live),
        limits,
        charge,
    )?
    .map_err(|_| "accepted retained envelope must project")?;
    assert_eq!(reply.generation(), 1);
    assert_eq!(reply.requested_type(), b"r");
    assert_eq!(reply.requested_state(), None);
    assert_eq!(reply.root().map_len(), Some(10));
    // A new matching account must admit the actual continuous live owners.
    // P2492 comes from the independent minimal ETF/index/catalog/State recipe;
    // Fresh adds581 and Replay516, with response V51 included once.
    let mut observation = PolicySchemaCharge::new(limits);
    let retention = reply
        .root()
        .atom_field("retention", &mut observation)?
        .ok_or("public retention map")?;
    assert_eq!(observation.p(), resident);
    assert_eq!(retention.map_len(), Some(2));
    assert_eq!(
        retention
            .atom_field("ttl_ms", &mut observation)?
            .ok_or("ttl")?
            .integer(),
        Some(i128::from(defaults.ttl_ms))
    );
    assert_eq!(
        retention
            .atom_field("history_max_events", &mut observation)?
            .ok_or("history")?
            .integer(),
        Some(i128::from(defaults.history_max_events))
    );
    assert!(
        retention
            .atom_field("history_hot_max_events", &mut observation)?
            .is_none()
    );
    assert_eq!(observation.p(), resident);
    assert_eq!(outcome.response(), minimal(1)?);
    Ok(())
}

fn header_live(outcome: &RetainedOutcome) -> usize {
    512 + outcome.scope().tenant().as_bytes().len()
        + outcome.scope().group().as_bytes().len()
        + outcome.scope().recovery().as_bytes().len()
        + outcome.request().as_bytes().len()
}
