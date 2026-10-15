//! Original raw frame -> one gate -> source-ordered Fresh -> original ETF owners.

use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as EncodingError, CompletePolicyReplaceApplyError as ExistingError,
    CompletePolicyRequestError as RequestError, FlowPolicyInputRejection as Reason,
    GovernancePolicyInputRejection as Gov, OriginalCompletePolicyReplaceApplyError as RawError,
    OriginalCompletePolicyReplaceApplyView as RawView, OriginalEtfSafety,
    OriginalFlowPolicyReplaceFresh as FlowFresh, OriginalFlowPolicyReplaceInput as Input,
    OriginalFlowPolicyReplaceRejection as Rejection, OriginalPolicyReadError as ReadError,
    OriginalPolicyReadValue as ReadValue, PolicyRetentionInputRejection as RetentionReason,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, RawFlowPolicyValue as Raw,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::{
    DispatchDecision, RequestScope, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::{
    OriginalFlowPolicyReplaceRequestPlan as Plan, decide_original_flow_policy_replace_frame,
};

use super::{TestResult, profile};

mod cases;
mod refusals;
mod source;

use cases::{
    KEY, accepted, atom, binary, control, current, expected, frame, frame_with, k1_safety, log,
    map, outcome, rejected, request, retained, safety, schema, scope, stamp, take, view,
};

fn apply<'v>(
    supplied: &RawView<'v, '_, '_>,
    bytes: &[u8],
    index: u64,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Plan<'v>, RawError> {
    decide_original_flow_policy_replace_frame(
        *supplied,
        bytes,
        log(index),
        limits,
        admission,
        charge,
    )
}

#[test]
fn original_flow_parent_emits_exact_original_envelopes() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let marks = BTreeMap::new();
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let bytes = frame(Raw::Nil, Raw::Map(&[]))?;
    let final_bytes = expected(1, b"r", &[])?;
    assert_eq!(
        final_bytes,
        refusals::hex(
            "836803770e666c6f775f706f6c6963795f763161017400000002770673746174657374000000007704747970656d0000000172"
        )?
    );
    let mut charge = PolicySchemaCharge::new(limits);
    let bad_safety = OriginalEtfSafety::new(&["duplicate", "duplicate"], &[], None, &[]);
    let plan = apply(
        &view(gate, &empty, &marks, &marks, bad_safety),
        &bytes,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    accepted(&plan, 0, 1, KEY, &final_bytes)?;
    outcome(retained(&plan)?, &bytes, &final_bytes);
    control(&plan, gate.control, &bytes, 0, 0, false)?;
    assert_eq!((charge.g(), charge.p()), (17_002, 4_096));
    source::assert_source_sequence(gate, limits)?;
    source::assert_replace_reset(gate, limits)?;
    source::assert_wide_versions(gate, limits)?;
    source::assert_own_output(gate, limits)?;
    source::assert_allocator_recipes(gate, limits)?;
    Ok(())
}

#[test]
fn original_flow_parent_keeps_allocator_guard_and_current_priority() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let poison = BTreeMap::from([(b"r".to_vec(), ReadValue::Other)]);
    //470ba's beta-cross-policy-wire is the actual generationless ETF BINARY patch.
    let beta = map(&[
        (atom(b"states")?, map(&[])?),
        (atom(b"type")?, binary(b"beta-cross-policy-wire")?),
    ])?;
    let mut binary_patch = vec![131, 104, 2];
    binary_patch.extend_from_slice(&atom(b"flow_policy_v1")?);
    binary_patch.extend_from_slice(&beta);
    for patch in [Raw::Nil, Raw::List(&[]), Raw::Binary(&binary_patch)] {
        let bytes = frame_with(b"invalid-key", Raw::Integer(-1), patch, b"q", 0)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &view(gate, &poison, &marks, &marks, safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        rejected(
            &plan,
            RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::PatchShape),
            b"ERR invalid flow policy patch",
            &bytes,
        )?;
    }
    for key in [b"".as_slice(), b"f:{f}:policy:", b"f:{flow}:policy:r"] {
        let bytes = frame_with(key, Raw::Integer(-1), Raw::Map(&[]), b"q", 0)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &view(gate, &poison, &marks, &marks, safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        rejected(
            &plan,
            RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::PolicyKey),
            b"ERR invalid flow policy key",
            &bytes,
        )?;
    }
    cases::assert_current_and_guard(gate, limits)?;
    Ok(())
}

#[test]
fn original_flow_parent_preserves_nonfresh_before_all_changed_reads() -> TestResult {
    for kind in 0..3 {
        let limits = profile(2_000_000_000)?;
        let (bundle, base, original) = origin(kind, limits)?;
        let scope = scope()?;
        let request_id = wf_types::identity::RequestId::new(b"q".to_vec())?;
        let old_body = bundle.1.response().to_vec();
        let old_status = bundle.1.status();
        let outcomes = BTreeMap::from([(request_id.clone(), bundle.1)]);
        let poison = BTreeMap::from([(b"r".to_vec(), ReadValue::Other)]);
        let failed = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
        let mut supplied = view(
            GateView {
                scope: &scope,
                control: base,
                outcomes: &outcomes,
            },
            &poison,
            &failed,
            &failed,
            OriginalEtfSafety::new(&["x", "x"], &[], None, &[]),
        );
        supplied.history_max_events_limit = Err(ApplyError::AllocationFailed);
        supplied.outcome_retention_ms = Err(ApplyError::InvalidAuthority);
        for (case, time, index) in [(0, 0_u64, 0), (1, 99, 1), (2, 100, 1)] {
            let mut bytes = original.clone();
            bytes[61..69].copy_from_slice(&time.to_be_bytes());
            bytes[71..79].copy_from_slice(&time.to_be_bytes());
            let mut charge = PolicySchemaCharge::new(limits);
            let plan = apply(
                &supplied,
                &bytes,
                index,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )?;
            control(&plan, base, &bytes, time, index, case == 0)?;
            match case {
                0 => assert!(matches!(plan.decision(), DispatchDecision::LogReplay)),
                1 => {
                    let DispatchDecision::Replay(out) = plan.decision() else {
                        return Err("Replay".into());
                    };
                    let replayed: &RetainedOutcome = out;
                    assert!(std::ptr::eq(replayed, &raw const outcomes[&request_id]));
                    assert_eq!(out.response(), old_body);
                    assert_eq!(out.status(), old_status);
                }
                _ => assert!(matches!(plan.decision(), DispatchDecision::Expired)),
            }
            if kind == 0 {
                assert_eq!((charge.g(), charge.p()), ([5_073, 5_918, 5_918][case], 0));
            }
            outcome(&outcomes[&request_id], &original, &old_body);
        }
        let conflict = frame_with(KEY, Raw::Integer(1), Raw::Map(&[]), b"q", 100)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &supplied,
            &conflict,
            1,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert!(matches!(plan.decision(), DispatchDecision::IntentConflict));
        if kind == 0 {
            assert_eq!((charge.g(), charge.p()), (5_808, 0));
        }
        if let Some(installed) = bundle.0 {
            assert_eq!(installed.encoded(), old_body);
        }
    }
    Ok(())
}

fn origin(
    kind: usize,
    limits: PolicySchemaLimits,
) -> TestResult<(FlowFresh, ControlState, Vec<u8>)> {
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let marks = BTreeMap::new();
    let fields = [(Raw::Atom("max_active_ms"), Raw::Integer(0))];
    let retention = [(Raw::Atom("history_max_events"), Raw::Integer(18))];
    let contextual = [(Raw::Atom("retention"), Raw::Map(&retention))];
    let patch = match kind {
        0 => Raw::Map(&[]),
        1 => Raw::Map(&fields),
        _ => Raw::Map(&contextual),
    };
    let bytes = frame(Raw::Nil, patch)?;
    let mut supplied = view(
        GateView {
            scope: &scope,
            control: ControlState::initial(stamp(0)?),
            outcomes: &outcomes,
        },
        &empty,
        &marks,
        &marks,
        safety(),
    );
    supplied.history_max_events_limit = Ok(17);
    let mut charge = PolicySchemaCharge::new(limits);
    let plan = apply(
        &supplied,
        &bytes,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    let body = match kind {
        0 => expected(1, b"r", &[])?,
        1 => b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity".to_vec(),
        _ => b"ERR flow retention history_max_events must be between 1 and 17".to_vec(),
    };
    let out = retained(&plan)?;
    outcome(out, &bytes, &body);
    if kind == 2 {
        assert_eq!(
            out.status(),
            RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::Input(Reason::Retention(
                RetentionReason::History { maximum: 17 }
            )))
        );
    }
    let (bundle, control) = take(plan)?;
    Ok((bundle, control, bytes))
}

#[test]
fn original_flow_parent_retains_only_reached_business_rejections() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let failed = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let mode = [(Raw::Atom("mode"), Raw::Nil)];
    let states = [(Raw::Atom("states"), Raw::Nil)];
    for (patch, reason, body) in [
        (
            Raw::Map(&mode),
            Reason::TopStateMode,
            b"ERR flow state mode is state-level only".as_slice(),
        ),
        (
            Raw::Map(&states),
            Reason::StatesShape,
            b"ERR flow policy states must be a map or keyword list",
        ),
    ] {
        let bytes = frame(Raw::Nil, patch)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &view(gate, &empty, &failed, &failed, safety()),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        rejected(
            &plan,
            RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::Input(reason)),
            body,
            &bytes,
        )?;
        let mut supplied = view(gate, &empty, &failed, &failed, safety());
        supplied.outcome_retention_ms = Err(ApplyError::AllocationFailed);
        assert_eq!(
            apply(
                &supplied,
                &bytes,
                0,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .err(),
            Some(RawError::Replace(ExistingError::Apply(
                ApplyError::AllocationFailed
            )))
        );
    }
    refusals::assert_large_source_errors(gate, limits)?;
    Ok(())
}

#[test]
fn original_flow_parent_keeps_context_content_and_highwater_order() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let active = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    let done = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::AllocationFailed))]);
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let bytes = frame(Raw::Nil, Raw::Map(&[]))?;
    let mut supplied = view(gate, &empty, &active, &done, safety());
    let mut charge = PolicySchemaCharge::new(limits);
    supplied.history_max_events_limit = Err(ApplyError::InvalidAuthority);
    assert_eq!(
        apply(
            &supplied,
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(RawError::Replace(ExistingError::Apply(
            ApplyError::InvalidAuthority
        )))
    );
    for c in [0, 1_000_001] {
        supplied.history_max_events_limit = Ok(c);
        assert_eq!(
            apply(
                &supplied,
                &bytes,
                0,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .err(),
            Some(RawError::Replace(ExistingError::Apply(
                ApplyError::InvalidAuthority
            )))
        );
    }
    supplied.history_max_events_limit = Ok(17);
    assert_eq!(
        apply(
            &supplied,
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(RawError::Replace(ExistingError::Apply(
            ApplyError::PositionConflict
        )))
    );
    let no_marks = BTreeMap::new();
    supplied.active_migrations = &no_marks;
    assert_eq!(
        apply(
            &supplied,
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(RawError::Replace(ExistingError::Apply(
            ApplyError::AllocationFailed
        )))
    );
    cases::assert_highwater(gate, limits)?;
    Ok(())
}

#[test]
fn original_flow_parent_owns_key_value_and_response_after_frame_drop() -> TestResult {
    let limits = profile(2_000_000_000)?;
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let final_bytes = expected(2, b"r", &[])?;
    let plan = {
        let owned_key = KEY.to_vec();
        let raw = current(1, b"r")?;
        let atoms = ["flow_policy_v1".to_owned(), "type".to_owned()];
        let names = atoms.iter().map(String::as_str).collect::<Vec<_>>();
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
        let bytes = frame_with(&owned_key, Raw::Integer(1), Raw::Map(&[]), b"q", 0)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = apply(
            &view(
                gate,
                &policies,
                &marks,
                &marks,
                OriginalEtfSafety::new(&names, &[], None, &[]),
            ),
            &bytes,
            0,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        accepted(&plan, 1, 2, &owned_key, &final_bytes)?;
        let DispatchDecision::Fresh((Some(value), _)) = plan.decision() else {
            return Err("owned value".into());
        };
        assert_ne!(value.policy_key().as_ptr(), owned_key.as_ptr());
        plan
    };
    accepted(&plan, 1, 2, KEY, &final_bytes)?;
    assert_eq!(retained(&plan)?.scope(), &scope);
    assert_eq!(retained(&plan)?.response(), final_bytes);
    Ok(())
}

#[test]
fn original_flow_parent_preserves_every_prospective_phase() -> TestResult {
    refusals::assert_parent_events()?;
    assert_profile()?;
    refusals::assert_present_current()?;
    refusals::assert_aggregate_and_expiry()?;
    assert_rejection_events()?;
    Ok(())
}

fn assert_rejection_events() -> TestResult {
    // I68/S8/F163; independent verifier4495+gate519+root/class256=5270.
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let poison = BTreeMap::from([(b"r".to_vec(), ReadValue::Other)]);
    let marks = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let bytes = frame(Raw::Nil, Raw::Nil)?;
    assert_eq!(bytes.len(), 163);
    let mut previous = 5_270;
    for delta in [128, 128, 128, 128, 157, 128] {
        let target = previous + delta;
        let limits = profile(target - 1)?;
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            apply(
                &view(gate, &poison, &marks, &marks, safety()),
                &bytes,
                0,
                limits,
                AdmissionLimits::default(),
                &mut charge
            )
            .err(),
            Some(schema(PolicySchemaError::WorkBytes {
                bytes: target,
                maximum: target - 1
            }))
        );
        assert_eq!((charge.g(), charge.p()), (previous, 0));
        previous = target;
    }
    assert_eq!(previous, 6_067);
    let limits = profile(6_067)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let plan = apply(
        &view(gate, &poison, &marks, &marks, safety()),
        &bytes,
        0,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    rejected(
        &plan,
        RetainedStatus::OriginalFlowPolicyReplaceRejected(Rejection::PatchShape),
        b"ERR invalid flow policy patch",
        &bytes,
    )?;
    assert_eq!((charge.g(), charge.p()), (6_067, 0));
    Ok(())
}

#[test]
fn original_flow_parent_keeps_continuous_held_owners() -> TestResult {
    refusals::assert_continuous()?;
    Ok(())
}

fn assert_profile() -> TestResult {
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let empty = BTreeMap::new();
    let marks = BTreeMap::new();
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    assert_eq!(
        apply(
            &view(gate, &empty, &marks, &marks, safety()),
            &frame(Raw::Nil, Raw::Map(&[]))?,
            0,
            profile(100_000_001)?,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(schema(PolicySchemaError::ProfileMismatch))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    Ok(())
}
