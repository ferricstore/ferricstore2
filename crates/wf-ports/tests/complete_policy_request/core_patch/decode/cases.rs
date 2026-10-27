//! Independent semantic owners, restoration fees and continuous-account controls.

use sha2::{Digest, Sha256};
use wf_domain::policy::BackoffKind;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchFrameHeader, CompleteCorePolicyPatchInput, CompletePolicyEncodingError,
    CompletePolicyRequestError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    decode_complete_core_policy_patch_frame_input, restore_complete_core_policy_patch_frame_header,
    restore_complete_policy, scan_complete_core_policy_patch_frame,
};
use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicyPatch,
};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, PreparedDigest};
use wf_ports::log::{DecodedCompleteCorePolicyPatchFrame, decode_complete_core_policy_patch_frame};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::super::super::{TestResult, minimal_data, profile};
use super::super::prepare::cases::{
    core_patch_prepare_command, core_patch_prepare_literal_frame, decode_core_patch_literal,
};
use super::super::prepare::literals::FROZEN_CORE_PATCH_FRAMES;

pub(super) const CORE_PATCH_RESTORATION_FEES: [u64; 16] = [
    1_077, 1_213, 1_213, 1_464, 1_852, 1_596, 1_855, 3_705, 3_831, 3_543, 3_827, 4_087, 1_466,
    1_852, 1_855, 1_075,
];

pub(super) fn assert_core_patch_decoded(
    decoded: &DecodedCompleteCorePolicyPatchFrame<'_>,
    expected: &[u8],
    hash: &str,
    command: &CompleteCorePolicyPatchInput,
    ids: [&[u8]; 4],
    stamp: CommittedTime,
) -> TestResult {
    let size = ids.iter().map(|id| id.len()).sum::<usize>();
    let original = &expected[79 + size..];
    let digest: [u8; 32] = decode_core_patch_literal(hash)?
        .try_into()
        .map_err(|_| "wrong independent frame digest length")?;
    assert_eq!(<[u8; 32]>::from(Sha256::digest(expected)), digest);
    assert_eq!(decoded.encoded(), expected);
    assert_eq!(decoded.original_intent(), original);
    assert_eq!(decoded.digest(), PreparedDigest::new(digest));
    assert_eq!(
        decoded.input().command().workflow_type,
        command.workflow_type
    );
    assert_eq!(
        decoded.input().command().expected_generation,
        command.expected_generation
    );
    assert_eq!(decoded.input().command().patch, command.patch);
    assert_eq!(decoded.input().scope().tenant().as_bytes(), ids[0]);
    assert_eq!(decoded.input().scope().group().as_bytes(), ids[1]);
    assert_eq!(decoded.input().scope().recovery().as_bytes(), ids[2]);
    assert_eq!(decoded.input().request().as_bytes(), ids[3]);
    assert_eq!(decoded.input().stamp(), stamp);
    let mut scope = b"FWSC\0\x01".to_vec();
    for id in &ids[..3] {
        scope.extend_from_slice(&u32::try_from(id.len())?.to_be_bytes());
        scope.extend_from_slice(id);
    }
    assert_eq!(decoded.isolation_scope(), scope.as_slice());
    assert_decoded_headers(
        decoded,
        expected,
        (original, size),
        digest,
        ids,
        stamp,
        &scope,
    );
    Ok(())
}

fn assert_core_patch_decoded_default(
    decoded: &DecodedCompleteCorePolicyPatchFrame<'_>,
    encoded: &[u8],
) -> TestResult {
    assert_core_patch_decoded(
        decoded,
        encoded,
        FROZEN_CORE_PATCH_FRAMES[0].1,
        &core_patch_prepare_command(0)?,
        [b"t", b"g", b"r", b"q"],
        CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?,
    )
}

fn core_patch_decode_header<'a>(
    encoded: &'a [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompleteCorePolicyPatchFrameHeader<'a>> {
    let scanned = scan_complete_core_policy_patch_frame(encoded, limits, admission, charge)?;
    let recomputed = Sha256::digest(scanned.original_intent()).into();
    Ok(restore_complete_core_policy_patch_frame_header(
        scanned, recomputed, limits, admission, charge,
    )?)
}

pub(super) fn assert_core_patch_decode_domain_owners(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let admission = AdmissionLimits::default();
    let expected = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    for case in 0..3 {
        let before = charge.g();
        let header = core_patch_decode_header(&expected, limits, admission, charge)?;
        let addresses = [
            header.scope().tenant().as_bytes().as_ptr(),
            header.scope().group().as_bytes().as_ptr(),
            header.scope().recovery().as_bytes().as_ptr(),
            header.request().as_bytes().as_ptr(),
        ];
        assert_eq!((charge.g(), charge.p()), (before + 4_423, 0));
        let before = charge.g();
        let selected = if case == 0 {
            profile(20_000_001)?
        } else {
            limits
        };
        let cap = if case == 1 {
            AdmissionLimits::new(1_048_576, 134, 1_000)?
        } else {
            admission
        };
        let result = decode_complete_core_policy_patch_frame_input(header, selected, cap, charge);
        if case < 2 {
            let error = if case == 0 {
                CompletePolicyEncodingError::Schema(PolicySchemaError::ProfileMismatch)
            } else {
                CompletePolicyEncodingError::ByteLimit
            };
            assert_eq!(
                result
                    .err()
                    .ok_or("expected consuming-header precheck refusal")?,
                CompletePolicyRequestError::Encoding(error)
            );
            assert_eq!((charge.g(), charge.p()), (before, 0));
        } else {
            let input = result?;
            assert_eq!(input.command().workflow_type, b"r");
            assert_eq!(input.command().expected_generation, None);
            assert_eq!(input.command().patch, PolicyPatch::default());
            assert_eq!(input.scope().tenant().as_bytes(), b"t");
            assert_eq!(input.scope().group().as_bytes(), b"g");
            assert_eq!(input.scope().recovery().as_bytes(), b"r");
            assert_eq!(input.request().as_bytes(), b"q");
            assert_eq!(
                input.stamp(),
                CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?
            );
            assert_eq!((charge.g(), charge.p()), (before + 1_077, 0));
            assert_eq!(
                [
                    input.scope().tenant().as_bytes().as_ptr(),
                    input.scope().group().as_bytes().as_ptr(),
                    input.scope().recovery().as_bytes().as_ptr(),
                    input.request().as_bytes().as_ptr()
                ],
                addresses
            );
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_decode_refusal(
    encoded: &[u8],
    expected: CompletePolicyRequestError,
    fee: u64,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let unchanged = encoded.to_vec();
    let before = (charge.g(), charge.p());
    let result = decode_complete_core_policy_patch_frame(
        encoded,
        limits,
        AdmissionLimits::default(),
        charge,
    );
    assert_eq!(
        result.err().ok_or("expected original decoder refusal")?,
        expected
    );
    assert_eq!(encoded, unchanged.as_slice());
    assert_eq!((charge.g(), charge.p()), (before.0 + fee, before.1));
    Ok(())
}

pub(super) fn assert_core_patch_decode_account(
    maximum: u64,
    target: u64,
    attempts: u64,
    peak: bool,
) -> TestResult {
    let limits = profile(maximum)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let mut expected_data = minimal_data();
    expected_data.indexed_attributes = Some(vec![b"a".to_vec()]);
    let held = if peak {
        let mut data = minimal_data();
        data.indexed_attributes = Some(vec![b"a".to_vec()]);
        Some(restore_complete_policy(1, data, limits, &mut charge)?)
    } else {
        None
    };
    let (setup, scratch) = if peak { (1_928, 64) } else { (0, 0) };
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (setup, scratch, setup + scratch)
    );
    let encoded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let mut outputs = Vec::new();
    for index in 0..attempts {
        let result = decode_complete_core_policy_patch_frame(
            &encoded,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        );
        let refused = index + 1 == attempts && maximum < target;
        if refused {
            assert_eq!(
                result
                    .err()
                    .ok_or("expected final owning-map work refusal")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::WorkBytes {
                        bytes: target,
                        maximum
                    }
                ))
            );
        } else {
            let decoded = result?;
            assert_core_patch_decoded_default(&decoded, &encoded)?;
            outputs.push(decoded);
        }
        for output in &outputs {
            assert_core_patch_decoded_default(output, &encoded)?;
        }
        assert_eq!(u64::try_from(outputs.len())?, index + u64::from(!refused));
        let g = setup + (index + 1) * 5_500 - if refused { 128 } else { 0 };
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (g, scratch, g + scratch)
        );
        if let Some(held) = &held {
            assert_eq!(held.generation(), 1);
            assert!(held.data() == &expected_data);
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_decode_presence(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    assert_core_patch_decode_top_fields(limits, charge)?;
    assert_core_patch_decode_state_fields(limits, charge)?;
    assert_core_patch_decode_backoff(limits, charge)?;
    Ok(())
}

fn assert_core_patch_decode_top_fields(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let literal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (field, empty, set_fee) in [
        (0, &[1, 0, 0, 0, 0, 0][..], 6_160),
        (1, &[1, 0, 0, 0, 0, 0, 0, 0, 0][..], 5_788),
        (2, &[1, 0, 0, 0][..], 6_152),
        (3, &[1, 0, 0][..], 6_148),
    ] {
        for action in 0..3 {
            let mut command = core_patch_prepare_command(0)?;
            match (field, action) {
                (0, 1) => command.patch.version = FieldPatch::Clear,
                (0, 2) => command.patch.version = FieldPatch::Set(PolicyVersion::Bytes(Vec::new())),
                (1, 1) => command.patch.max_active_ms = FieldPatch::Clear,
                (1, 2) => command.patch.max_active_ms = FieldPatch::Set(0),
                (2, 1) => command.patch.retry = FieldPatch::Clear,
                (2, 2) => command.patch.retry = FieldPatch::Set(RetryPatch::default()),
                (3, 1) => command.patch.retention = FieldPatch::Clear,
                (3, 2) => command.patch.retention = FieldPatch::Set(RetentionOverride::default()),
                _ => {}
            }
            let mut intent = literal[83..].to_vec();
            let replacement = match action {
                0 => &[0][..],
                1 => &[2][..],
                _ => empty,
            };
            drop(intent.splice(44 + field..45 + field, replacement.iter().copied()));
            let encoded =
                core_patch_prepare_literal_frame(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let hash = Sha256::digest(&encoded)
                .iter()
                .flat_map(|byte| [byte >> 4, byte & 15])
                .map(|digit| char::from(b"0123456789abcdef"[usize::from(digit)]))
                .collect::<String>();
            let before = charge.g();
            let decoded = decode_complete_core_policy_patch_frame(
                &encoded,
                limits,
                AdmissionLimits::default(),
                charge,
            )?;
            assert_core_patch_decoded(
                &decoded,
                &encoded,
                &hash,
                &command,
                [b"t", b"g", b"r", b"q"],
                stamp,
            )?;
            assert_eq!(
                (charge.g(), charge.p()),
                (before + if action == 2 { set_fee } else { 5_500 }, 0)
            );
        }
    }
    Ok(())
}

fn assert_core_patch_decode_state_fields(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let literal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[6].0)?;
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (field, empty, set_fee) in [
        (0, &[1, 0][..], 6_949),
        (1, &[1, 0, 0, 0][..], 7_341),
        (2, &[1, 0, 0][..], 7_337),
    ] {
        for action in 0..3 {
            let mut state = StatePolicyPatch::default();
            match (field, action) {
                (0, 1) => state.mode = FieldPatch::Clear,
                (0, 2) => state.mode = FieldPatch::Set(StateMode::Parallel),
                (1, 1) => state.retry = FieldPatch::Clear,
                (1, 2) => state.retry = FieldPatch::Set(RetryPatch::default()),
                (2, 1) => state.retention = FieldPatch::Clear,
                (2, 2) => state.retention = FieldPatch::Set(RetentionOverride::default()),
                _ => {}
            }
            let mut command = core_patch_prepare_command(0)?;
            command
                .patch
                .states
                .insert(b"a".to_vec(), StateChange::Patch(state));
            let mut intent = literal[83..].to_vec();
            let replacement = match action {
                0 => &[0][..],
                1 => &[2][..],
                _ => empty,
            };
            drop(intent.splice(58 + field..59 + field, replacement.iter().copied()));
            let encoded =
                core_patch_prepare_literal_frame(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let hash = Sha256::digest(&encoded)
                .iter()
                .flat_map(|byte| [byte >> 4, byte & 15])
                .map(|digit| char::from(b"0123456789abcdef"[usize::from(digit)]))
                .collect::<String>();
            let before = charge.g();
            let decoded = decode_complete_core_policy_patch_frame(
                &encoded,
                limits,
                AdmissionLimits::default(),
                charge,
            )?;
            assert_core_patch_decoded(
                &decoded,
                &encoded,
                &hash,
                &command,
                [b"t", b"g", b"r", b"q"],
                stamp,
            )?;
            assert_eq!(
                (charge.g(), charge.p()),
                (before + if action == 2 { set_fee } else { 6_689 }, 0)
            );
        }
    }
    Ok(())
}

fn assert_core_patch_decode_backoff(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let literal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
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
        let mut intent = literal[83..127].to_vec();
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
        let encoded = core_patch_prepare_literal_frame(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let hash = Sha256::digest(&encoded)
            .iter()
            .flat_map(|byte| [byte >> 4, byte & 15])
            .map(|digit| char::from(b"0123456789abcdef"[usize::from(digit)]))
            .collect::<String>();
        let before = charge.g();
        let decoded = decode_complete_core_policy_patch_frame(
            &encoded,
            limits,
            AdmissionLimits::default(),
            charge,
        )?;
        assert_core_patch_decoded(
            &decoded,
            &encoded,
            &hash,
            &command,
            [b"t", b"g", b"r", b"q"],
            stamp,
        )?;
        assert_eq!((charge.g(), charge.p()), (before + 7_068, 0));
    }
    Ok(())
}

fn assert_decoded_headers(
    decoded: &DecodedCompleteCorePolicyPatchFrame<'_>,
    expected: &[u8],
    (original, size): (&[u8], usize),
    digest: [u8; 32],
    ids: [&[u8]; 4],
    stamp: CommittedTime,
    scope: &[u8],
) {
    for log in [
        LogIdentity::new(1, 2, AppliedPosition::new(3)),
        LogIdentity::new(9, 8, AppliedPosition::new(7)),
    ] {
        let header = decoded.header(log);
        assert_eq!(header.format_version, 1);
        assert_eq!(header.scope.tenant().as_bytes(), ids[0]);
        assert_eq!(header.scope.group().as_bytes(), ids[1]);
        assert_eq!(header.scope.recovery().as_bytes(), ids[2]);
        assert_eq!(header.request.as_bytes(), ids[3]);
        assert_eq!(header.stamp, stamp);
        assert_eq!(header.intent.version(), 2);
        assert_eq!(
            header.intent.digest(),
            &<[u8; 32]>::from(Sha256::digest(original))
        );
        assert_eq!(
            header.intent.digest().as_slice(),
            &expected[25 + size..57 + size]
        );
        assert_eq!(
            header.identity,
            EntryIdentity::new(log, PreparedDigest::new(digest))
        );
        assert_eq!(header.encoded_entry, expected);
        assert_eq!(header.encoded_isolation_scope, scope);
    }
}
