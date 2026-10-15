//! Original raw-current Replace composition; supplied views are not storage.

use super::super::{TestResult, minimal_data, present_data, profile};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    CompletePolicyData, CompletePolicyEncodingError as EncodingError,
    CompletePolicyReplaceApplyError as ExistingError, CompletePolicyReplaceFresh,
    CompletePolicyReplaceRejectionKind as Kind, CompletePolicyRequestError as RequestError,
    OriginalCompletePolicyReplaceApplyError as RawError,
    OriginalCompletePolicyReplaceApplyView as RawView, OriginalEtfSafety,
    OriginalPolicyReadError as ReadError, OriginalPolicyReadValue as ReadValue, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::{
    DispatchDecision, RequestScope, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::{
    CompletePolicyReplaceRequestPlan, decide_original_complete_policy_replace_frame,
};
use wf_types::identity::RequestId;

mod cases;
mod refusals;

use cases::{
    accepted, atom, binary, control, corrupt, current, envelope, frame, input_frame, k1_safety,
    log, map, outcome, rejected, request, safety, schema, scope, stamp, value_bytes, view,
};

fn apply<'v>(
    view: &RawView<'v, '_, '_>,
    bytes: &[u8],
    index: u64,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyReplaceRequestPlan<'v>, RawError> {
    decide_original_complete_policy_replace_frame(
        *view,
        bytes,
        log(index),
        limits,
        AdmissionLimits::default(),
        charge,
    )
}

#[test]
fn original_current_replace_accepts_zero_and_discards_stored_business_fields() -> TestResult {
    let limits = profile(200_000_000)?;
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let base = ControlState::initial(stamp(0)?);
    let gate = GateView {
        scope: &scope,
        control: base,
        outcomes: &outcomes,
    };
    let bytes = frame(false, None, b"q", 0)?;
    for generation in [0, 1] {
        let expected = value_bytes(generation + 1, false)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let plan = {
            let raw = envelope(
                generation,
                &map(&[
                    (atom(b"type")?, binary(b"r")?),
                    (atom(b"max_active_ms")?, vec![98, 255, 255, 255, 255]),
                    (atom(b"states")?, vec![97, 7]),
                ])?,
            )?;
            let untouched = raw.clone();
            let names = [
                "flow_policy_v1".to_owned(),
                "type".to_owned(),
                "max_active_ms".to_owned(),
                "states".to_owned(),
            ];
            let borrowed = names.iter().map(String::as_str).collect::<Vec<_>>();
            let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
            let marks = BTreeMap::new();
            let supplied = view(
                gate,
                &policies,
                &marks,
                OriginalEtfSafety::new(&borrowed, &[], None, &[]),
            );
            let plan = apply(&supplied, &bytes, 0, limits, &mut charge)?;
            assert_eq!(raw, untouched);
            assert_eq!(policies.len(), 1);
            plan
        };
        accepted(&plan, &minimal_data(), &expected, generation, &bytes)?;
        control(&plan, base, &bytes, 0, 0, false)?;
    }
    let raw = current(0)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let marks = BTreeMap::new();
    let mut charge = PolicySchemaCharge::new(limits);
    let bytes = frame(true, None, b"q", 0)?;
    let expected = value_bytes(1, true)?;
    let plan = apply(
        &view(gate, &policies, &marks, k1_safety()),
        &bytes,
        0,
        limits,
        &mut charge,
    )?;
    accepted(&plan, &present_data(), &expected, 0, &bytes)?;
    assert_eq!(raw, current(0)?);
    assert_source_replace(gate, limits, &mut charge)?;
    Ok(())
}

fn assert_source_replace(
    gate: GateView<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let queued = map(&[(atom(b"mode")?, atom(b"fifo")?)])?;
    let raw = envelope(
        1,
        &map(&[
            (atom(b"type")?, binary(b"r")?),
            (atom(b"max_active_ms")?, vec![98, 0, 0, 3, 232]),
            (atom(b"states")?, map(&[(binary(b"queued")?, queued)])?),
            (atom(b"unknown")?, binary(b"discarded")?),
        ])?,
    )?;
    let untouched = raw.clone();
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let marks = BTreeMap::new();
    let safety = OriginalEtfSafety::new(
        &[
            "flow_policy_v1",
            "type",
            "max_active_ms",
            "states",
            "mode",
            "fifo",
            "unknown",
        ],
        &[],
        None,
        &[],
    );
    let bytes = frame(false, None, b"q", 0)?;
    let expected = value_bytes(2, false)?;
    let plan = apply(
        &view(gate, &policies, &marks, safety),
        &bytes,
        0,
        limits,
        charge,
    )?;
    accepted(&plan, &minimal_data(), &expected, 1, &bytes)?;
    assert_eq!(raw, untouched);
    Ok(())
}

#[test]
fn original_current_replace_keeps_decode_cas_and_type_priority() -> TestResult {
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let mut trailing = current(0)?;
    trailing.push(0);
    let deprecated = envelope(
        0,
        &map(&[(
            atom(b"retry")?,
            map(&[(atom(b"max_attempts")?, vec![106])])?,
        )])?,
    )?;
    let mut negative = envelope(0, &map(&[])?)?;
    negative.splice(19..21, [98, 255, 255, 255, 255]);
    for raw in [
        vec![131, 80, 0, 0, 0, 1, 0],
        trailing,
        deprecated,
        negative,
        envelope(MAX_POLICY_GENERATION + 1, &map(&[])?)?,
        source603()?,
    ] {
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
        for expected in [Some(1), Some(MAX_POLICY_GENERATION + 1)] {
            let bytes = frame(false, expected, b"q", 0)?;
            let mut supplied = view(gate, &policies, &marks, safety());
            supplied.outcome_retention_ms = Err(ApplyError::PositionConflict);
            assert_eq!(
                apply(&supplied, &bytes, 0, limits, &mut charge).err(),
                Some(corrupt())
            );
        }
    }
    for policy in [
        map(&[])?,
        map(&[
            (atom(b"type")?, binary(b"foreign-policy-type")?),
            (atom(b"max_active_ms")?, vec![98, 0, 0, 35, 40]),
        ])?,
        map(&[(binary(b"type")?, binary(b"r")?)])?,
        map(&[(atom(b"type")?, atom(b"type")?)])?,
    ] {
        let raw = envelope(0, &policy)?;
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
        for expected in [Some(1), Some(0), None] {
            let bytes = frame(false, expected, b"q", 0)?;
            let mut supplied = view(gate, &policies, &marks, safety());
            supplied.history_max_events_limit = Err(ApplyError::AllocationFailed);
            let result = apply(&supplied, &bytes, 0, limits, &mut charge);
            if expected == Some(1) {
                let plan = result?;
                rejected(&plan, b"ERR stale flow policy generation", &bytes)?;
                let DispatchDecision::Fresh((None, out)) = plan.decision() else {
                    return Err("stale branch".into());
                };
                assert!(
                    matches!(out.status(), RetainedStatus::PolicyRejected(reason) if reason.error() == PolicyError::StaleGeneration)
                );
            } else {
                assert_eq!(result.err(), Some(corrupt()));
            }
        }
    }
    let raw = current(0)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let bytes = frame(false, Some(MAX_POLICY_GENERATION + 1), b"q", 0)?;
    let plan = apply(
        &view(gate, &policies, &marks, k1_safety()),
        &bytes,
        0,
        limits,
        &mut charge,
    )?;
    rejected(&plan, b"ERR invalid flow policy generation", &bytes)?;
    let DispatchDecision::Fresh((None, out)) = plan.decision() else {
        return Err("invalid guard".into());
    };
    assert!(
        matches!(out.status(), RetainedStatus::PolicyRejected(reason) if reason.error() == PolicyError::InvalidGeneration)
    );
    assert_current_bit_types(gate, &marks, limits, &mut charge)?;
    cases::assert_source_cas_sequence(gate, limits, &mut charge)?;
    Ok(())
}

fn assert_current_bit_types(
    gate: GateView<'_>,
    marks: &BTreeMap<Vec<u8>, Result<u64, ApplyError>>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for bits in [7, 8] {
        let raw = envelope(
            0,
            &map(&[(atom(b"type")?, vec![77, 0, 0, 0, 1, bits, b'r'])])?,
        )?;
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
        let bytes = frame(false, Some(0), b"q", 0)?;
        let result = apply(
            &view(gate, &policies, marks, k1_safety()),
            &bytes,
            0,
            limits,
            charge,
        );
        if bits == 8 {
            accepted(
                &result?,
                &minimal_data(),
                &value_bytes(1, false)?,
                0,
                &bytes,
            )?;
        } else {
            assert_eq!(result.err(), Some(corrupt()));
        }
    }
    Ok(())
}

fn origin(
    kind: u8,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<(CompletePolicyReplaceFresh, ControlState, Vec<u8>, Vec<u8>)> {
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let marks = BTreeMap::new();
    let raw = current(0)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let base = ControlState::initial(stamp(0)?);
    let mut data = minimal_data();
    if kind == 1 {
        data.max_active_ms = Some(0);
    }
    if kind == 2 {
        data.retention = Some(RetentionOverride {
            ttl_ms: None,
            history_max_events: Some(18),
        });
    }
    let bytes = if kind == 0 {
        frame(false, None, b"q", 0)?
    } else {
        input_frame(data)?
    };
    let expected = match kind {
        0 => value_bytes(1, false)?,
        1 => b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity".to_vec(),
        _ => b"ERR flow retention history_max_events must be between 1 and 17".to_vec(),
    };
    let mut supplied = view(
        GateView {
            scope: &scope,
            control: base,
            outcomes: &outcomes,
        },
        &policies,
        &marks,
        k1_safety(),
    );
    supplied.history_max_events_limit = Ok(17);
    let plan = apply(&supplied, &bytes, 0, limits, charge)?;
    control(&plan, base, &bytes, 0, 0, false)?;
    let (_, _, next, decision) = plan.into_parts();
    let DispatchDecision::Fresh(bundle) = decision else {
        return Err("real raw-current origin".into());
    };
    outcome(&bundle.1, &bytes, &expected, 0, 0);
    if kind == 0 {
        assert_eq!(bundle.1.status(), RetainedStatus::Accepted);
        assert!(bundle.0.is_some());
    } else {
        assert!(bundle.0.is_none());
        let RetainedStatus::CompletePolicyReplaceRejected(reason) = bundle.1.status() else {
            return Err("incoming-only content descriptor".into());
        };
        assert_eq!(
            reason.kind(),
            if kind == 1 {
                Kind::InvalidMaxActive
            } else {
                Kind::RetentionHistory { maximum: 17 }
            }
        );
        assert_eq!(
            reason.history_max_events_limit(),
            if kind == 1 { None } else { Some(17) }
        );
    }
    Ok((bundle, next.next(), bytes, expected))
}

#[test]
fn original_current_replace_preserves_nonfresh_and_real_retained_replays() -> TestResult {
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    for kind in [0, 1, 2] {
        let (bundle, base, original, expected) = origin(kind, limits, &mut charge)?;
        let request = RequestId::new(b"q".to_vec())?;
        let outcomes = BTreeMap::from([(request.clone(), bundle.1)]);
        let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&[131, 73, 0, 1]))]);
        let marks = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::AllocationFailed))]);
        let bad = OriginalEtfSafety::new(&["duplicate", "duplicate"], &[], None, &[]);
        let mut supplied = view(
            GateView {
                scope: &scope,
                control: base,
                outcomes: &outcomes,
            },
            &policies,
            &marks,
            bad,
        );
        supplied.history_max_events_limit = Err(ApplyError::PositionConflict);
        supplied.outcome_retention_ms = Err(ApplyError::InvalidAuthority);
        for (case, time, index) in [(0, 0_u64, 0), (1, 99, 1), (2, 100, 1), (3, 100, 1)] {
            let mut bytes = original.clone();
            bytes[61..69].copy_from_slice(&time.to_be_bytes());
            bytes[71..79].copy_from_slice(&time.to_be_bytes());
            if case == 3 {
                bytes[123] ^= 1;
                let hash: [u8; 32] = Sha256::digest(&bytes[83..]).into();
                bytes[29..61].copy_from_slice(&hash);
            }
            let before = charge.g();
            let plan = apply(&supplied, &bytes, index, limits, &mut charge)?;
            control(&plan, base, &bytes, time, index, case == 0)?;
            match case {
                0 => assert!(matches!(plan.decision(), DispatchDecision::LogReplay)),
                1 => {
                    let DispatchDecision::Replay(out) = plan.decision() else {
                        return Err("real retained replay".into());
                    };
                    outcome(out, &original, &expected, 0, 0);
                    assert_eq!(out.status(), outcomes[&request].status());
                    let replayed: &RetainedOutcome = out;
                    let original: &RetainedOutcome = &outcomes[&request];
                    assert!(std::ptr::eq(replayed, original));
                }
                2 => assert!(matches!(plan.decision(), DispatchDecision::Expired)),
                _ => assert!(matches!(plan.decision(), DispatchDecision::IntentConflict)),
            }
            if kind != 1 {
                let fee = [4_813, 5_658, 5_658, 5_530][case];
                assert_eq!(charge.g(), before + fee + if kind == 2 { 402 } else { 0 });
            }
            assert_eq!(charge.p(), 448);
            outcome(&outcomes[&request], &original, &expected, 0, 0);
        }
        if let Some((_, value)) = bundle.0 {
            assert_eq!(value.encoded(), value_bytes(1, false)?);
        }
    }
    Ok(())
}

#[test]
fn original_current_replace_checks_context_and_both_migration_reads_before_exhaustion() -> TestResult
{
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let scope = scope()?;
    let outcomes = BTreeMap::new();
    let raw = current(MAX_POLICY_GENERATION)?;
    let policies = BTreeMap::from([(b"r".to_vec(), ReadValue::Binary(&raw))]);
    let active = BTreeMap::from([(b"r".to_vec(), Ok(MAX_POLICY_GENERATION))]);
    let failed = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::PositionConflict))]);
    let other_failed = BTreeMap::from([(b"r".to_vec(), Err(ApplyError::AllocationFailed))]);
    let empty = BTreeMap::new();
    let gate = GateView {
        scope: &scope,
        control: ControlState::initial(stamp(0)?),
        outcomes: &outcomes,
    };
    let bytes = frame(false, None, b"q", 0)?;
    let mut supplied = view(gate, &policies, &active, k1_safety());
    supplied.completed_migrations = &failed;
    assert_eq!(
        apply(&supplied, &bytes, 0, limits, &mut charge).err(),
        Some(RawError::Replace(ExistingError::Apply(
            ApplyError::PositionConflict
        )))
    );
    supplied.active_migrations = &failed;
    supplied.completed_migrations = &other_failed;
    assert_eq!(
        apply(&supplied, &bytes, 0, limits, &mut charge).err(),
        Some(RawError::Replace(ExistingError::Apply(
            ApplyError::PositionConflict
        )))
    );
    supplied.active_migrations = &active;
    supplied.completed_migrations = &empty;
    let plan = apply(&supplied, &bytes, 0, limits, &mut charge)?;
    rejected(&plan, b"ERR flow policy generation exhausted", &bytes)?;
    let DispatchDecision::Fresh((None, out)) = plan.decision() else {
        return Err("exhaustion".into());
    };
    assert!(
        matches!(out.status(), RetainedStatus::PolicyRejected(reason) if reason.error() == PolicyError::GenerationExhausted)
    );
    supplied.history_max_events_limit = Err(ApplyError::AllocationFailed);
    let mut bad = minimal_data();
    bad.max_active_ms = Some(0);
    let bad_frame = input_frame(bad)?;
    assert_eq!(
        apply(&supplied, &bad_frame, 0, limits, &mut charge).err(),
        Some(RawError::Replace(ExistingError::Apply(
            ApplyError::AllocationFailed
        )))
    );
    for value in [0, 1_000_001] {
        supplied.history_max_events_limit = Ok(value);
        assert_eq!(
            apply(&supplied, &bytes, 0, limits, &mut charge).err(),
            Some(RawError::Replace(ExistingError::Apply(
                ApplyError::InvalidAuthority
            )))
        );
    }
    cases::assert_source_boundary(gate, limits, &mut charge)?;
    cases::assert_dominating_migrations(gate, limits, &mut charge)?;
    Ok(())
}

#[test]
fn original_current_replace_retains_only_incoming_content_and_generation_errors() -> TestResult {
    let limits = profile(200_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    for kind in [1, 2] {
        let (first, _, bytes, expected) = origin(kind, limits, &mut charge)?;
        outcome(&first.1, &bytes, &expected, 0, 0);
    }
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
    let mut supplied = view(gate, &policies, &marks, k1_safety());
    supplied.outcome_retention_ms = Err(ApplyError::PositionConflict);
    for expected in [1, MAX_POLICY_GENERATION + 1] {
        let bytes = frame(false, Some(expected), b"q", 0)?;
        assert_eq!(
            apply(&supplied, &bytes, 0, limits, &mut charge).err(),
            Some(RawError::Replace(ExistingError::Apply(
                ApplyError::PositionConflict
            )))
        );
    }
    let mut too_wide = minimal_data();
    too_wide.version = Some(wf_domain::policy::definition::PolicyVersion::Bytes(
        vec![0; 262_143],
    ));
    assert_eq!(
        apply(&supplied, &input_frame(too_wide)?, 0, limits, &mut charge).err(),
        Some(schema(PolicySchemaError::SnapshotBytes))
    );
    for cap in [767, 768] {
        let bytes = frame(true, None, b"q", 0)?;
        assert_eq!(bytes.len(), 477);
        let expected = value_bytes(1, true)?;
        assert_eq!(4 + 2 * expected.len(), 768);
        let result = decide_original_complete_policy_replace_frame(
            view(gate, &policies, &marks, k1_safety()),
            &bytes,
            log(0),
            limits,
            AdmissionLimits::new(1_048_576, cap, 1_000)?,
            &mut charge,
        );
        if cap == 767 {
            assert_eq!(result.err(), Some(request(EncodingError::ByteLimit)));
        } else {
            accepted(&result?, &present_data(), &expected, 0, &bytes)?;
        }
    }
    Ok(())
}

fn source603() -> TestResult<Vec<u8>> {
    let bytes = super::literals::replace_frame_hex(concat!(
        "836802770e666c6f775f706f6c6963795f76317400000003770572657472797400000001",
        "770c6d61785f617474656d7074736105770673746174657374000000016d0000000b",
        "6368617267655f636172647400000001770572657472797400000001770c6d6178",
        "5f617474656d70747361027704747970656d00000008636865636b6f7574"
    ))?;
    assert_eq!(bytes.len(), 133);
    assert_eq!(&bytes[..3], &[131, 104, 2]);
    Ok(bytes)
}
