//! Independently fixed event refusals and ordered Result authorities.

use super::cases::{assert_generation, input_frame};
use super::literals::{assert_accepted, assert_control, assert_outcome, value_bytes};
use super::{
    AdmissionLimits, ApplyError, BTreeMap, CompletePolicyEncodingError,
    CompletePolicyReplaceApplyError, CompletePolicyRequestError, ControlState, DispatchDecision,
    PolicyError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, TestResult, apply,
    current, literal_frame, log, minimal_data, present_data, profile, schema, scope, stamp, view,
};
use wf_domain::policy::complete::restore_complete_policy;
use wf_domain::policy::installation::MAX_POLICY_GENERATION;
use wf_domain::workflow::request::RequestValueError;

#[test]
fn complete_replace_parent_reads_context_active_and_done_before_generation() -> TestResult {
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    let control = ControlState::initial(stamp(0)?);
    let policies = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let frame = literal_frame(false, None, b"q", 0)?;
    for case in 0..9 {
        let active = BTreeMap::from([(
            b"r".to_vec(),
            match case {
                1 => Err(ApplyError::PositionConflict),
                2 | 3 => Ok(MAX_POLICY_GENERATION),
                4 => Ok(MAX_POLICY_GENERATION + 1),
                6 => Ok(MAX_POLICY_GENERATION - 1),
                _ => Ok(2),
            },
        )]);
        let done = BTreeMap::from([(
            b"r".to_vec(),
            match case {
                0 | 1 | 2 | 4 => Err(ApplyError::AllocationFailed),
                5 => Ok(MAX_POLICY_GENERATION + 1),
                3 | 6 => Ok(0),
                _ => Ok(5),
            },
        )]);
        let mut supplied = view(&scope, control, &outcomes, &policies, &active, &done);
        if case == 0 {
            supplied.history_max_events_limit = Err(ApplyError::AllocationFailed);
        }
        if case == 7 {
            supplied.history_max_events_limit = Ok(0);
        }
        if case == 8 {
            supplied.history_max_events_limit = Ok(1_000_001);
        }
        let before = charge.g();
        let result = apply(supplied, &frame, 0, limits, &mut charge);
        let refusal = match case {
            0 => Some((ApplyError::AllocationFailed, 6_532)),
            1 => Some((ApplyError::PositionConflict, 7_431)),
            2 => Some((ApplyError::AllocationFailed, 7_560)),
            4 => Some((ApplyError::InvalidAuthority, 7_431)),
            5 => Some((ApplyError::InvalidAuthority, 7_560)),
            7 | 8 => Some((ApplyError::InvalidAuthority, 6_532)),
            _ => None,
        };
        if let Some((error, fee)) = refusal {
            assert_eq!(
                result.err(),
                Some(CompletePolicyReplaceApplyError::Apply(error))
            );
            assert_eq!((charge.g(), charge.p()), (before + fee, 0));
        } else {
            let plan = result?;
            assert_control(&plan, supplied, &frame, (0, 0), false)?;
            if case == 3 {
                assert_generation(&plan, PolicyError::GenerationExhausted, &frame)?;
            } else {
                let expected = value_bytes(MAX_POLICY_GENERATION, false)?;
                assert_accepted(&plan, &minimal_data(), &expected, 0, &frame, (0, 0))?;
            }
        }
        assert!(policies.is_empty() && outcomes.is_empty());
        assert_eq!(active.len(), 1);
        assert_eq!(done.len(), 1);
    }
    assert_reader_and_window_priority(limits, &mut charge)?;
    Ok(())
}

fn assert_reader_and_window_priority(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let scope = scope()?;
    let control = ControlState::initial(stamp(0)?);
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let policies = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    let frame = literal_frame(false, Some(0), b"q", 0)?;
    let mut supplied = view(&scope, control, &outcomes, &policies, &marks, &marks);
    supplied.history_max_events_limit = Err(ApplyError::AllocationFailed);
    supplied.outcome_retention_ms = Err(ApplyError::InvalidAuthority);
    let before = charge.g();
    assert_eq!(
        apply(supplied, &frame, 0, limits, charge).err(),
        Some(CompletePolicyReplaceApplyError::Apply(
            ApplyError::PositionConflict
        ))
    );
    assert_eq!(charge.g(), before + 6_564); // 6276 + the original guard's 288.
    let empty = BTreeMap::new();
    let frame = literal_frame(false, None, b"q", 0)?;
    for window in [Err(ApplyError::AllocationFailed), Ok(0), Ok(u64::MAX)] {
        let mut supplied = view(&scope, control, &outcomes, &empty, &marks, &marks);
        supplied.outcome_retention_ms = window;
        let result = apply(supplied, &frame, 0, limits, charge);
        if window == Ok(u64::MAX) {
            // A zero original time plus MAX is a valid positive deadline.
            let plan = result?;
            let DispatchDecision::Fresh((Some(_), outcome)) = plan.decision() else {
                return Err("MAX window from zero is valid".into());
            };
            assert_eq!(outcome.expires_at().get(), u64::MAX);
        } else {
            let expected = if window == Ok(0) {
                CompletePolicyReplaceApplyError::Outcome(RequestValueError::InvalidExpiry)
            } else {
                CompletePolicyReplaceApplyError::Apply(ApplyError::AllocationFailed)
            };
            assert_eq!(result.err(), Some(expected));
        }
    }
    let mut supplied = view(&scope, control, &outcomes, &empty, &marks, &marks);
    supplied.outcome_retention_ms = Ok(u64::MAX);
    let frame = literal_frame(false, None, b"q", 1)?;
    assert_eq!(
        apply(supplied, &frame, 0, limits, charge).err(),
        Some(CompletePolicyReplaceApplyError::Outcome(
            RequestValueError::InvalidExpiry
        ))
    );
    Ok(())
}

#[test]
fn complete_replace_parent_keeps_one_account_through_all_refusals_and_late_caps() -> TestResult {
    assert_each_minimal_event(false)?;
    assert_each_minimal_event(true)?;
    for (maximum, attempts, held_peak, expected_g, expected_p, target) in [
        (9_640, 1, false, 9_640, 0, 9_640),
        (9_639, 1, false, 9_512, 0, 9_640),
        (19_280, 2, false, 19_280, 0, 19_280),
        (19_279, 2, false, 19_152, 0, 19_280),
        (11_632, 1, true, 11_568, 64, 11_632),
        (11_631, 1, true, 11_440, 64, 11_632),
    ] {
        assert_continuous_account(
            maximum,
            attempts,
            held_peak,
            (expected_g, expected_p, target),
        )?;
    }
    assert_rejected_body_events()?;
    assert_late_caps()?;
    Ok(())
}

fn minimal_events(present_current: bool) -> Vec<u64> {
    // Reader statement order: fixed 83-byte header, original 52-byte intent.
    let mut events = vec![
        128, 4, 2, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 128, 4, 1, 2, 32, 8, 2, 8, 128, 4, 128, 30,
        128, 4, 1, 1, 128, 128, 4, 1, 1, 1, 1, 1, 4, 128, 1, 1, 1, 130, 180, 192, 128, 128, 129,
        129, 129, 129, 128, 533, 149, 533, 263, 128, 134, 128, 129, 128, 30, 128, 4, 1, 1, 128,
        128, 128, 4, 1, 129, 1, 1, 1, 1, 4, 128, 128, 1, 1, 1, 128, 129,
    ];
    assert_eq!(events.iter().sum::<u64>(), 6_276);
    let value = [
        128, 4, 2, 8, 4, 128, 4, 2, 128, 4, 1, 1, 1, 1, 1, 4, 128, 1, 1, 1,
    ];
    if present_current {
        events.extend_from_slice(&value);
        events.extend(value.map(|fee| if fee == 128 { fee } else { 2 * fee }));
    }
    events.push(128); // Optional CAS, including None.
    if present_current {
        events.push(130);
    } // Both type lengths.
    events.extend_from_slice(&[128, 128, 128, 128, 1, 257, 128, 129, 129, 128]);
    events.extend_from_slice(&value);
    events.push(168);
    events.extend_from_slice(&value);
    events.extend_from_slice(&[128, 128, 128, 168, 128]);
    assert_eq!(
        events.iter().sum::<u64>(),
        if present_current { 10_914 } else { 9_640 }
    );
    events
}

fn assert_each_minimal_event(present_current: bool) -> TestResult {
    let setup_limits = profile(200_000_000)?;
    let mut setup = PolicySchemaCharge::new(setup_limits);
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let policies = if present_current {
        BTreeMap::from([(
            b"r".to_vec(),
            Ok(current(1, minimal_data(), setup_limits, &mut setup)?),
        )])
    } else {
        BTreeMap::new()
    };
    let frame = literal_frame(false, None, b"q", 0)?;
    let mut prior = 0;
    for fee in minimal_events(present_current) {
        let target = prior + fee;
        let maximum = target - 1;
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let supplied = view(
            &scope,
            ControlState::initial(stamp(0)?),
            &outcomes,
            &policies,
            &marks,
            &marks,
        );
        assert_eq!(
            apply(supplied, &frame, 0, limits, &mut charge).err(),
            Some(schema(PolicySchemaError::WorkBytes {
                bytes: target,
                maximum
            }))
        );
        assert_eq!((charge.g(), charge.p(), charge.total()), (prior, 0, prior));
        assert_eq!(outcomes.len(), 0);
        if present_current {
            assert_eq!(
                policies[b"r".as_slice()]
                    .as_ref()
                    .map_err(|_| "current")?
                    .encoded(),
                value_bytes(1, false)?
            );
        }
        prior = target;
    }
    Ok(())
}

fn assert_continuous_account(
    maximum: u64,
    attempts: u64,
    held_peak: bool,
    expected: (u64, u64, u64),
) -> TestResult {
    let limits = profile(maximum)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let seed = if held_peak {
        let mut data = minimal_data();
        data.indexed_attributes = Some(vec![b"x".to_vec()]);
        let owner = restore_complete_policy(1, data, limits, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (1_928, 64));
        Some(owner)
    } else {
        None
    };
    let scope = scope()?;
    let policies = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let frame = literal_frame(false, None, b"q", 0)?;
    let bytes = value_bytes(1, false)?;
    let supplied = view(
        &scope,
        ControlState::initial(stamp(0)?),
        &outcomes,
        &policies,
        &marks,
        &marks,
    );
    let mut held = None;
    for attempt in 0..attempts {
        let result = apply(supplied, &frame, 0, limits, &mut charge);
        if attempt + 1 == attempts && maximum < expected.2 {
            assert_eq!(
                result.err(),
                Some(schema(PolicySchemaError::WorkBytes {
                    bytes: expected.2,
                    maximum,
                }))
            );
        } else {
            let plan = result?;
            assert_accepted(&plan, &minimal_data(), &bytes, 0, &frame, (0, 0))?;
            assert_control(&plan, supplied, &frame, (0, 0), false)?;
            held = Some(plan);
        }
    }
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (expected.0, expected.1, expected.0 + expected.1)
    );
    if let Some(owner) = seed {
        assert_eq!(owner.data().indexed_attributes, Some(vec![b"x".to_vec()]));
    }
    if let Some(plan) = held {
        assert_accepted(&plan, &minimal_data(), &bytes, 0, &frame, (0, 0))?;
    }
    assert!(outcomes.is_empty() && policies.is_empty());
    Ok(())
}

fn assert_rejected_body_events() -> TestResult {
    let mut data = minimal_data();
    data.retention = Some(wf_domain::retention::RetentionOverride {
        ttl_ms: None,
        history_max_events: Some(18),
    });
    let frame = input_frame(data, None)?;
    assert_eq!(frame.len(), 141);
    let scope = scope()?;
    let policies = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let expected = b"ERR flow retention history_max_events must be between 1 and 17";
    // 4825 verification +519 gate +1595 reconstruction, then root/current/CAS/
    // context513 +CONTENT898 +window/R/metadata384. Encoder128,2,190; owner128.
    for (target, prior) in [
        (8_478, 8_350),
        (8_606, 8_478),
        (8_734, 8_606),
        (8_862, 8_734),
        (8_864, 8_862),
        (9_054, 8_864),
        (9_182, 9_054),
    ] {
        let maximum = target - 1;
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let mut supplied = view(
            &scope,
            ControlState::initial(stamp(0)?),
            &outcomes,
            &policies,
            &marks,
            &marks,
        );
        supplied.history_max_events_limit = Ok(17);
        assert_eq!(
            apply(supplied, &frame, 0, limits, &mut charge).err(),
            Some(schema(PolicySchemaError::WorkBytes {
                bytes: target,
                maximum
            }))
        );
        assert_eq!((charge.g(), charge.p()), (prior, 0));
    }
    let limits = profile(9_182)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let mut supplied = view(
        &scope,
        ControlState::initial(stamp(0)?),
        &outcomes,
        &policies,
        &marks,
        &marks,
    );
    supplied.history_max_events_limit = Ok(17);
    let plan = apply(supplied, &frame, 0, limits, &mut charge)?;
    let DispatchDecision::Fresh((None, outcome)) = plan.decision() else {
        return Err("exact rejected-parent fee must retain the original body".into());
    };
    assert_outcome(outcome, &frame, expected, (0, 0))?;
    assert_eq!((charge.g(), charge.p()), (9_182, 0));
    Ok(())
}

fn assert_late_caps() -> TestResult {
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let policies = BTreeMap::from([(
        b"r".to_vec(),
        Ok(current(1, present_data(), limits, &mut charge)?),
    )]);
    let frame = literal_frame(true, None, b"q", 0)?;
    let expected = value_bytes(2, true)?;
    assert_eq!(
        (frame.len(), expected.len(), 4 + 2 * expected.len()),
        (477, 382, 768)
    );
    let supplied = view(
        &scope,
        ControlState::initial(stamp(0)?),
        &outcomes,
        &policies,
        &marks,
        &marks,
    );
    for cap in [767, 768] {
        let result = wf_ports::log::decide_complete_policy_replace_frame(
            supplied,
            &frame,
            log(0),
            limits,
            AdmissionLimits::new(1_048_576, cap, 1_000)?,
            &mut charge,
        );
        if cap == 767 {
            assert_eq!(
                result.err(),
                Some(CompletePolicyReplaceApplyError::Request(
                    CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
                ))
            );
        } else {
            let plan = result?;
            assert_accepted(&plan, &present_data(), &expected, 1, &frame, (0, 0))?;
            assert_control(&plan, supplied, &frame, (0, 0), false)?;
        }
        assert!(outcomes.is_empty());
        assert_eq!(
            policies[b"r".as_slice()]
                .as_ref()
                .map_err(|_| "stored")?
                .encoded(),
            value_bytes(1, true)?
        );
    }
    let before = (charge.g(), charge.p());
    assert_eq!(
        apply(supplied, &frame, 0, profile(200_000_001)?, &mut charge).err(),
        Some(schema(PolicySchemaError::ProfileMismatch))
    );
    assert_eq!((charge.g(), charge.p()), before);
    assert_output_and_raw_refusals(limits, &mut charge)?;
    Ok(())
}

pub(super) fn assert_current_caps(
    setup_limits: PolicySchemaLimits,
    setup: &mut PolicySchemaCharge,
) -> TestResult {
    let mut data = minimal_data();
    for name in [b"s", b"t"] {
        data.states.insert(name.to_vec(), super::cases::state());
    }
    let policies = BTreeMap::from([(b"r".to_vec(), Ok(current(1, data, setup_limits, setup)?))]);
    let limits = PolicySchemaLimits::new(
        wf_domain::policy::installation::PolicyLimits::new(1)?,
        200_000_000,
    )?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let supplied = view(
        &scope,
        ControlState::initial(stamp(0)?),
        &outcomes,
        &policies,
        &marks,
        &marks,
    );
    for guard in [0, MAX_POLICY_GENERATION + 1] {
        let frame = literal_frame(false, Some(guard), b"q", 0)?;
        assert_eq!(
            apply(supplied, &frame, 0, limits, &mut charge).err(),
            Some(CompletePolicyReplaceApplyError::Request(
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ItemLimit)
            ))
        );
    }
    Ok(())
}

fn assert_output_and_raw_refusals(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let policies = BTreeMap::new();
    let marks = BTreeMap::new();
    let mut supplied = view(
        &scope,
        ControlState::initial(stamp(0)?),
        &outcomes,
        &policies,
        &marks,
        &marks,
    );
    supplied.outcome_retention_ms = Err(ApplyError::AllocationFailed);
    let mut data = minimal_data();
    data.workflow_type = vec![b'r'; 262_144];
    let large = input_frame(data, None)?;
    assert_eq!(
        apply(supplied, &large, 0, limits, charge).err(),
        Some(schema(PolicySchemaError::SnapshotBytes))
    );
    for (start, value, error) in [
        (79, 1_048_577_u32, CompletePolicyEncodingError::ByteLimit),
        (
            128,
            u32::try_from(limits.policy_limits().max_states() + 1)?,
            CompletePolicyEncodingError::ItemLimit,
        ),
    ] {
        let mut raw = literal_frame(false, None, b"q", 0)?;
        raw[start..start + 4].copy_from_slice(&value.to_be_bytes());
        assert_eq!(
            apply(supplied, &raw, 0, limits, charge).err(),
            Some(CompletePolicyReplaceApplyError::Request(
                CompletePolicyRequestError::Encoding(error)
            ))
        );
    }
    Ok(())
}
