//! Inert private test-case and observation proposal.

use sha2::{Digest, Sha256};
use wf_domain::policy::BackoffKind;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchCaller, CompleteCorePolicyPatchInput, CompletePolicyEncodingError,
    CompletePolicyRequestError, CompletePolicyValue, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, prepare_complete_core_policy_patch, prepare_complete_policy_value_v2,
    restore_complete_policy,
};
use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicyPatch,
};
use wf_domain::policy::installation::PolicyError;
use wf_domain::retention::{RetentionError, RetentionOverride};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_ports::log::prepare_complete_core_policy_patch_intent;
use wf_types::version::CommittedTime;

use super::super::{TestResult, minimal_data, profile};
use super::{assert_core_patch_profile_refusal, core_patch_input};

pub(super) fn assert_core_patch_default_caller(
    caller: &CompleteCorePolicyPatchCaller,
    expected: &PolicyPatch,
    guard: Option<u64>,
    stamp: CommittedTime,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
) {
    assert_eq!(caller.input().scope().tenant().as_bytes(), b"t");
    assert_eq!(caller.input().scope().group().as_bytes(), b"g");
    assert_eq!(caller.input().scope().recovery().as_bytes(), b"r");
    assert_eq!(caller.input().request().as_bytes(), b"q");
    assert_eq!(caller.input().stamp(), stamp);
    assert_eq!(caller.input().command().workflow_type.as_slice(), b"r");
    assert_eq!(caller.input().command().expected_generation, guard);
    assert_eq!(caller.input().command().patch, *expected);
    assert!(caller.schema_limits() == limits);
    assert_eq!(caller.admission_limits(), admission);
}

pub(super) fn assert_core_patch_scalar_widths(
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (case, fields_hex, bytes, nodes) in [
        (
            0,
            "0101ffffffffffffffff01ffffffffffffffff0101ffff01010301ffffffffffffffff01000000000000000001ff010000000200ff0101ffffffffffffffff01ffffffff",
            116,
            22_u64,
        ),
        (
            1,
            "010100000000000000000100000000000000000101000001010001000000000000000001000000000000000001000100000000010100000000000000000100000000",
            114,
            22,
        ),
        (
            2,
            "01000000000200ff01ffffffffffffffff0101ffff01010301ffffffffffffffff01000000000000000001ff010000000200ff0101ffffffffffffffff01ffffffff",
            114,
            23,
        ),
    ] {
        let mut literal = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
        literal.extend_from_slice(&[0, 0, 0, 1, b't', 0, 0, 0, 0, 1, b'r']);
        for pair in fields_hex.as_bytes().as_chunks::<2>().0 {
            literal.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
        }
        literal.extend_from_slice(&[0; 4]);
        assert_eq!(literal.len(), bytes);
        let digest: [u8; 32] = Sha256::digest(&literal).into();
        let mut patch = PolicyPatch::default();
        let mut expected = PolicyPatch::default();
        for value in [&mut patch, &mut expected] {
            value.version = FieldPatch::Set(match case {
                0 => PolicyVersion::Number(u64::MAX),
                1 => PolicyVersion::Number(0),
                _ => PolicyVersion::Bytes(vec![0, 0xff]),
            });
            value.max_active_ms = FieldPatch::Set(if case == 1 { 0 } else { u64::MAX });
            value.retry = FieldPatch::Set(RetryPatch {
                max_retries: Some(if case == 1 { 0 } else { u16::MAX }),
                backoff: Some(BackoffPatch {
                    kind: Some(if case == 1 {
                        BackoffKind::None
                    } else {
                        BackoffKind::Exponential
                    }),
                    base_ms: Some(if case == 1 { 0 } else { u64::MAX }),
                    max_ms: Some(0),
                    jitter_pct: Some(if case == 1 { 0 } else { u8::MAX }),
                }),
                exhausted_to: Some(if case == 1 { Vec::new() } else { vec![0, 0xff] }),
            });
            value.retention = FieldPatch::Set(RetentionOverride {
                ttl_ms: Some(if case == 1 { 0 } else { u64::MAX }),
                history_max_events: Some(if case == 1 { 0 } else { u32::MAX }),
            });
        }
        let input = core_patch_input(CompleteCorePolicyPatchInput {
            workflow_type: b"r".to_vec(),
            expected_generation: None,
            patch,
        })?;
        let stamp = input.stamp();
        let before = charge.g();
        let intent = prepare_complete_core_policy_patch_intent(input, limits, admission, charge)?;
        assert_core_patch_default_caller(
            intent.caller(),
            &expected,
            None,
            stamp,
            limits,
            admission,
        );
        assert_eq!(intent.digest(), &digest);
        assert_eq!(
            charge.g(),
            before + 256 * nodes + 3 * u64::try_from(bytes)? + 128
        );
    }
    Ok(())
}

pub(super) fn assert_core_patch_state_actions(
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (case, row_hex, bytes, nodes) in [
        (0, "00000001000000016101000000", 61, 8_u64),
        (1, "0000000100000001610101000000", 62, 9),
        (2, "0000000100000001610101010000", 62, 9),
        (3, "00000001000000016101020000", 61, 8),
        (4, "00000001000000016101000200", 61, 8),
        (5, "00000001000000016101000100000000", 64, 10),
        (6, "00000001000000016101000002", 61, 8),
        (7, "000000010000000161010000010000", 63, 10),
        (8, "00000001000000016100", 58, 7),
        (
            9,
            "000000040000000100010100000000000001610000000001620000000001ff0101010000",
            84,
            17,
        ),
    ] {
        let mut literal = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
        literal.extend_from_slice(&[0, 0, 0, 1, b't', 0, 0, 0, 0, 1, b'r', 0, 0, 0, 0]);
        for pair in row_hex.as_bytes().as_chunks::<2>().0 {
            literal.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
        }
        assert_eq!(literal.len(), bytes);
        let digest: [u8; 32] = Sha256::digest(&literal).into();
        let mut patch = PolicyPatch::default();
        let mut expected = PolicyPatch::default();
        for value in [&mut patch, &mut expected] {
            if case == 9 {
                value.states.insert(b"a".to_vec(), StateChange::Remove);
                value.states.insert(b"b".to_vec(), StateChange::Remove);
                for (name, mode) in [
                    (vec![0], StateMode::Parallel),
                    (vec![0xff], StateMode::Fifo),
                ] {
                    value.states.insert(
                        name,
                        StateChange::Patch(StatePolicyPatch {
                            mode: FieldPatch::Set(mode),
                            ..StatePolicyPatch::default()
                        }),
                    );
                }
            } else if case == 8 {
                value.states.insert(b"a".to_vec(), StateChange::Remove);
            } else {
                let mut state = StatePolicyPatch::default();
                match case {
                    1 => state.mode = FieldPatch::Set(StateMode::Parallel),
                    2 => state.mode = FieldPatch::Set(StateMode::Fifo),
                    3 => state.mode = FieldPatch::Clear,
                    4 => state.retry = FieldPatch::Clear,
                    5 => state.retry = FieldPatch::Set(RetryPatch::default()),
                    6 => state.retention = FieldPatch::Clear,
                    7 => state.retention = FieldPatch::Set(RetentionOverride::default()),
                    _ => {}
                }
                value
                    .states
                    .insert(b"a".to_vec(), StateChange::Patch(state));
            }
        }
        let input = core_patch_input(CompleteCorePolicyPatchInput {
            workflow_type: b"r".to_vec(),
            expected_generation: None,
            patch,
        })?;
        let stamp = input.stamp();
        let before = charge.g();
        let intent = prepare_complete_core_policy_patch_intent(input, limits, admission, charge)?;
        assert_core_patch_default_caller(
            intent.caller(),
            &expected,
            None,
            stamp,
            limits,
            admission,
        );
        assert_eq!(intent.digest(), &digest);
        assert_eq!(
            (charge.g(), charge.p()),
            (before + 256 * nodes + 3 * u64::try_from(bytes)? + 128, 0)
        );
    }
    Ok(())
}

pub(super) fn assert_core_patch_current_models(
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
    minimal_digest: &[u8; 32],
) -> TestResult {
    // Paid standalone models, not a retained or Fresh runtime gate.
    let mut baseline: Option<CompletePolicyValue> = None;
    for (generation, high_water, active, next, set_same) in [
        (1_u64, 0_u64, 1_000_u64, 2_u64, false),
        (2, 7, 2_000, 8, false),
        (1, 0, 1_000, 2, true),
    ] {
        let mut value = minimal_data();
        value.max_active_ms = Some(active);
        let mut expected = minimal_data();
        expected.max_active_ms = Some(active);
        let mut encoded = vec![
            0x46, 0x57, 0x50, 0x56, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 22, 0x46, 0x57, 0x50,
            0x53, 0, 2, 0, 0, 0, 1, b'r', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        encoded[6..14].copy_from_slice(&next.to_be_bytes());
        encoded[14..18].copy_from_slice(&30_u32.to_be_bytes());
        encoded[30] = 1;
        encoded.splice(31..31, active.to_be_bytes());
        let mut original_encoded = encoded.clone();
        original_encoded[6..14].copy_from_slice(&generation.to_be_bytes());
        let current = restore_complete_policy(generation, value, limits, charge)?;
        let current = prepare_complete_policy_value_v2(current, limits, charge)?;
        let mut patch = PolicyPatch::default();
        if set_same {
            patch.max_active_ms = FieldPatch::Set(active);
        }
        let merged = prepare_complete_core_policy_patch(
            Some(&current),
            high_water,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: None,
                patch,
            },
            1_000_000,
            limits,
            charge,
        )?;
        assert!(merged.data() == &expected);
        assert_eq!(merged.generation(), next);
        assert_eq!(merged.encoded(), encoded.as_slice());
        assert!(current.data() == &expected);
        assert_eq!(current.generation(), generation);
        assert_eq!(current.encoded(), original_encoded.as_slice());
        if set_same {
            let old = baseline.as_ref().ok_or("unchanged model missing")?;
            assert!(old.data() == merged.data());
            assert_eq!(old.generation(), merged.generation());
            assert_eq!(old.encoded(), merged.encoded());
        }
        let mut patch = PolicyPatch::default();
        let mut expected_patch = PolicyPatch::default();
        if set_same {
            patch.max_active_ms = FieldPatch::Set(active);
            expected_patch.max_active_ms = FieldPatch::Set(active);
        }
        let input = core_patch_input(CompleteCorePolicyPatchInput {
            workflow_type: b"r".to_vec(),
            expected_generation: None,
            patch,
        })?;
        let stamp = input.stamp();
        let mut literal = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
        literal.extend_from_slice(&[0, 0, 0, 1, b't', 0, 0, 0, 0, 1, b'r', 0]);
        if set_same {
            literal.push(1);
            literal.extend_from_slice(&active.to_be_bytes());
            literal.extend_from_slice(&[0; 6]);
        } else {
            literal.extend_from_slice(&[0; 7]);
        }
        let digest: [u8; 32] = Sha256::digest(&literal).into();
        let before = charge.g();
        let intent = prepare_complete_core_policy_patch_intent(input, limits, admission, charge)?;
        assert_core_patch_default_caller(
            intent.caller(),
            &expected_patch,
            None,
            stamp,
            limits,
            admission,
        );
        assert_eq!(intent.digest(), &digest);
        assert_eq!(intent.digest() == minimal_digest, !set_same);
        let fee = if set_same { 1_844 } else { 1_564 };
        assert_eq!(charge.g(), before + fee);
        if !set_same && generation == 1 {
            baseline = Some(merged);
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_history_context(
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
    minimal_digest: &[u8; 32],
) -> TestResult {
    for captured in [2_u32, 1] {
        let mut value = minimal_data();
        value.retention = Some(RetentionOverride {
            ttl_ms: None,
            history_max_events: Some(2),
        });
        let mut expected = minimal_data();
        expected.retention = Some(RetentionOverride {
            ttl_ms: None,
            history_max_events: Some(2),
        });
        let mut original_encoded = vec![
            0x46, 0x57, 0x50, 0x56, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 22, 0x46, 0x57, 0x50,
            0x53, 0, 2, 0, 0, 0, 1, b'r', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        original_encoded[14..18].copy_from_slice(&28_u32.to_be_bytes());
        original_encoded[32] = 1;
        original_encoded.splice(33..33, [0, 1, 0, 0, 0, 2]);
        let mut encoded = original_encoded.clone();
        encoded[6..14].copy_from_slice(&2_u64.to_be_bytes());
        let current = restore_complete_policy(1, value, limits, charge)?;
        let current = prepare_complete_policy_value_v2(current, limits, charge)?;
        assert!(current.data() == &expected);
        assert_eq!(current.generation(), 1);
        assert_eq!(current.encoded(), original_encoded.as_slice());
        let before = charge.g();
        let result = prepare_complete_core_policy_patch(
            Some(&current),
            0,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: None,
                patch: PolicyPatch::default(),
            },
            captured,
            limits,
            charge,
        );
        if captured == 1 {
            let error = result.err().ok_or("captured history refusal missing")?;
            assert_eq!(
                error,
                CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(
                    PolicyError::Retention(RetentionError::History)
                ),)
            );
        } else {
            let merged = result?;
            assert!(merged.data() == &expected);
            assert_eq!(merged.generation(), 2);
            assert_eq!(merged.encoded(), encoded.as_slice());
        }
        assert!(charge.g() > before);
        assert_eq!(charge.p(), 0);
        assert!(current.data() == &expected);
        assert_eq!(current.generation(), 1);
        assert_eq!(current.encoded(), original_encoded.as_slice());
        let input = core_patch_input(CompleteCorePolicyPatchInput {
            workflow_type: b"r".to_vec(),
            expected_generation: None,
            patch: PolicyPatch::default(),
        })?;
        let stamp = input.stamp();
        let before = charge.g();
        let intent = prepare_complete_core_policy_patch_intent(input, limits, admission, charge)?;
        assert_core_patch_default_caller(
            intent.caller(),
            &PolicyPatch::default(),
            None,
            stamp,
            limits,
            admission,
        );
        assert_eq!(intent.digest(), minimal_digest);
        assert_eq!(charge.g(), before + 1_564);
        assert!(current.data() == &expected);
        assert_eq!(current.generation(), 1);
        assert_eq!(current.encoded(), original_encoded.as_slice());
    }
    Ok(())
}

pub(super) fn assert_core_patch_original_owners(
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let workflow_type = b"r".to_vec();
    let version = vec![0, 0xff];
    let target = b"dead".to_vec();
    let name = b"queued".to_vec();
    let descriptors = (
        workflow_type.as_ptr(),
        version.as_ptr(),
        target.as_ptr(),
        name.as_ptr(),
    );
    let mut patch = PolicyPatch {
        version: FieldPatch::Set(PolicyVersion::Bytes(version)),
        retry: FieldPatch::Set(RetryPatch {
            exhausted_to: Some(target),
            ..RetryPatch::default()
        }),
        ..PolicyPatch::default()
    };
    patch.states.insert(
        name,
        StateChange::Patch(StatePolicyPatch {
            mode: FieldPatch::Set(StateMode::Fifo),
            ..StatePolicyPatch::default()
        }),
    );
    let mut expected = PolicyPatch {
        version: FieldPatch::Set(PolicyVersion::Bytes(vec![0, 0xff])),
        retry: FieldPatch::Set(RetryPatch {
            exhausted_to: Some(b"dead".to_vec()),
            ..RetryPatch::default()
        }),
        ..PolicyPatch::default()
    };
    expected.states.insert(
        b"queued".to_vec(),
        StateChange::Patch(StatePolicyPatch {
            mode: FieldPatch::Set(StateMode::Fifo),
            ..StatePolicyPatch::default()
        }),
    );
    let input = core_patch_input(CompleteCorePolicyPatchInput {
        workflow_type,
        expected_generation: None,
        patch,
    })?;
    let mut literal = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
    literal.extend_from_slice(&[0, 0, 0, 1, b't', 0, 0, 0, 0, 1, b'r']);
    let fields_hex =
        "01000000000200ff000100000100000004646561640000000001000000067175657565640101010000";
    for pair in fields_hex.as_bytes().as_chunks::<2>().0 {
        literal.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
    }
    assert_eq!(literal.len(), 85);
    let digest: [u8; 32] = Sha256::digest(&literal).into();
    let stamp = input.stamp();
    let before = charge.g();
    let intent = prepare_complete_core_policy_patch_intent(input, limits, admission, charge)?;
    let caller = intent.caller();
    assert_core_patch_default_caller(caller, &expected, None, stamp, limits, admission);
    assert_eq!(intent.digest(), &digest);
    assert_eq!((charge.g(), charge.p()), (before + 4_223, 0));
    let FieldPatch::Set(PolicyVersion::Bytes(version)) = &caller.input().command().patch.version
    else {
        return Err("original version missing".into());
    };
    let FieldPatch::Set(retry) = &caller.input().command().patch.retry else {
        return Err("original retry missing".into());
    };
    let target = retry
        .exhausted_to
        .as_ref()
        .ok_or("original target missing")?;
    let (name, _) = caller
        .input()
        .command()
        .patch
        .states
        .first_key_value()
        .ok_or("state missing")?;
    assert_eq!(
        caller.input().command().workflow_type.as_ptr(),
        descriptors.0
    );
    assert_eq!(version.as_ptr(), descriptors.1);
    assert_eq!(target.as_ptr(), descriptors.2);
    assert_eq!(name.as_ptr(), descriptors.3);
    Ok(())
}

pub(super) fn assert_core_patch_account_case(
    maximum: u64,
    target: u64,
    attempts: u64,
    guarded: bool,
    peak: bool,
    admission: AdmissionLimits,
) -> TestResult {
    let limits = profile(maximum)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let mut expected_indexed = minimal_data();
    expected_indexed.indexed_attributes = Some(vec![b"a".to_vec()]);
    let held = if peak {
        let mut indexed = minimal_data();
        indexed.indexed_attributes = Some(vec![b"a".to_vec()]);
        Some(restore_complete_policy(1, indexed, limits, &mut charge)?)
    } else {
        None
    };
    let setup = if peak { 1_928 } else { 0 };
    let scratch = if peak { 64 } else { 0 };
    let initial = (setup, scratch, setup + scratch);
    assert_eq!((charge.g(), charge.p(), charge.total()), initial);
    if let Some(held) = &held {
        assert!(held.data() == &expected_indexed);
        assert_eq!(held.generation(), 1);
    }
    let mut literal = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
    literal.extend_from_slice(&[0, 0, 0, 1, b't']);
    if guarded {
        literal.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0, 0]);
    } else {
        literal.push(0);
    }
    literal.extend_from_slice(&[0, 0, 0, 1, b'r', 0, 0, 0, 0, 0, 0, 0, 0]);
    let digest: [u8; 32] = Sha256::digest(&literal).into();
    let guard = if guarded { Some(0) } else { None };
    let fee = if guarded { 1_844 } else { 1_564 };
    let last = if guarded { 188 } else { 180 };
    let expected = PolicyPatch::default();
    let mut outputs = Vec::new();
    for index in 0..attempts {
        let workflow_type = b"r".to_vec();
        let descriptor = workflow_type.as_ptr();
        let input = core_patch_input(CompleteCorePolicyPatchInput {
            workflow_type,
            expected_generation: guard,
            patch: PolicyPatch::default(),
        })?;
        let stamp = input.stamp();
        let result =
            prepare_complete_core_policy_patch_intent(input, limits, admission, &mut charge);
        let refused = index + 1 == attempts && maximum < target;
        if refused {
            let error = result.err().ok_or("final SHA work refusal missing")?;
            assert_eq!(
                error,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::WorkBytes {
                        bytes: target,
                        maximum,
                    }
                ),)
            );
        } else {
            let value = result?;
            let caller = value.caller();
            assert_core_patch_default_caller(caller, &expected, guard, stamp, limits, admission);
            assert_eq!(value.digest(), &digest);
            assert_eq!(caller.input().command().workflow_type.as_ptr(), descriptor);
            outputs.push(value);
        }
        let g = if refused {
            target - scratch - last
        } else {
            setup + (index + 1) * fee
        };
        let expected_charge = (g, scratch, g + scratch);
        assert_eq!((charge.g(), charge.p(), charge.total()), expected_charge);
        for old in &outputs {
            let caller = old.caller();
            assert_core_patch_default_caller(caller, &expected, guard, stamp, limits, admission);
            assert_eq!(old.digest(), &digest);
        }
    }
    assert_core_patch_profile_refusal(maximum, admission, &mut charge)?;
    if let Some(held) = &held {
        assert!(held.data() == &expected_indexed);
        assert_eq!(held.generation(), 1);
    }
    Ok(())
}
