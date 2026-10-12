//! Inert proposal: original normalized partial-Patch caller intent controls.

use sha2::{Digest, Sha256};
use wf_domain::policy::BackoffKind;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput, CompletePolicyEncodingError,
    CompletePolicyRequestError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    prepare_complete_core_policy_patch_caller,
};
use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicyPatch,
};
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::prepare_complete_core_policy_patch_intent;
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, HlcTimestamp};

use super::{TestResult, profile};

mod cases;
mod decode;
mod frame;
mod gate;
mod prepare;

use cases::{
    assert_core_patch_account_case, assert_core_patch_current_models,
    assert_core_patch_default_caller, assert_core_patch_history_context,
    assert_core_patch_original_owners, assert_core_patch_scalar_widths,
    assert_core_patch_state_actions,
};

fn core_patch_input(
    command: CompleteCorePolicyPatchInput,
) -> TestResult<CompleteCorePolicyPatchRequestInput> {
    Ok(CompleteCorePolicyPatchRequestInput::new(
        RequestScope::new(
            TenantId::new(b"t".to_vec())?,
            GroupId::new(b"g".to_vec())?,
            RecoveryId::new(b"r".to_vec())?,
        ),
        RequestId::new(b"q".to_vec())?,
        CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?,
        command,
    ))
}

#[test]
fn core_patch_intent_matches_independent_partial_command_literals() -> TestResult {
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let literals = [
        "57462d494e54454e5400434f4d504c4554452d434f52452d50415443480056320000000001740000000001720000000000000000",
        "57462d494e54454e5400434f4d504c4554452d434f52452d504154434800563200000000017401000000000000000000000001720000000000000000",
        "57462d494e54454e5400434f4d504c4554452d434f52452d504154434800563200000000017401ffffffffffffffff00000001720000000000000000",
        "57462d494e54454e5400434f4d504c4554452d434f52452d50415443480056320000000001740000000001720000010000000000000000",
        "57462d494e54454e5400434f4d504c4554452d434f52452d5041544348005632000000000174000000000172000001000100000000000000000000",
        "57462d494e54454e5400434f4d504c4554452d434f52452d50415443480056320000000001740000000001720000000000000001000000016100",
        "57462d494e54454e5400434f4d504c4554452d434f52452d50415443480056320000000001740000000001720000000000000001000000016101000000",
        "57462d494e54454e5400434f4d504c4554452d434f52452d5041544348005632000000000174000000000172000000000000000200000006717565756564010001000001000000046465616400000000067265766965770101010000",
    ];
    let sizes = [52, 60, 60, 55, 59, 58, 61, 92];
    let nodes = [5, 6, 6, 7, 9, 7, 8, 16];
    let fees = [1_564_u64, 1_844, 1_844, 2_085, 2_609, 2_094, 2_359, 4_500];
    for (case, hex) in literals.iter().enumerate() {
        let mut literal = Vec::new();
        for pair in hex.as_bytes().as_chunks::<2>().0 {
            literal.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
        }
        assert_eq!(hex.len(), sizes[case] * 2);
        assert_eq!(literal.len(), sizes[case]);
        let bytes = u64::try_from(sizes[case])?;
        assert_eq!(256 * nodes[case] + 3 * bytes + 128, fees[case]);
        let digest: [u8; 32] = Sha256::digest(&literal).into();
        let mut patch = PolicyPatch::default();
        let mut expected = PolicyPatch::default();
        for value in [&mut patch, &mut expected] {
            match case {
                3 => value.retry = FieldPatch::Set(RetryPatch::default()),
                4 => {
                    value.retry = FieldPatch::Set(RetryPatch {
                        backoff: Some(BackoffPatch::default()),
                        ..RetryPatch::default()
                    });
                }
                5 => {
                    value.states.insert(b"a".to_vec(), StateChange::Remove);
                }
                6 => {
                    value.states.insert(
                        b"a".to_vec(),
                        StateChange::Patch(StatePolicyPatch::default()),
                    );
                }
                7 => {
                    value.states.insert(
                        b"queued".to_vec(),
                        StateChange::Patch(StatePolicyPatch {
                            retry: FieldPatch::Set(RetryPatch {
                                exhausted_to: Some(b"dead".to_vec()),
                                ..RetryPatch::default()
                            }),
                            ..StatePolicyPatch::default()
                        }),
                    );
                    value.states.insert(
                        b"review".to_vec(),
                        StateChange::Patch(StatePolicyPatch {
                            mode: FieldPatch::Set(StateMode::Fifo),
                            ..StatePolicyPatch::default()
                        }),
                    );
                }
                _ => {}
            }
        }
        let guard = match case {
            1 => Some(0),
            2 => Some(u64::MAX),
            _ => None,
        };
        let input = core_patch_input(CompleteCorePolicyPatchInput {
            workflow_type: b"r".to_vec(),
            expected_generation: guard,
            patch,
        })?;
        let stamp = input.stamp();
        let before = charge.g();
        let intent =
            prepare_complete_core_policy_patch_intent(input, limits, admission, &mut charge)?;
        let caller = intent.caller();
        assert_core_patch_default_caller(caller, &expected, guard, stamp, limits, admission);
        assert_eq!(intent.digest(), &digest);
        assert_eq!((charge.g(), charge.p()), (before + fees[case], 0));
    }
    Ok(())
}

#[test]
fn core_patch_intent_preserves_unchanged_clear_and_empty_set() -> TestResult {
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    for (field, empty_payload, extra_nodes) in [
        (0, &[1, 0, 0, 0, 0, 0][..], 2_u64),
        (1, &[1, 0, 0, 0, 0, 0, 0, 0, 0][..], 1),
        (2, &[1, 0, 0, 0][..], 2),
        (3, &[1, 0, 0][..], 2),
    ] {
        for action in 0..3 {
            let mut literal = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
            literal.extend_from_slice(&[0, 0, 0, 1, b't', 0, 0, 0, 0, 1, b'r']);
            literal.extend_from_slice(&[0; 8]);
            let replacement = match action {
                0 => &[0][..],
                1 => &[2][..],
                _ => empty_payload,
            };
            literal.splice(44 + field..45 + field, replacement.iter().copied());
            let bytes = u64::try_from(literal.len())?;
            let nodes = 5 + if action == 2 { extra_nodes } else { 0 };
            let fee = 256 * nodes + 3 * bytes + 128;
            let digest: [u8; 32] = Sha256::digest(&literal).into();
            let mut patch = PolicyPatch::default();
            let mut expected = PolicyPatch::default();
            for value in [&mut patch, &mut expected] {
                match (field, action) {
                    (0, 1) => value.version = FieldPatch::Clear,
                    (0, 2) => value.version = FieldPatch::Set(PolicyVersion::Bytes(Vec::new())),
                    (1, 1) => value.max_active_ms = FieldPatch::Clear,
                    (1, 2) => value.max_active_ms = FieldPatch::Set(0),
                    (2, 1) => value.retry = FieldPatch::Clear,
                    (2, 2) => value.retry = FieldPatch::Set(RetryPatch::default()),
                    (3, 1) => value.retention = FieldPatch::Clear,
                    (3, 2) => value.retention = FieldPatch::Set(RetentionOverride::default()),
                    _ => {}
                }
            }
            let input = core_patch_input(CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: None,
                patch,
            })?;
            let stamp = input.stamp();
            let before = charge.g();
            let intent =
                prepare_complete_core_policy_patch_intent(input, limits, admission, &mut charge)?;
            let caller = intent.caller();
            assert_core_patch_default_caller(caller, &expected, None, stamp, limits, admission);
            assert_eq!(intent.digest(), &digest);
            assert_eq!((charge.g(), charge.p()), (before + fee, 0));
        }
    }
    Ok(())
}

#[test]
fn core_patch_intent_keeps_partial_backoff_and_state_actions() -> TestResult {
    let limits = PolicySchemaLimits::new(PolicyLimits::new(2)?, 20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    assert_core_patch_scalar_widths(limits, admission, &mut charge)?;
    for (tag, kind) in [
        BackoffKind::None,
        BackoffKind::Fixed,
        BackoffKind::Linear,
        BackoffKind::Exponential,
    ]
    .into_iter()
    .enumerate()
    {
        let mut patch = PolicyPatch::default();
        let mut expected = PolicyPatch::default();
        for value in [&mut patch, &mut expected] {
            value.retry = FieldPatch::Set(RetryPatch {
                backoff: Some(BackoffPatch {
                    kind: Some(kind),
                    ..BackoffPatch::default()
                }),
                ..RetryPatch::default()
            });
        }
        let mut literal = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
        literal.extend_from_slice(&[0, 0, 0, 1, b't', 0, 0, 0, 0, 1, b'r', 0, 0]);
        literal.extend_from_slice(&[1, 0, 1, 1, u8::try_from(tag)?, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let digest: [u8; 32] = Sha256::digest(&literal).into();
        let input = core_patch_input(CompleteCorePolicyPatchInput {
            workflow_type: b"r".to_vec(),
            expected_generation: None,
            patch,
        })?;
        let stamp = input.stamp();
        let before = charge.g();
        let intent =
            prepare_complete_core_policy_patch_intent(input, limits, admission, &mut charge)?;
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
            (literal.len(), charge.g(), charge.p()),
            (60, before + 2_868, 0)
        );
    }
    assert_core_patch_state_actions(limits, admission, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_intent_excludes_preparation_and_current_policy() -> TestResult {
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let mut minimal = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
    minimal.extend_from_slice(&[0, 0, 0, 1, b't', 0, 0, 0, 0, 1, b'r']);
    minimal.extend_from_slice(&[0; 8]);
    let minimal_digest: [u8; 32] = Sha256::digest(&minimal).into();
    for case in 0..9 {
        let tenant = if case == 5 { &b"t2"[..] } else { &b"t"[..] };
        let group = if case == 1 { &b"g2"[..] } else { &b"g"[..] };
        let recovery = if case == 2 { &b"r2"[..] } else { &b"r"[..] };
        let request = if case == 3 { &b"q2"[..] } else { &b"q"[..] };
        let stamp = if case == 4 {
            CommittedTime::new(HlcTimestamp::new(99, 7)?, 98)?
        } else {
            CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?
        };
        let workflow_type = if case == 6 { &b"r2"[..] } else { &b"r"[..] };
        let guard = if case == 7 { Some(0) } else { None };
        let mut patch = PolicyPatch::default();
        let mut expected = PolicyPatch::default();
        if case == 8 {
            patch.max_active_ms = FieldPatch::Set(1);
            expected.max_active_ms = FieldPatch::Set(1);
        }
        let mut literal = minimal.clone();
        match case {
            5 => {
                literal[33..37].copy_from_slice(&2_u32.to_be_bytes());
                literal.insert(38, b'2');
            }
            6 => {
                literal[39..43].copy_from_slice(&2_u32.to_be_bytes());
                literal.insert(44, b'2');
            }
            7 => {
                literal.splice(38..39, [1, 0, 0, 0, 0, 0, 0, 0, 0]);
            }
            8 => {
                literal.splice(45..46, [1, 0, 0, 0, 0, 0, 0, 0, 1]);
            }
            _ => {}
        }
        let digest: [u8; 32] = Sha256::digest(&literal).into();
        let input = CompleteCorePolicyPatchRequestInput::new(
            RequestScope::new(
                TenantId::new(tenant.to_vec())?,
                GroupId::new(group.to_vec())?,
                RecoveryId::new(recovery.to_vec())?,
            ),
            RequestId::new(request.to_vec())?,
            stamp,
            CompleteCorePolicyPatchInput {
                workflow_type: workflow_type.to_vec(),
                expected_generation: guard,
                patch,
            },
        );
        let intent =
            prepare_complete_core_policy_patch_intent(input, limits, admission, &mut charge)?;
        let caller = intent.caller();
        assert_eq!(caller.input().scope().tenant().as_bytes(), tenant);
        assert_eq!(caller.input().scope().group().as_bytes(), group);
        assert_eq!(caller.input().scope().recovery().as_bytes(), recovery);
        assert_eq!(caller.input().request().as_bytes(), request);
        assert_eq!(caller.input().stamp(), stamp);
        assert_eq!(
            caller.input().command().workflow_type.as_slice(),
            workflow_type
        );
        assert_eq!(caller.input().command().expected_generation, guard);
        assert_eq!(caller.input().command().patch, expected);
        assert!(caller.schema_limits() == limits);
        assert_eq!(caller.admission_limits(), admission);
        assert_eq!(intent.digest(), &digest);
        assert_eq!(intent.digest() == &minimal_digest, case <= 4);
    }
    // Actual paid models; no retained or Fresh runtime gate is executed.
    assert_core_patch_current_models(limits, admission, &mut charge, &minimal_digest)?;
    assert_core_patch_history_context(limits, admission, &mut charge, &minimal_digest)?;
    Ok(())
}

#[test]
fn core_patch_intent_retains_original_owners_and_caps() -> TestResult {
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    assert_core_patch_scope_limits(limits, admission, &mut charge)?;
    assert_core_patch_raw_limits(limits, admission, &mut charge)?;
    // Global bytes win over this write's prospective Work; prior length is paid.
    let global_profile = profile(788_167)?;
    let mut global_charge = PolicySchemaCharge::new(global_profile);
    let mut patch = PolicyPatch::default();
    for last in 0..4 {
        let mut key = vec![0; 262_144];
        key[262_143] = last;
        patch.states.insert(key, StateChange::Remove);
    }
    let input = core_patch_input(CompleteCorePolicyPatchInput {
        workflow_type: b"r".to_vec(),
        expected_generation: None,
        patch,
    })?;
    let error = prepare_complete_core_policy_patch_caller(
        input,
        global_profile,
        admission,
        &mut global_charge,
    )
    .err()
    .ok_or("global cap refusal missing")?;
    assert_eq!(
        error,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
    );
    assert_eq!((global_charge.g(), global_charge.p()), (788_167, 0));
    assert_core_patch_original_owners(limits, admission, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_intent_uses_one_nonrollback_schema_account() -> TestResult {
    let admission = AdmissionLimits::default();
    for (maximum, target, attempts, guarded, peak) in [
        (1_564_u64, 1_564_u64, 1_u64, false, false),
        (1_563, 1_564, 1, false, false),
        (3_128, 3_128, 2, false, false),
        (3_127, 3_128, 2, false, false),
        (1_844, 1_844, 1, true, false),
        (1_843, 1_844, 1, true, false),
        (3_556, 3_556, 1, false, true),
        (3_555, 3_556, 1, false, true),
    ] {
        assert_core_patch_account_case(maximum, target, attempts, guarded, peak, admission)?;
    }
    let scalar_profile = profile(127)?;
    let mut scalar_charge = PolicySchemaCharge::new(scalar_profile);
    let input = core_patch_input(CompleteCorePolicyPatchInput {
        workflow_type: b"r".to_vec(),
        expected_generation: None,
        patch: PolicyPatch::default(),
    })?;
    let error = prepare_complete_core_policy_patch_caller(
        input,
        scalar_profile,
        admission,
        &mut scalar_charge,
    )
    .err()
    .ok_or("root node refusal missing")?;
    assert_eq!(
        error,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes {
                bytes: 128,
                maximum: 127,
            }
        ),)
    );
    assert_eq!(
        (scalar_charge.g(), scalar_charge.p(), scalar_charge.total()),
        (0, 0, 0)
    );
    Ok(())
}

fn assert_core_patch_scope_limits(
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (tenant_length, different_profile) in [(8_172, false), (8_173, false), (8_173, true)] {
        let input = CompleteCorePolicyPatchRequestInput::new(
            RequestScope::new(
                TenantId::new(vec![b't'; tenant_length])?,
                GroupId::new(b"g".to_vec())?,
                RecoveryId::new(b"r".to_vec())?,
            ),
            RequestId::new(b"q".to_vec())?,
            CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: None,
                patch: PolicyPatch::default(),
            },
        );
        let stamp = input.stamp();
        let requested = if different_profile {
            profile(20_000_001)?
        } else {
            limits
        };
        let before = charge.g();
        let result = prepare_complete_core_policy_patch_caller(input, requested, admission, charge);
        if different_profile {
            assert_eq!(
                result.err().ok_or("profile refusal missing")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::ProfileMismatch,
                ))
            );
            assert_eq!((charge.g(), charge.p()), (before, 0));
        } else if tenant_length == 8_173 {
            assert_eq!(
                result.err().ok_or("scope refusal missing")?,
                CompletePolicyRequestError::ScopeLimit
            );
            assert_eq!((charge.g(), charge.p()), (before + 128, 0));
        } else {
            let caller = result?;
            assert_eq!(
                caller.input().scope().tenant().as_bytes(),
                vec![b't'; 8_172]
            );
            assert_eq!(caller.input().scope().group().as_bytes(), b"g");
            assert_eq!(caller.input().scope().recovery().as_bytes(), b"r");
            assert_eq!(caller.input().request().as_bytes(), b"q");
            assert_eq!(caller.input().stamp(), stamp);
            assert_eq!(caller.input().command().workflow_type.as_slice(), b"r");
            assert_eq!(caller.input().command().expected_generation, None);
            assert_eq!(caller.input().command().patch, PolicyPatch::default());
            assert!(caller.schema_limits() == limits);
            assert_eq!(caller.admission_limits(), admission);
            assert_eq!((charge.g(), charge.p()), (before + 26_077, 0));
        }
    }
    Ok(())
}

fn assert_core_patch_raw_limits(
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (field, success_fee, refusal_fee) in [
        (0, 787_993_u64, 551_u64),
        (1, 788_523, 814),
        (2, 789_041, 1_074),
        (3, 788_523, 948),
    ] {
        for length in [262_144, 262_145] {
            let workflow_type = if field == 0 {
                vec![b'x'; length]
            } else {
                b"r".to_vec()
            };
            let expected_type = if field == 0 {
                vec![b'x'; length]
            } else {
                b"r".to_vec()
            };
            let mut patch = PolicyPatch::default();
            let mut expected = PolicyPatch::default();
            for value in [&mut patch, &mut expected] {
                match field {
                    1 => value.version = FieldPatch::Set(PolicyVersion::Bytes(vec![b'x'; length])),
                    2 => {
                        value.retry = FieldPatch::Set(RetryPatch {
                            exhausted_to: Some(vec![b'x'; length]),
                            ..RetryPatch::default()
                        });
                    }
                    3 => {
                        value.states.insert(vec![b'x'; length], StateChange::Remove);
                    }
                    _ => {}
                }
            }
            let input = core_patch_input(CompleteCorePolicyPatchInput {
                workflow_type,
                expected_generation: None,
                patch,
            })?;
            let stamp = input.stamp();
            let before = charge.g();
            let result =
                prepare_complete_core_policy_patch_caller(input, limits, admission, charge);
            if length == 262_145 {
                assert_eq!(
                    result.err().ok_or("RAW cap refusal missing")?,
                    CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
                );
                assert_eq!((charge.g(), charge.p()), (before + refusal_fee, 0));
            } else {
                let caller = result?;
                assert_eq!(caller.input().scope().tenant().as_bytes(), b"t");
                assert_eq!(caller.input().scope().group().as_bytes(), b"g");
                assert_eq!(caller.input().scope().recovery().as_bytes(), b"r");
                assert_eq!(caller.input().request().as_bytes(), b"q");
                assert_eq!(caller.input().stamp(), stamp);
                assert_eq!(caller.input().command().workflow_type, expected_type);
                assert_eq!(caller.input().command().expected_generation, None);
                assert_eq!(caller.input().command().patch, expected);
                assert!(caller.schema_limits() == limits);
                assert_eq!(caller.admission_limits(), admission);
                assert_eq!((charge.g(), charge.p()), (before + success_fee, 0));
            }
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_profile_refusal(
    maximum: u64,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let before = (charge.g(), charge.p(), charge.total());
    let bad_profile = profile(maximum + 1)?;
    let input = core_patch_input(CompleteCorePolicyPatchInput {
        workflow_type: b"r".to_vec(),
        expected_generation: None,
        patch: PolicyPatch::default(),
    })?;
    let error = prepare_complete_core_policy_patch_caller(input, bad_profile, admission, charge)
        .err()
        .ok_or("profile mismatch missing")?;
    assert_eq!(
        error,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ),)
    );
    assert_eq!((charge.g(), charge.p(), charge.total()), before);
    Ok(())
}

mod apply;
