//! Whole-byte RAW/global/scope fixtures and pre-materialization wire refusals.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompletePolicyEncodingError, CompletePolicyRequestError,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};
use wf_domain::policy::definition::{FieldPatch, PolicyVersion, RetryPatch, StateChange};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_ports::log::decode_complete_core_policy_patch_frame;
use wf_types::version::{CommittedTime, HlcTimestamp};

use super::super::super::{TestResult, profile};
use super::super::prepare::cases::{
    core_patch_prepare_command, core_patch_prepare_literal_frame, decode_core_patch_literal,
};
use super::super::prepare::literals::FROZEN_CORE_PATCH_FRAMES;
use super::cases::{assert_core_patch_decode_refusal, assert_core_patch_decoded};

pub(super) fn assert_core_patch_decode_boundaries(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    assert_core_patch_decode_wire(limits, charge)?;
    assert_core_patch_decode_raw_caps(limits, charge)?;
    assert_core_patch_decode_global_bytes(limits, charge)?;
    assert_core_patch_decode_scope_limits(limits, charge)?;
    Ok(())
}

fn assert_core_patch_decode_wire(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let literal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let wire = |error| CompletePolicyRequestError::Encoding(error);
    for (offset, byte, error, fee) in [
        (0, b'X', CompletePolicyEncodingError::Malformed, 132),
        (5, 2, CompletePolicyEncodingError::UnsupportedVersion, 134),
        (6, 6, CompletePolicyEncodingError::UnsupportedTag, 135),
        (28, 3, CompletePolicyEncodingError::UnsupportedVersion, 669),
        (
            121,
            2,
            CompletePolicyEncodingError::InvalidOptionality,
            1_146,
        ),
        (127, 3, CompletePolicyEncodingError::UnsupportedTag, 1_408),
        (120, b'u', CompletePolicyEncodingError::Malformed, 1_673),
    ] {
        let mut encoded = literal.clone();
        encoded[offset] = byte;
        encoded[29] ^= 1;
        assert_core_patch_decode_refusal(&encoded, wire(error), fee, limits, charge)?;
    }
    let mut late = literal.clone();
    late.push(0);
    assert_core_patch_decode_refusal(
        &late,
        wire(CompletePolicyEncodingError::TrailingBytes),
        851,
        limits,
        charge,
    )?;
    late[79..83].copy_from_slice(&53_u32.to_be_bytes());
    assert_core_patch_decode_refusal(
        &late,
        wire(CompletePolicyEncodingError::TrailingBytes),
        1_543,
        limits,
        charge,
    )?;
    let mut bad_digest = literal.clone();
    bad_digest[29] ^= 1;
    assert_core_patch_decode_refusal(
        &bad_digest,
        CompletePolicyRequestError::IntentMismatch,
        2_045,
        limits,
        charge,
    )?;
    for keys in [*b"aa", *b"ba"] {
        let mut intent = literal[83..131].to_vec();
        intent.extend_from_slice(&2_u32.to_be_bytes());
        for key in keys {
            intent.extend_from_slice(&[0, 0, 0, 1, key, 0]);
        }
        let encoded = core_patch_prepare_literal_frame(
            &intent,
            [b"t", b"g", b"r", b"q"],
            CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?,
        )?;
        assert_core_patch_decode_refusal(
            &encoded,
            wire(CompletePolicyEncodingError::Malformed),
            2_196,
            limits,
            charge,
        )?;
    }
    Ok(())
}

fn core_patch_decode_raw_fixture(
    field: usize,
    length: usize,
) -> TestResult<(Vec<u8>, CompleteCorePolicyPatchInput)> {
    let literal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let mut intent = literal[83..].to_vec();
    let mut payload = u32::try_from(length)?.to_be_bytes().to_vec();
    payload.extend(std::iter::repeat_n(b'x', length));
    let mut command = core_patch_prepare_command(0)?;
    match field {
        0 => {
            drop(intent.splice(39..44, payload));
            command.workflow_type = vec![b'x'; length];
        }
        1 => {
            drop(payload.splice(0..0, [1, 0]));
            drop(intent.splice(44..45, payload));
            command.patch.version = FieldPatch::Set(PolicyVersion::Bytes(vec![b'x'; length]));
        }
        2 => {
            drop(payload.splice(0..0, [1, 0, 0, 1]));
            drop(intent.splice(46..47, payload));
            command.patch.retry = FieldPatch::Set(RetryPatch {
                exhausted_to: Some(vec![b'x'; length]),
                ..RetryPatch::default()
            });
        }
        3 => {
            intent[48..52].copy_from_slice(&1_u32.to_be_bytes());
            intent.extend_from_slice(&payload);
            intent.push(0);
            command
                .patch
                .states
                .insert(vec![b'x'; length], StateChange::Remove);
        }
        _ => return Err("unknown independent decoder RAW field".into()),
    }
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    let encoded = core_patch_prepare_literal_frame(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
    Ok((encoded, command))
}

fn assert_core_patch_decode_raw_caps(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (field, success_fee, refusal_fee, hash) in [
        (
            0,
            1_316_215,
            1_406,
            "2888dde3aa22ac6e9451cd76f9bce3b19050cfd3a1d7ea1ccece6b6172350095",
        ),
        (
            1,
            1_316_880,
            1_669,
            "b5f1fc080e75c8193f8bdd15e669925abdf7bad8eb69ad16509bc9da0de85da4",
        ),
        (
            2,
            1_317_528,
            1_929,
            "eec4eef9d51252e0119f205eb65904e7e1f224118a872c2ea8b7a1edf8d17492",
        ),
        (
            3,
            1_317_008,
            1_803,
            "fd02e95cead010d9ce9585a5e99bba4c66a550a884f91ff05c1ad380178aeb7d",
        ),
    ] {
        for length in [262_144, 262_145] {
            let (encoded, command) = core_patch_decode_raw_fixture(field, length)?;
            let before = charge.g();
            if length == 262_145 {
                assert_core_patch_decode_refusal(
                    &encoded,
                    CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit),
                    refusal_fee,
                    limits,
                    charge,
                )?;
            } else {
                let decoded = decode_complete_core_policy_patch_frame(
                    &encoded,
                    limits,
                    AdmissionLimits::default(),
                    charge,
                )?;
                assert_core_patch_decoded(
                    &decoded,
                    &encoded,
                    hash,
                    &command,
                    [b"t", b"g", b"r", b"q"],
                    stamp,
                )?;
                assert_eq!((charge.g(), charge.p()), (before + success_fee, 0));
            }
        }
    }
    assert_core_patch_decode_large_copy_refusal()?;
    Ok(())
}

fn assert_core_patch_decode_large_copy_refusal() -> TestResult {
    let (encoded, _) = core_patch_decode_raw_fixture(0, 262_144)?;
    let tight_limits = profile(1_315_950)?;
    let mut tight = PolicySchemaCharge::new(tight_limits);
    let result = decode_complete_core_policy_patch_frame(
        &encoded,
        tight_limits,
        AdmissionLimits::default(),
        &mut tight,
    );
    assert_eq!(
        result
            .err()
            .ok_or("expected prospective large-leaf reserve/copy debit refusal")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes {
                bytes: 1_315_951,
                maximum: 1_315_950
            }
        ))
    );
    assert_eq!(
        (tight.g(), tight.p(), tight.total()),
        (1_053_679, 0, 1_053_679)
    );
    Ok(())
}

fn assert_core_patch_decode_global_bytes(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let literal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (final_length, hash) in [
        (
            262_072,
            "46f43dd393c90bceb987566bd13bbdba2561b018ed4ce7b09c3404d36e30993c",
        ),
        (
            262_073,
            "0cb749d7892cd97d98281beb68d19da1c23f49f3da1b986ba4eda95a67ccd00c",
        ),
    ] {
        let mut intent = literal[83..].to_vec();
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
        let encoded = core_patch_prepare_literal_frame(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        assert_eq!(
            <[u8; 32]>::from(Sha256::digest(&encoded)).as_slice(),
            decode_core_patch_literal(hash)?.as_slice()
        );
        let before = charge.g();
        if final_length == 262_073 {
            assert_eq!((intent.len(), encoded.len()), (1_048_577, 1_048_660));
            assert_core_patch_decode_refusal(
                &encoded,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit),
                851,
                limits,
                charge,
            )?;
        } else {
            assert_eq!((intent.len(), encoded.len()), (1_048_576, 1_048_659));
            let decoded = decode_complete_core_policy_patch_frame(
                &encoded,
                limits,
                AdmissionLimits::default(),
                charge,
            )?;
            assert_core_patch_decoded(
                &decoded,
                &encoded,
                hash,
                &command,
                [b"t", b"g", b"r", b"q"],
                stamp,
            )?;
            assert_eq!((charge.g(), charge.p()), (before + 8_397_524, 0));
        }
    }
    Ok(())
}

fn assert_core_patch_decode_scope_limits(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for length in [8_172, 8_173] {
        let tenant = vec![b't'; length];
        let mut intent = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
        intent.extend_from_slice(&u32::try_from(length)?.to_be_bytes());
        intent.extend_from_slice(&tenant);
        intent.extend_from_slice(b"\0\0\0\0\x01r\0\0\0\0\0\0\0\0");
        let mut encoded =
            core_patch_prepare_literal_frame(&intent, [&tenant, b"g", b"r", b"q"], stamp)?;
        let before = charge.g();
        if length == 8_173 {
            encoded[25 + length + 3] ^= 1;
            assert_core_patch_decode_refusal(
                &encoded,
                CompletePolicyRequestError::ScopeLimit,
                8_706,
                limits,
                charge,
            )?;
        } else {
            assert_eq!((intent.len(), encoded.len()), (8_223, 16_477));
            let decoded = decode_complete_core_policy_patch_frame(
                &encoded,
                limits,
                AdmissionLimits::default(),
                charge,
            )?;
            assert_core_patch_decoded(
                &decoded,
                &encoded,
                "b310d2477236a40842e006125aff1e9d3ee305463e9c2223cef0b51ffe03548b",
                &core_patch_prepare_command(0)?,
                [&tenant, b"g", b"r", b"q"],
                stamp,
            )?;
            assert_eq!(decoded.isolation_scope().len(), 8_192);
            assert_eq!((charge.g(), charge.p()), (before + 103_552, 0));
        }
    }
    Ok(())
}
