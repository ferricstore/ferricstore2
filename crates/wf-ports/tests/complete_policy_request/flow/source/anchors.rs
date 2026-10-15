//! Direct source-component observations; public and physical assertions stay open.

use super::super::cases::{scope, stamp};
use super::{
    BTreeMap, GateView, PolicyError, PolicySchemaCharge, PolicySchemaLimits, Raw, Reason,
    Rejection, RetainedStatus, RetentionReason, TestResult, allocator, allocator_limit, atom,
    binary, expected, map, number, policy, retained, take,
};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::DispatchDecision;

#[test]
fn original_flow_source_monotonic_generation_values() -> TestResult {
    with_gate(|gate, limits| {
        let ty = b"generation-always-on-1";
        let first_bytes = expected(1, ty, &[("max_active_ms", number(1_000)?)])?;
        let second_bytes = expected(2, ty, &[("max_active_ms", number(2_000)?)])?;
        let initial = [(Raw::Atom("max_active_ms"), Raw::Integer(1_000))];
        let updated = [(Raw::Atom("max_active_ms"), Raw::Integer(2_000))];
        let mut charge = PolicySchemaCharge::new(limits);
        let (first, _) = take(allocator(
            gate,
            (ty, None),
            (Raw::Nil, Raw::Map(&initial)),
            (0, b"q"),
            limits,
            &mut charge,
        )?)?;
        let first = first.0.ok_or("first original installation")?;
        assert_eq!(first.generation(), 1);
        assert_eq!(first.encoded(), first_bytes);
        let (second, _) = take(allocator(
            gate,
            (ty, Some(first.encoded())),
            (Raw::Nil, Raw::Map(&updated)),
            (0, b"s"),
            limits,
            &mut charge,
        )?)?;
        let second = second.0.ok_or("second original installation")?;
        assert_eq!(second.generation(), 2);
        assert_eq!(second.encoded(), second_bytes);
        assert_eq!(first.encoded(), first_bytes);
        Ok(())
    })
}

#[test]
fn original_flow_source_stale_generation_values() -> TestResult {
    with_gate(|gate, limits| {
        let ty = b"generation-cas-1";
        let first_bytes = expected(1, ty, &[("max_active_ms", number(1_000)?)])?;
        let second_bytes = expected(2, ty, &[("max_active_ms", number(2_000)?)])?;
        let initial = [(Raw::Atom("max_active_ms"), Raw::Integer(1_000))];
        let updated = [(Raw::Atom("max_active_ms"), Raw::Integer(2_000))];
        let stale = [(Raw::Atom("max_active_ms"), Raw::Integer(3_000))];
        let mut charge = PolicySchemaCharge::new(limits);
        let (first, _) = take(allocator(
            gate,
            (ty, None),
            (Raw::Nil, Raw::Map(&initial)),
            (0, b"q"),
            limits,
            &mut charge,
        )?)?;
        let first = first.0.ok_or("first CAS installation")?;
        assert_eq!(first.generation(), 1);
        assert_eq!(first.encoded(), first_bytes);
        let (second, _) = take(allocator(
            gate,
            (ty, Some(first.encoded())),
            (Raw::Integer(1), Raw::Map(&updated)),
            (0, b"s"),
            limits,
            &mut charge,
        )?)?;
        let second = second.0.ok_or("second CAS installation")?;
        assert_eq!(second.generation(), 2);
        assert_eq!(second.encoded(), second_bytes);
        let rejected = allocator(
            gate,
            (ty, Some(second.encoded())),
            (Raw::Integer(1), Raw::Map(&stale)),
            (0, b"t"),
            limits,
            &mut charge,
        )?;
        let out = retained(&rejected)?;
        assert!(
            matches!(out.status(), RetainedStatus::PolicyRejected(reason)
            if reason.error() == PolicyError::StaleGeneration)
        );
        assert_eq!(out.response(), b"ERR stale flow policy generation");
        assert!(matches!(
            rejected.decision(),
            DispatchDecision::Fresh((None, _))
        ));
        assert_eq!(second.encoded(), second_bytes);
        Ok(())
    })
}

#[test]
fn original_flow_source_replacement_omission_values() -> TestResult {
    with_gate(|gate, limits| {
        let ty = b"generation-replace-1";
        let queued = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
        let states = [(Raw::Binary(b"queued"), Raw::Map(&queued))];
        let initial = [
            (Raw::Atom("max_active_ms"), Raw::Integer(1_000)),
            (Raw::Atom("states"), Raw::Map(&states)),
        ];
        let first_bytes = expected(
            1,
            ty,
            &[
                ("max_active_ms", number(1_000)?),
                (
                    "states",
                    map(&[(
                        binary(b"queued")?,
                        map(&[(atom(b"mode")?, atom(b"fifo")?)])?,
                    )])?,
                ),
            ],
        )?;
        let second_bytes = expected(2, ty, &[])?;
        let mut charge = PolicySchemaCharge::new(limits);
        let (first, _) = take(allocator(
            gate,
            (ty, None),
            (Raw::Nil, Raw::Map(&initial)),
            (0, b"q"),
            limits,
            &mut charge,
        )?)?;
        let first = first.0.ok_or("initial FIFO policy")?;
        assert_eq!(first.generation(), 1);
        assert_eq!(first.encoded(), first_bytes);
        let (second, _) = take(allocator(
            gate,
            (ty, Some(first.encoded())),
            (Raw::Nil, Raw::Map(&[])),
            (0, b"s"),
            limits,
            &mut charge,
        )?)?;
        let second = second.0.ok_or("reset policy")?;
        assert_eq!(second.generation(), 2);
        assert_eq!(second.encoded(), second_bytes);
        assert_eq!(first.encoded(), first_bytes);
        Ok(())
    })
}

#[test]
fn original_flow_source_cross_policy_generation_values() -> TestResult {
    with_gate(|gate, limits| {
        let ty = b"current-cross-policy-wire";
        let first_bytes = expected(1, ty, &[("max_active_ms", number(1_000)?)])?;
        let second_bytes = expected(2, ty, &[("max_active_ms", number(3_000)?)])?;
        let initial = [
            (Raw::Atom("max_active_ms"), Raw::Integer(1_000)),
            (Raw::Atom("states"), Raw::Map(&[])),
            (Raw::Atom("type"), Raw::Binary(ty)),
        ];
        let updated = [
            (Raw::Atom("max_active_ms"), Raw::Integer(3_000)),
            (Raw::Atom("states"), Raw::Map(&[])),
            (Raw::Atom("type"), Raw::Binary(ty)),
        ];
        let beta = b"beta-cross-policy-wire";
        let mut beta_input = vec![131, 104, 2];
        beta_input.extend_from_slice(&atom(b"flow_policy_v1")?);
        beta_input.extend_from_slice(&policy(beta, &[("max_active_ms", number(2_000)?)])?);
        let mut charge = PolicySchemaCharge::new(limits);
        let (first, _) = take(allocator(
            gate,
            (ty, None),
            (Raw::Nil, Raw::Map(&initial)),
            (1_000, b"q"),
            limits,
            &mut charge,
        )?)?;
        let first = first.0.ok_or("initial normalized policy")?;
        assert_eq!(first.generation(), 1);
        assert_eq!(first.encoded(), first_bytes);
        let bad = allocator(
            gate,
            (beta, None),
            (Raw::Nil, Raw::Binary(&beta_input)),
            (2_000, b"b"),
            limits,
            &mut charge,
        )?;
        assert_eq!(retained(&bad)?.response(), b"ERR invalid flow policy patch");
        assert_eq!(
            retained(&bad)?.status(),
            RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::PatchShape)
        );
        assert!(matches!(bad.decision(), DispatchDecision::Fresh((None, _))));
        let (second, _) = take(allocator(
            gate,
            (ty, Some(first.encoded())),
            (Raw::Nil, Raw::Map(&updated)),
            (3_000, b"u"),
            limits,
            &mut charge,
        )?)?;
        let second = second.0.ok_or("updated normalized policy")?;
        assert_eq!(second.generation(), 2);
        assert_eq!(second.encoded(), second_bytes);
        assert_eq!(first.encoded(), first_bytes);
        Ok(())
    })
}

#[test]
fn original_flow_source_target_highwater_value() -> TestResult {
    with_gate(|gate, limits| {
        let ty = b"target-high-water-policy";
        let stored = expected(3, ty, &[("max_active_ms", number(1_000)?)])?;
        let final_bytes = expected(4, ty, &[("max_active_ms", number(2_000)?)])?;
        let patch = [
            (Raw::Atom("max_active_ms"), Raw::Integer(2_000)),
            (Raw::Atom("states"), Raw::Map(&[])),
            (Raw::Atom("type"), Raw::Binary(ty)),
        ];
        let mut charge = PolicySchemaCharge::new(limits);
        let (fresh, _) = take(allocator(
            gate,
            (ty, Some(&stored)),
            (Raw::Nil, Raw::Map(&patch)),
            (2_000, b"q"),
            limits,
            &mut charge,
        )?)?;
        let installed = fresh.0.ok_or("allocated high-water policy")?;
        assert_eq!(installed.generation(), 4);
        assert_eq!(installed.encoded(), final_bytes);
        assert_eq!(
            stored,
            expected(3, ty, &[("max_active_ms", number(1_000)?)])?
        );
        Ok(())
    })
}

#[test]
fn original_flow_source_captured_history_limit_value() -> TestResult {
    with_gate(|gate, limits| {
        let accepted_type = b"replicated-policy-limit-accepted";
        let rejected_type = b"replicated-policy-limit-rejected";
        let history = [(Raw::Atom("history_max_events"), Raw::Integer(50))];
        let patch = [(Raw::Atom("retention"), Raw::Map(&history))];
        let expected_bytes = expected(
            1,
            accepted_type,
            &[(
                "retention",
                map(&[(atom(b"history_max_events")?, number(50)?)])?,
            )],
        )?;
        let mut charge = PolicySchemaCharge::new(limits);
        let (accepted, _) = take(allocator_limit(
            gate,
            (accepted_type, None),
            (Raw::Nil, Raw::Map(&patch)),
            (1_000, b"q"),
            (limits, 100),
            &mut charge,
        )?)?;
        let accepted = accepted.0.ok_or("permissive captured history limit")?;
        assert_eq!(accepted.generation(), 1);
        assert_eq!(accepted.encoded(), expected_bytes);
        let rejected = allocator_limit(
            gate,
            (rejected_type, None),
            (Raw::Nil, Raw::Map(&patch)),
            (2_000, b"r"),
            (limits, 10),
            &mut charge,
        )?;
        assert_eq!(
            retained(&rejected)?.status(),
            RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::Input(Reason::Retention(
                RetentionReason::History { maximum: 10 }
            )))
        );
        assert_eq!(
            retained(&rejected)?.response(),
            b"ERR flow retention history_max_events must be between 1 and 10"
        );
        assert!(matches!(
            rejected.decision(),
            DispatchDecision::Fresh((None, _))
        ));
        assert_eq!(accepted.encoded(), expected_bytes);
        Ok(())
    })
}

fn with_gate(test: impl FnOnce(GateView<'_>, PolicySchemaLimits) -> TestResult) -> TestResult {
    let limits = super::super::profile(2_000_000_000)?;
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    test(
        GateView {
            scope: &scope,
            control: ControlState::initial(stamp(0)?),
            outcomes: &outcomes,
        },
        limits,
    )
}
