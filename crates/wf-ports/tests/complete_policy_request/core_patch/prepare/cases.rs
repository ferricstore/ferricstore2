//! Independent frame literals, owned inputs and continuous-account controls.

use sha2::{Digest, Sha256};
use wf_domain::policy::BackoffKind;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompleteCorePolicyPatchRequestInput, CompletePolicyEncodingError,
    CompletePolicyRequestError, CompletePolicyValue, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, prepare_complete_core_policy_patch, prepare_complete_policy_value_v2,
    restore_complete_policy,
};
use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicyPatch,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{PreparedBytes, prepare_complete_core_policy_patch_frame};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, HlcTimestamp};

use super::super::super::{TestResult, minimal_data, profile};

use super::literals::FROZEN_CORE_PATCH_FRAMES;

pub(in crate::core_patch) fn decode_core_patch_literal(hex: &str) -> TestResult<Vec<u8>> {
    let mut bytes = Vec::new();
    for pair in hex.as_bytes().as_chunks::<2>().0 {
        bytes.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
    }
    assert_eq!(bytes.len() * 2, hex.len());
    Ok(bytes)
}

pub(super) fn core_patch_prepare_input(
    command: CompleteCorePolicyPatchInput,
    ids: [&[u8]; 4],
    stamp: CommittedTime,
) -> TestResult<CompleteCorePolicyPatchRequestInput> {
    Ok(CompleteCorePolicyPatchRequestInput::new(
        RequestScope::new(
            TenantId::new(ids[0].to_vec())?,
            GroupId::new(ids[1].to_vec())?,
            RecoveryId::new(ids[2].to_vec())?,
        ),
        RequestId::new(ids[3].to_vec())?,
        stamp,
        command,
    ))
}

pub(in crate::core_patch) fn core_patch_prepare_command(
    case: usize,
) -> TestResult<CompleteCorePolicyPatchInput> {
    let mut command = CompleteCorePolicyPatchInput {
        workflow_type: b"r".to_vec(),
        expected_generation: None,
        patch: PolicyPatch::default(),
    };
    match case {
        0 => {}
        1 => command.expected_generation = Some(0),
        2 => command.expected_generation = Some(u64::MAX),
        3 => command.patch.retry = FieldPatch::Set(RetryPatch::default()),
        4 => {
            command.patch.retry = FieldPatch::Set(RetryPatch {
                backoff: Some(BackoffPatch::default()),
                ..RetryPatch::default()
            });
        }
        5 => {
            command
                .patch
                .states
                .insert(b"a".to_vec(), StateChange::Remove);
        }
        6 => {
            command.patch.states.insert(
                b"a".to_vec(),
                StateChange::Patch(StatePolicyPatch::default()),
            );
        }
        7 => {
            command.patch.states.insert(
                b"queued".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    retry: FieldPatch::Set(RetryPatch {
                        exhausted_to: Some(b"dead".to_vec()),
                        ..RetryPatch::default()
                    }),
                    ..StatePolicyPatch::default()
                }),
            );
            command.patch.states.insert(
                b"review".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Fifo),
                    ..StatePolicyPatch::default()
                }),
            );
        }
        8 | 10 | 11 => core_patch_prepare_widths(&mut command, case),
        9 => {
            for key in [255, b'b', b'a', 0] {
                command.patch.states.insert(vec![key], StateChange::Remove);
            }
        }
        12 => command.patch.version = FieldPatch::Set(PolicyVersion::Bytes(Vec::new())),
        13 => {
            command.patch.retry = FieldPatch::Set(RetryPatch {
                exhausted_to: Some(Vec::new()),
                ..RetryPatch::default()
            });
        }
        14 => {
            command.patch.states.insert(
                b"a".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Clear,
                    ..StatePolicyPatch::default()
                }),
            );
        }
        15 => command.workflow_type = Vec::new(),
        _ => return Err("unknown independent producer case".into()),
    }
    Ok(command)
}

fn core_patch_prepare_widths(command: &mut CompleteCorePolicyPatchInput, case: usize) {
    let maximum = case != 10;
    let scalar = if maximum { u64::MAX } else { 0 };
    command.patch.version = FieldPatch::Set(if case == 11 {
        PolicyVersion::Bytes(vec![0, 255])
    } else {
        PolicyVersion::Number(scalar)
    });
    command.patch.max_active_ms = FieldPatch::Set(scalar);
    command.patch.retry = FieldPatch::Set(RetryPatch {
        max_retries: Some(if maximum { u16::MAX } else { 0 }),
        backoff: Some(BackoffPatch {
            kind: Some(if maximum {
                BackoffKind::Exponential
            } else {
                BackoffKind::None
            }),
            base_ms: Some(scalar),
            max_ms: Some(scalar),
            jitter_pct: Some(if maximum { 255 } else { 0 }),
        }),
        exhausted_to: Some(if maximum { vec![0, 255] } else { Vec::new() }),
    });
    command.patch.retention = FieldPatch::Set(wf_domain::retention::RetentionOverride {
        ttl_ms: Some(scalar),
        history_max_events: Some(if maximum { u32::MAX } else { 0 }),
    });
}

pub(in crate::core_patch) fn core_patch_prepare_literal_frame(
    intent: &[u8],
    ids: [&[u8]; 4],
    stamp: CommittedTime,
) -> TestResult<Vec<u8>> {
    let mut expected = b"FWCP\0\x01\x05".to_vec();
    for id in ids {
        expected.extend_from_slice(&u32::try_from(id.len())?.to_be_bytes());
        expected.extend_from_slice(id);
    }
    expected.extend_from_slice(b"\0\x02");
    expected.extend_from_slice(&<[u8; 32]>::from(Sha256::digest(intent)));
    expected.extend_from_slice(&stamp.hlc().physical().get().to_be_bytes());
    expected.extend_from_slice(&stamp.hlc().logical().to_be_bytes());
    expected.extend_from_slice(&stamp.wall().get().to_be_bytes());
    expected.extend_from_slice(&u32::try_from(intent.len())?.to_be_bytes());
    expected.extend_from_slice(intent);
    Ok(expected)
}

pub(super) fn assert_core_patch_prepared_literal(value: &PreparedBytes, expected: &[u8]) {
    assert_eq!(value.as_bytes(), expected);
    assert_eq!(
        value.digest(),
        wf_domain::workflow::control::PreparedDigest::new(Sha256::digest(expected).into())
    );
}

pub(super) fn assert_core_patch_prepare_raw_caps(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    let minimal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    for (field, success_fee, refusal_fee) in [
        (0, 1_577_342, 551),
        (1, 1_578_146, 814),
        (2, 1_578_926, 1_074),
        (3, 1_578_146, 948),
    ] {
        for length in [262_144, 262_145] {
            let mut intent = minimal[83..].to_vec();
            let mut payload = u32::try_from(length)?.to_be_bytes().to_vec();
            payload.extend(std::iter::repeat_n(b'x', length));
            let prefix = if field == 1 {
                &[1, 0][..]
            } else {
                &[1, 0, 0, 1][..]
            };
            let offset = if field == 1 { 44 } else { 46 };
            match field {
                0 => {
                    drop(intent.splice(39..44, payload));
                }
                1 | 2 => {
                    drop(payload.splice(0..0, prefix.iter().copied()));
                    drop(intent.splice(offset..=offset, payload));
                }
                3 => {
                    intent[48..52].copy_from_slice(&1_u32.to_be_bytes());
                    intent.extend_from_slice(&payload);
                    intent.push(0);
                }
                _ => return Err("unknown independent RAW literal field".into()),
            }
            let expected_frame =
                core_patch_prepare_literal_frame(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let mut command = core_patch_prepare_command(0)?;
            match field {
                0 => command.workflow_type = vec![b'x'; length],
                1 => {
                    command.patch.version =
                        FieldPatch::Set(PolicyVersion::Bytes(vec![b'x'; length]));
                }
                2 => {
                    command.patch.retry = FieldPatch::Set(RetryPatch {
                        exhausted_to: Some(vec![b'x'; length]),
                        ..RetryPatch::default()
                    });
                }
                3 => {
                    command
                        .patch
                        .states
                        .insert(vec![b'x'; length], StateChange::Remove);
                }
                _ => return Err("unknown RAW producer field".into()),
            }
            let input = core_patch_prepare_input(command, [b"t", b"g", b"r", b"q"], stamp)?;
            let before = charge.g();
            let result = prepare_complete_core_policy_patch_frame(
                input,
                limits,
                AdmissionLimits::default(),
                charge,
            );
            if length == 262_145 {
                assert_eq!(
                    result.err().ok_or("expected original RAW cap refusal")?,
                    CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
                );
                assert_eq!(charge.g(), before + refusal_fee);
            } else {
                let prepared = result?;
                let expected = match field {
                    0 => 262_278,
                    1 | 3 => 262_284,
                    2 => 262_286,
                    _ => return Err("unknown independent frame length".into()),
                };
                assert_core_patch_prepared_literal(&prepared, &expected_frame);
                assert_eq!(prepared.as_bytes().len(), expected);
                assert_eq!(charge.g(), before + success_fee);
            }
            assert_eq!(charge.p(), 0);
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_prepare_global_bytes(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    let minimal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    for final_length in [262_072, 262_073] {
        let mut intent = minimal[83..].to_vec();
        intent[48..52].copy_from_slice(&4_u32.to_be_bytes());
        let mut command = core_patch_prepare_command(0)?;
        for first in 0..4 {
            let length = if first == 3 { final_length } else { 262_144 };
            let mut key = vec![0; length];
            key[0] = first;
            intent.extend_from_slice(&u32::try_from(length)?.to_be_bytes());
            intent.extend_from_slice(&key);
            intent.push(0);
            command.patch.states.insert(key, StateChange::Remove);
        }
        let expected_frame =
            core_patch_prepare_literal_frame(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let input = core_patch_prepare_input(command, [b"t", b"g", b"r", b"q"], stamp)?;
        let before = charge.g();
        let result = prepare_complete_core_policy_patch_frame(
            input,
            limits,
            AdmissionLimits::default(),
            charge,
        );
        if final_length == 262_073 {
            assert_eq!(
                result.err().ok_or("expected global intent cap refusal")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
            );
            assert_eq!((charge.g(), charge.p()), (before + 1_050_240, 0));
        } else {
            let prepared = result?;
            assert_core_patch_prepared_literal(&prepared, &expected_frame);
            assert_eq!(prepared.as_bytes().len(), 1_048_659);
            assert_eq!(&prepared.as_bytes()[79..83], &1_048_576_u32.to_be_bytes());
            assert_eq!((charge.g(), charge.p()), (before + 6_298_700, 0));
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_prepare_scope_limits(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for tenant_length in [8_172, 8_173] {
        let tenant = vec![b't'; tenant_length];
        let input = core_patch_prepare_input(
            core_patch_prepare_command(0)?,
            [&tenant, b"g", b"r", b"q"],
            stamp,
        )?;
        let before = charge.g();
        let result = prepare_complete_core_policy_patch_frame(
            input,
            limits,
            AdmissionLimits::default(),
            charge,
        );
        if tenant_length == 8_173 {
            assert_eq!(
                result.err().ok_or("expected aggregate scope refusal")?,
                CompletePolicyRequestError::ScopeLimit
            );
            assert_eq!((charge.g(), charge.p()), (before + 128, 0));
        } else {
            let prepared = result?;
            let mut intent = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
            intent.extend_from_slice(&8_172_u32.to_be_bytes());
            intent.extend_from_slice(&tenant);
            intent.extend_from_slice(b"\0\0\0\0\x01r\0\0\0\0\0\0\0\0");
            let expected =
                core_patch_prepare_literal_frame(&intent, [&tenant, b"g", b"r", b"q"], stamp)?;
            assert_core_patch_prepared_literal(&prepared, &expected);
            let digest = decode_core_patch_literal(
                "b310d2477236a40842e006125aff1e9d3ee305463e9c2223cef0b51ffe03548b",
            )?;
            assert_eq!(
                &<[u8; 32]>::from(Sha256::digest(&expected)),
                digest.as_slice()
            );
            assert_eq!((intent.len(), expected.len()), (8_223, 16_477));
            assert_eq!((charge.g(), charge.p()), (before + 86_194, 0));
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_prepare_account(
    maximum: u64,
    target: u64,
    attempts: u64,
    held_peak: bool,
) -> TestResult {
    let limits = profile(maximum)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let mut expected_data = minimal_data();
    expected_data.indexed_attributes = Some(vec![b"a".to_vec()]);
    let held = if held_peak {
        let mut data = minimal_data();
        data.indexed_attributes = Some(vec![b"a".to_vec()]);
        Some(restore_complete_policy(1, data, limits, &mut charge)?)
    } else {
        None
    };
    let (setup, peak) = if held_peak { (1_928, 64) } else { (0, 0) };
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (setup, peak, setup + peak)
    );
    let expected = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    let mut outputs = Vec::new();
    for index in 0..attempts {
        let input = core_patch_prepare_input(
            core_patch_prepare_command(0)?,
            [b"t", b"g", b"r", b"q"],
            stamp,
        )?;
        let result =
            prepare_complete_core_policy_patch_frame(input, limits, admission, &mut charge);
        let refused = index + 1 == attempts && maximum < target;
        if refused {
            assert_eq!(
                result
                    .err()
                    .ok_or("expected final frame SHA debit refusal")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::WorkBytes {
                        bytes: target,
                        maximum
                    }
                ))
            );
        } else {
            let prepared = result?;
            assert_core_patch_prepared_literal(&prepared, &expected);
            outputs.push(prepared);
        }
        for output in &outputs {
            assert_core_patch_prepared_literal(output, &expected);
        }
        assert_eq!(u64::try_from(outputs.len())?, index + u64::from(!refused));
        let g = setup + (index + 1) * 4_484 - if refused { 263 } else { 0 };
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (g, peak, g + peak)
        );
        if let Some(held) = &held {
            assert!(held.data() == &expected_data);
            assert_eq!(held.generation(), 1);
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_prepare_backoff_kinds(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let minimal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (tag, kind) in [
        BackoffKind::None,
        BackoffKind::Fixed,
        BackoffKind::Linear,
        BackoffKind::Exponential,
    ]
    .into_iter()
    .enumerate()
    {
        let mut command = core_patch_prepare_command(0)?;
        command.patch.retry = FieldPatch::Set(RetryPatch {
            backoff: Some(BackoffPatch {
                kind: Some(kind),
                ..BackoffPatch::default()
            }),
            ..RetryPatch::default()
        });
        let mut intent = minimal[83..127].to_vec();
        intent.extend_from_slice(&[
            0,
            0,
            1,
            0,
            1,
            1,
            u8::try_from(tag)?,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ]);
        let expected = core_patch_prepare_literal_frame(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let input = core_patch_prepare_input(command, [b"t", b"g", b"r", b"q"], stamp)?;
        let before = charge.g();
        let prepared = prepare_complete_core_policy_patch_frame(
            input,
            limits,
            AdmissionLimits::default(),
            charge,
        )?;
        assert_core_patch_prepared_literal(&prepared, &expected);
        assert_eq!(intent.len(), 60);
        assert_eq!((charge.g(), charge.p()), (before + 6_452, 0));
    }
    Ok(())
}

pub(in crate::core_patch) fn core_patch_prepare_equal_current(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompletePolicyValue> {
    let original_value = decode_core_patch_literal(
        "46575056000200000000000000010000001e465750530002000000017200010000000000000007000000000000000000",
    )?;
    let merged_value = decode_core_patch_literal(
        "46575056000200000000000000020000001e465750530002000000017200010000000000000007000000000000000000",
    )?;
    assert_eq!((original_value.len(), merged_value.len()), (48, 48));
    assert_eq!(&original_value[18..], &merged_value[18..]);
    assert_eq!(original_value[18..].len(), 30);
    let mut data = minimal_data();
    data.max_active_ms = Some(7);
    let mut expected_data = minimal_data();
    expected_data.max_active_ms = Some(7);
    let candidate = restore_complete_policy(1, data, limits, charge)?;
    let current = prepare_complete_policy_value_v2(candidate, limits, charge)?;
    assert_eq!(current.encoded(), original_value.as_slice());
    let unchanged = prepare_complete_core_policy_patch(
        Some(&current),
        0,
        core_patch_prepare_command(0)?,
        1_000_000,
        limits,
        charge,
    )?;
    let mut same = core_patch_prepare_command(0)?;
    same.patch.max_active_ms = FieldPatch::Set(7);
    let set_same =
        prepare_complete_core_policy_patch(Some(&current), 0, same, 1_000_000, limits, charge)?;
    assert!(unchanged.data() == &expected_data);
    assert!(set_same.data() == &expected_data);
    assert_eq!(unchanged.generation(), 2);
    assert_eq!(set_same.generation(), 2);
    assert_eq!(unchanged.encoded(), merged_value.as_slice());
    assert_eq!(set_same.encoded(), merged_value.as_slice());
    assert!(current.data() == &expected_data);
    assert_eq!(current.generation(), 1);
    assert_eq!(current.encoded(), original_value.as_slice());
    Ok(current)
}

pub(super) fn assert_core_patch_prepare_state_presence(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let original = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[6].0)?;
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (field, empty_payload, extra_nodes) in [
        (0, &[1, 0][..], 1_u64),
        (1, &[1, 0, 0, 0][..], 2),
        (2, &[1, 0, 0][..], 2),
    ] {
        for action in 0..3 {
            let mut state = StatePolicyPatch::default();
            match (field, action) {
                (0, 1) => state.mode = FieldPatch::Clear,
                (0, 2) => state.mode = FieldPatch::Set(StateMode::Parallel),
                (1, 1) => state.retry = FieldPatch::Clear,
                (1, 2) => state.retry = FieldPatch::Set(RetryPatch::default()),
                (2, 1) => state.retention = FieldPatch::Clear,
                (2, 2) => {
                    state.retention =
                        FieldPatch::Set(wf_domain::retention::RetentionOverride::default());
                }
                _ => {}
            }
            let mut command = core_patch_prepare_command(0)?;
            command
                .patch
                .states
                .insert(b"a".to_vec(), StateChange::Patch(state));
            let mut intent = original[83..].to_vec();
            let replacement = match action {
                0 => &[0][..],
                1 => &[2][..],
                _ => empty_payload,
            };
            drop(intent.splice(58 + field..59 + field, replacement.iter().copied()));
            let expected =
                core_patch_prepare_literal_frame(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let nodes = 8 + if action == 2 { extra_nodes } else { 0 };
            let fee = 384 * nodes + 6 * u64::try_from(intent.len())? + 2_252;
            let input = core_patch_prepare_input(command, [b"t", b"g", b"r", b"q"], stamp)?;
            let before = charge.g();
            let prepared = prepare_complete_core_policy_patch_frame(
                input,
                limits,
                AdmissionLimits::default(),
                charge,
            )?;
            assert_core_patch_prepared_literal(&prepared, &expected);
            assert_eq!((charge.g(), charge.p()), (before + fee, 0));
        }
    }
    Ok(())
}
