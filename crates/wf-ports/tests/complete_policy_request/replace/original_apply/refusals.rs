//! Independently frozen debit boundaries and unchanged neighboring contracts.

use super::{
    AdmissionLimits, ApplyError, BTreeMap, ControlState, EncodingError, ExistingError,
    OriginalEtfSafety, PolicyError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    RawError, ReadError, ReadValue, RequestError, RequestId, TestResult, accepted, apply, atom,
    binary, corrupt, current, envelope, frame, k1_safety, log, map, minimal_data, origin, outcome,
    profile, schema, scope, stamp, value_bytes, view,
};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchApplyError, CompleteCorePolicyPatchApplyView,
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput,
    CompletePolicyReplaceApplyView, StoredPolicyValue, prepare_complete_core_policy_patch_fresh,
    prepare_complete_policy_replace, prepare_complete_policy_value_v2,
    read_original_complete_policy_entry, restore_complete_policy,
};
use wf_domain::policy::definition::PolicyPatch;
use wf_domain::workflow::request::ClientIntent;
use wf_domain::workflow::request::gate::GateView;

#[test]
fn original_current_replace_keeps_one_account_and_owned_results_through_refusals() -> TestResult {
    assert_reader_events()?;
    assert_parent_phases()?;
    for (maximum, attempts, seed, retained) in [
        (16_996, 1, false, 16_548),
        (16_995, 1, false, 16_420),
        (33_544, 2, false, 33_096),
        (33_543, 2, false, 32_968),
        (18_924, 1, true, 18_476),
        (18_923, 1, true, 18_348),
    ] {
        assert_continuous(maximum, attempts, seed, retained)?;
    }
    assert_runtime_prefix()?;
    assert_held_rejection()?;
    Ok(())
}

fn reader_events() -> Vec<(u64, u64)> {
    // Original lexical fields/blocks, independently repeated in the two passes.
    let lexical = [
        1, 128, 1, 1, 128, 1, 1, 14, 156, 128, 1, 1, 128, 1, 4, 128, 1, 1, 4, 146, 136, 128, 1, 4,
        1,
    ];
    assert_eq!(lexical.iter().sum::<u64>(), 1_244);
    let mut events = [128, 1, 1, 128, 142, 132].map(|g| (g, 0)).to_vec();
    events.extend(lexical.map(|g| (g, 0)));
    events.extend_from_slice(&[(0, 448), (320, 0), (132, 0), (128, 0)]);
    events.extend(lexical.map(|g| (g, 0)));
    events.extend([128, 128, 128, 156, 128, 128].map(|g| (g, 0)));
    events.extend_from_slice(&[
        (128, 0),
        (128, 0),
        (128, 0),
        (4, 0),
        (137, 0),
        (138, 0),
        (144, 0),
        (128, 0),
        (0, 128),
        (128, 0),
        (132, 0),
        (137, 0),
        (128, 0),
        (132, 0),
        (138, 0),
    ]);
    events.extend([128, 132, 136, 128, 128].map(|g| (g, 0)));
    assert_eq!(events.iter().map(|event| event.0).sum::<u64>(), 6_778);
    events
}

fn current_work(target: u64, maximum: u64) -> RawError {
    RawError::Current(ReadError::Encoding(EncodingError::Schema(
        PolicySchemaError::WorkBytes {
            bytes: target,
            maximum,
        },
    )))
}

fn assert_reader_events() -> TestResult {
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let raw = current(0)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let bytes = frame(false, None, b"q", 0)?;
    let (mut prior_g, mut prior_p) = (6_276, 0);
    for (g, p) in reader_events() {
        let next_g = prior_g + g;
        let next_p = prior_p.max(p);
        let target = next_g + next_p;
        if target > prior_g + prior_p {
            let maximum = target - 1;
            let limits = profile(maximum)?;
            let mut charge = PolicySchemaCharge::new(limits);
            assert_eq!(
                apply(
                    &view(gate, &policies, &marks, k1_safety()),
                    &bytes,
                    0,
                    limits,
                    &mut charge
                )
                .err(),
                Some(current_work(target, maximum))
            );
            assert_eq!((charge.g(), charge.p()), (prior_g, prior_p));
            assert_eq!(raw, current(0)?);
            assert!(outcomes.is_empty());
        }
        (prior_g, prior_p) = (next_g, next_p);
    }
    assert_eq!((prior_g, prior_p), (13_054, 448));
    Ok(())
}

fn assert_parent_phases() -> TestResult {
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let raw = current(0)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let bytes = frame(false, None, b"q", 0)?;
    let mut phases = vec![
        (6_147, 6_019, 0, false),
        (6_276, 6_147, 0, false),
        (13_182, 13_054, 448, false),
        (13_312, 13_182, 448, true),
    ];
    let writer = [
        128, 4, 2, 8, 4, 128, 4, 2, 128, 4, 1, 1, 1, 1, 1, 4, 128, 1, 1, 1,
    ];
    assert_eq!(writer.iter().sum::<u64>(), 552);
    let mut tail = vec![128, 128, 128, 128, 1, 257, 128, 129, 129, 128];
    tail.extend_from_slice(&writer);
    tail.push(168);
    tail.extend_from_slice(&writer);
    tail.extend_from_slice(&[128, 128, 128, 168, 128]);
    let mut prior = 13_312;
    for fee in tail {
        phases.push((prior + fee, prior, 448, false));
        prior += fee;
    }
    assert_eq!(prior, 16_548);
    for (target_g, prior_g, peak, current_phase) in phases {
        let target = target_g + peak;
        let maximum = target - 1;
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let expected = if current_phase {
            current_work(target, maximum)
        } else {
            schema(PolicySchemaError::WorkBytes {
                bytes: target,
                maximum,
            })
        };
        assert_eq!(
            apply(
                &view(gate, &policies, &marks, k1_safety()),
                &bytes,
                0,
                limits,
                &mut charge
            )
            .err(),
            Some(expected)
        );
        assert_eq!((charge.g(), charge.p()), (prior_g, peak));
    }
    Ok(())
}

fn assert_continuous(maximum: u64, attempts: u64, seeded: bool, retained: u64) -> TestResult {
    let limits = profile(maximum)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let seed = if seeded {
        let mut data = minimal_data();
        data.indexed_attributes = Some(vec![b"x".to_vec()]);
        let owner = restore_complete_policy(1, data, limits, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (1_928, 64));
        Some(owner)
    } else {
        None
    };
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let raw = current(0)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let supplied = view(gate, &policies, &marks, k1_safety());
    let bytes = frame(false, None, b"q", 0)?;
    let expected = value_bytes(1, false)?;
    let target = 16_548 * attempts + u64::from(seeded) * 1_928 + 448;
    let mut held = None;
    for attempt in 0..attempts {
        let result = apply(&supplied, &bytes, 0, limits, &mut charge);
        if attempt + 1 == attempts && maximum < target {
            assert_eq!(
                result.err(),
                Some(schema(PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum
                }))
            );
        } else {
            let plan = result?;
            accepted(&plan, &minimal_data(), &expected, 0, &bytes)?;
            if held.is_none() {
                held = Some(plan);
            }
        }
    }
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (retained, 448, retained + 448)
    );
    if let Some(owner) = seed {
        assert_eq!(owner.data().indexed_attributes, Some(vec![b"x".to_vec()]));
    }
    if let Some(plan) = held {
        accepted(&plan, &minimal_data(), &expected, 0, &bytes)?;
    }
    assert_eq!(raw, current(0)?);
    assert!(outcomes.is_empty());
    Ok(())
}

fn assert_runtime_prefix() -> TestResult {
    let mut fun = vec![112, 0, 0, 0, 0, 0];
    fun.extend_from_slice(&[0; 16]);
    fun.extend_from_slice(&[0; 8]);
    fun.extend_from_slice(&atom(b"m")?);
    fun.extend_from_slice(&[97, 0, 97, 7, 88]);
    fun.extend_from_slice(&atom(b"n")?);
    fun.extend_from_slice(&[0; 12]);
    let raw = envelope(0, &map(&[(binary(b"x")?, fun)])?)?;
    assert_eq!(raw.len(), 85);
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let safety = OriginalEtfSafety::new(&["flow_policy_v1", "m", "n"], &[], Some(("n", 0)), &[]);
    let bytes = frame(false, None, b"q", 0)?;
    for (target, prior_g, peak) in [
        (10_362, 9_530, 0),
        (13_757, 12_797, 832),
        (13_885, 12_925, 832),
    ] {
        let maximum = target - 1;
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            apply(
                &view(gate, &policies, &marks, safety),
                &bytes,
                0,
                limits,
                &mut charge
            )
            .err(),
            Some(current_work(target, maximum))
        );
        assert_eq!((charge.g(), charge.p()), (prior_g, peak));
    }
    let limits = profile(100_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        apply(
            &view(gate, &policies, &marks, safety),
            &bytes,
            0,
            limits,
            &mut charge
        )
        .err(),
        Some(corrupt())
    );
    assert_eq!((charge.g(), charge.p()), (16_197, 832));
    let ordinary = current(0)?;
    let ordinary_policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&ordinary))]);
    let plan = apply(
        &view(gate, &ordinary_policies, &marks, k1_safety()),
        &bytes,
        0,
        limits,
        &mut charge,
    )?;
    accepted(&plan, &minimal_data(), &value_bytes(1, false)?, 0, &bytes)?;
    assert_eq!((charge.g(), charge.p()), (32_745, 832));
    Ok(())
}

fn assert_held_rejection() -> TestResult {
    let limits = profile(100_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let (held, _, original, body) = origin(2, limits, &mut charge)?;
    assert_eq!((charge.g(), charge.p()), (16_090, 448));
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let raw = current(0)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let bytes = frame(false, None, b"q", 0)?;
    let plan = apply(
        &view(gate, &policies, &marks, k1_safety()),
        &bytes,
        0,
        limits,
        &mut charge,
    )?;
    accepted(&plan, &minimal_data(), &value_bytes(1, false)?, 0, &bytes)?;
    assert_eq!((charge.g(), charge.p()), (32_638, 448));
    outcome(&held.1, &original, &body, 0, 0);
    assert!(held.0.is_none());
    Ok(())
}

#[test]
fn original_current_replace_keeps_existing_typed_and_standalone_contracts() -> TestResult {
    assert_typed_parents()?;
    assert_unchanged_guard_order()?;
    assert_public_read_and_reference()?;
    Ok(())
}

fn assert_typed_parents() -> TestResult {
    let limits = profile(100_000)?;
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let bytes = frame(false, None, b"q", 0)?;
    let expected = value_bytes(1, false)?;
    for present in [false, true] {
        let mut setup = PolicySchemaCharge::new(limits);
        let policies = if present {
            let candidate = restore_complete_policy(1, minimal_data(), limits, &mut setup)?;
            BTreeMap::from([(
                b"r".to_vec(),
                Ok(prepare_complete_policy_value_v2(
                    candidate, limits, &mut setup,
                )?),
            )])
        } else {
            BTreeMap::new()
        };
        let supplied = CompletePolicyReplaceApplyView {
            gate: GateView {
                scope: &scope,
                control: ControlState::initial(stamp(0)?),
                outcomes: &outcomes,
            },
            revision: 7,
            policies: &policies,
            active_migrations: &marks,
            completed_migrations: &marks,
            history_max_events_limit: Ok(1_000_000),
            outcome_retention_ms: Ok(100),
        };
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = wf_ports::log::decide_complete_policy_replace_frame(
            supplied,
            &bytes,
            log(0),
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        accepted(
            &plan,
            &minimal_data(),
            &value_bytes(1 + u64::from(present), false)?,
            u64::from(present),
            &bytes,
        )?;
        assert_eq!(
            (charge.g(), charge.p()),
            (if present { 10_914 } else { 9_640 }, 0)
        );
    }
    let raw_policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&expected))]);
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        apply(
            &view(gate, &raw_policies, &marks, k1_safety()),
            &bytes,
            0,
            limits,
            &mut charge
        )
        .err(),
        Some(corrupt())
    );
    Ok(())
}

fn assert_unchanged_guard_order() -> TestResult {
    let limits = profile(100_000)?;
    let mut setup = PolicySchemaCharge::new(limits);
    let current = restore_complete_policy(1, minimal_data(), limits, &mut setup)?
        .bind_encoded_snapshot(0..1, vec![0], limits, &mut setup)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        prepare_complete_policy_replace(
            Some(&current),
            0,
            Some(0),
            minimal_data(),
            limits,
            &mut charge
        )
        .err(),
        Some(EncodingError::Schema(PolicySchemaError::Policy(
            PolicyError::StaleGeneration
        )))
    );
    assert_eq!((charge.g(), charge.p()), (128, 0));
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let mut policies = BTreeMap::from([(
        b"r".to_vec(),
        Ok(StoredPolicyValue::Complete(Box::new(current))),
    )]);
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let supplied = CompleteCorePolicyPatchApplyView {
        gate,
        revision: 7,
        policies: &policies,
        active_migrations: &marks,
        completed_migrations: &marks,
        history_max_events_limit: Ok(1_000_000),
        outcome_retention_ms: Err(ApplyError::PositionConflict),
    };
    let input = CompleteCorePolicyPatchRequestInput::new(
        scope.clone(),
        RequestId::new(b"q".to_vec())?,
        stamp(0)?,
        CompleteCorePolicyPatchInput {
            workflow_type: b"r".to_vec(),
            expected_generation: Some(0),
            patch: PolicyPatch::default(),
        },
    );
    assert_eq!(
        prepare_complete_core_policy_patch_fresh(
            supplied,
            input,
            (
                ClientIntent::new(1, [0; 32])?,
                log(0),
                stamp(0)?.hlc().physical()
            ),
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(CompleteCorePolicyPatchApplyError::Request(
            RequestError::Encoding(EncodingError::EncodingMismatch)
        ))
    );
    let (kind, current) = policies.pop_first().ok_or("missing Complete fixture")?;
    assert!(policies.is_empty());
    let Ok(StoredPolicyValue::Complete(current)) = current else {
        return Err("expected Complete fixture".into());
    };
    let policies = BTreeMap::from([(kind, Ok(*current))]);
    let typed = CompletePolicyReplaceApplyView {
        gate,
        revision: 7,
        policies: &policies,
        active_migrations: &marks,
        completed_migrations: &marks,
        history_max_events_limit: Ok(1_000_000),
        outcome_retention_ms: Err(ApplyError::PositionConflict),
    };
    assert_eq!(
        wf_ports::log::decide_complete_policy_replace_frame(
            typed,
            &frame(false, Some(0), b"q", 0)?,
            log(0),
            limits,
            AdmissionLimits::default(),
            &mut charge
        )
        .err(),
        Some(ExistingError::Request(RequestError::Encoding(
            EncodingError::EncodingMismatch
        )))
    );
    Ok(())
}

fn assert_public_read_and_reference() -> TestResult {
    let limits = profile(100_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let bad = OriginalEtfSafety::new(&["duplicate", "duplicate"], &[], None, &[]);
    assert!(
        read_original_complete_policy_entry(ReadValue::Absent, b"r", &bad, limits, &mut charge)?
            .is_none()
    );
    assert_eq!((charge.g(), charge.p()), (128, 0));
    let reference = wf_ports::log::prepare_original_complete_policy_reference(
        ReadValue::Absent,
        b"r",
        &bad,
        limits,
        &mut charge,
    )?;
    assert_eq!(reference.generation(), 0);
    assert_eq!(reference.workflow_type(), b"r");
    assert_eq!(reference.encoded(), current(0)?);
    assert_eq!((charge.g(), charge.p()), (2_328, 0));
    assert_eq!(
        read_original_complete_policy_entry(ReadValue::Other, b"r", &bad, limits, &mut charge)
            .err(),
        Some(ReadError::Corrupt)
    );
    assert_eq!(ReadError::Corrupt.to_string(), "ERR flow policy is corrupt");
    Ok(())
}

pub(super) fn assert_current_nodes(
    gate: GateView<'_>,
    marks: &BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    frame: &[u8],
    limits: PolicySchemaLimits,
) -> TestResult {
    let rows = (0_u32..2047)
        .map(|key| Ok((binary(&key.to_be_bytes())?, vec![106])))
        .collect::<TestResult<Vec<_>>>()?;
    let raw = envelope(0, &map(&rows)?)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        apply(
            &view(
                gate,
                &policies,
                marks,
                OriginalEtfSafety::new(&["flow_policy_v1"], &[], None, &[]),
            ),
            frame,
            0,
            limits,
            &mut charge
        )
        .err(),
        Some(RawError::Current(ReadError::Encoding(
            EncodingError::ItemLimit
        )))
    );
    assert_eq!((charge.g(), charge.p()), (551_734, 0));
    Ok(())
}
