//! Independent frame fixtures and complete source, scope, and account observations.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyRequestError, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, restore_complete_policy,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, PreparedDigest};
use wf_ports::log::{
    VerifiedCompleteCorePolicyPatchFrame, verify_complete_core_policy_patch_frame,
};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::super::super::{TestResult, minimal_data, profile};

pub(super) fn core_patch_frame_bytes(intent: &[u8]) -> TestResult<Vec<u8>> {
    let digest: [u8; 32] = Sha256::digest(intent).into();
    let mut encoded = b"FWCP\0\x01\x05\0\0\0\x01t\0\0\0\x01g\0\0\0\x01r\0\0\0\x01q\0\x02".to_vec();
    encoded.extend_from_slice(&digest);
    encoded.extend_from_slice(&[0; 18]);
    encoded.extend_from_slice(&u32::try_from(intent.len())?.to_be_bytes());
    encoded.extend_from_slice(intent);
    Ok(encoded)
}

pub(super) fn assert_core_patch_default_frame(
    value: &VerifiedCompleteCorePolicyPatchFrame<'_>,
    encoded: &[u8],
    intent: &[u8],
) {
    let original: [u8; 32] = Sha256::digest(intent).into();
    let prepared: [u8; 32] = Sha256::digest(encoded).into();
    let log = LogIdentity::new(1, 2, AppliedPosition::new(3));
    let header = value.header(log);
    assert_eq!(value.encoded(), encoded);
    assert_eq!(value.original_intent(), intent);
    assert_eq!(value.digest(), PreparedDigest::new(prepared));
    assert_eq!(
        value.isolation_scope(),
        b"FWSC\0\x01\0\0\0\x01t\0\0\0\x01g\0\0\0\x01r"
    );
    assert_eq!(header.format_version, 1);
    assert_eq!(header.scope.tenant().as_bytes(), b"t");
    assert_eq!(header.scope.group().as_bytes(), b"g");
    assert_eq!(header.scope.recovery().as_bytes(), b"r");
    assert_eq!(header.request.as_bytes(), b"q");
    assert_eq!(header.intent.version(), 2);
    assert_eq!(header.intent.digest(), &original);
    assert_eq!(header.stamp.hlc().physical().get(), 0);
    assert_eq!(header.stamp.hlc().logical(), 0);
    assert_eq!(header.stamp.wall().get(), 0);
    assert_eq!(header.identity, EntryIdentity::new(log, value.digest()));
    assert_eq!(header.encoded_entry, encoded);
    assert_eq!(header.encoded_isolation_scope, value.isolation_scope());
}

pub(super) fn assert_core_patch_frame_refusal(
    encoded: &[u8],
    expected: CompletePolicyRequestError,
    fee: u64,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let unchanged = encoded.to_vec();
    let before = charge.g();
    let result = verify_complete_core_policy_patch_frame(
        encoded,
        limits,
        AdmissionLimits::default(),
        charge,
    );
    assert_eq!(result.err().ok_or("expected frame refusal")?, expected);
    assert_eq!(encoded, unchanged.as_slice());
    assert_eq!((charge.g(), charge.p()), (before + fee, 0));
    Ok(())
}

pub(super) fn assert_core_patch_frame_prefix_refusals(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let intent = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0\0\0\0\x01t\0\0\0\0\x01r\0\0\0\0\0\0\0\0";
    let base = core_patch_frame_bytes(intent)?;
    let malformed = CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed);
    let byte_limit = CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit);
    let version =
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::UnsupportedVersion);
    let tag = CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::UnsupportedTag);
    let optional =
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::InvalidOptionality);
    for (offset, replacement, remove, error, fee) in [
        (0, b"X".as_slice(), 1, malformed, 132),
        (5, b"\x02", 1, version, 134),
        (6, b"\x06", 1, tag, 135),
        (7, b"\0\0\0\0", 4, malformed, 267),
        (7, b"\0\0\x20\x01", 4, byte_limit, 267),
        (12, b"\0\0\0\0", 4, malformed, 400),
        (12, b"\0\0\x01\x01", 4, byte_limit, 400),
        (17, b"\0\0\0\0", 4, malformed, 533),
        (17, b"\0\0\x01\x01", 4, byte_limit, 533),
        (22, b"\0\0\0\0", 4, malformed, 666),
        (22, b"\0\0\x01\x01", 4, byte_limit, 666),
        (28, b"\x03", 1, version, 669),
        (61, b"\x01", 1, malformed, 711),
        (78, b"\x01", 1, malformed, 719),
        (79, b"\0\x10\0\x01", 4, byte_limit, 851),
        (83, b"\0", 1, malformed, 1_012),
        (121, b"\x02", 1, optional, 1_146),
        (122, b"\0\x04\0\x01", 4, byte_limit, 1_406),
        (127, b"\x03", 1, tag, 1_408),
        (128, b"\x03", 1, tag, 1_409),
        (129, b"\x03", 1, tag, 1_410),
        (130, b"\x03", 1, tag, 1_411),
        (134, b"\x01", 1, malformed, 1_799),
        (120, b"u", 1, malformed, 1_673),
    ] {
        let mut encoded = base.clone();
        drop(encoded.splice(offset..offset + remove, replacement.iter().copied()));
        assert_core_patch_frame_refusal(&encoded, error, fee, limits, charge)?;
    }
    for (case, fee) in [(0, 669), (1, 851), (2, 1_411), (3, 851), (4, 1_543)] {
        let mut encoded = base.clone();
        let error = if case <= 2 {
            malformed
        } else {
            CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::TrailingBytes)
        };
        match case {
            0 => encoded.truncate(29),
            1 => encoded.truncate(134),
            2 => {
                encoded.truncate(134);
                encoded[79..83].copy_from_slice(&51_u32.to_be_bytes());
            }
            3 => encoded.push(0),
            4 => {
                encoded.push(0);
                encoded[79..83].copy_from_slice(&53_u32.to_be_bytes());
            }
            _ => return Err("unknown fixed truncation case".into()),
        }
        assert_core_patch_frame_refusal(&encoded, error, fee, limits, charge)?;
    }
    Ok(())
}

pub(super) fn assert_core_patch_frame_scope_limits(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let admission = AdmissionLimits::default();
    for tenant_length in [8_172, 8_173] {
        let tenant = vec![b't'; tenant_length];
        let count = u32::try_from(tenant_length)?.to_be_bytes();
        let mut intent = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0".to_vec();
        intent.extend_from_slice(&count);
        intent.extend_from_slice(&tenant);
        intent.extend_from_slice(b"\0\0\0\0\x01r\0\0\0\0\0\0\0\0");
        let original: [u8; 32] = Sha256::digest(&intent).into();
        let mut encoded = b"FWCP\0\x01\x05".to_vec();
        encoded.extend_from_slice(&count);
        encoded.extend_from_slice(&tenant);
        encoded.extend_from_slice(b"\0\0\0\x01g\0\0\0\x01r\0\0\0\x01q\0\x02");
        let digest_offset = encoded.len();
        encoded.extend_from_slice(&original);
        encoded.extend_from_slice(&[0; 18]);
        encoded.extend_from_slice(&u32::try_from(intent.len())?.to_be_bytes());
        encoded.extend_from_slice(&intent);
        let before = charge.g();
        if tenant_length == 8_173 {
            encoded[digest_offset] ^= 1;
            let error = CompletePolicyRequestError::ScopeLimit;
            assert_core_patch_frame_refusal(&encoded, error, 8_706, limits, charge)?;
            let wrong = profile(19_999_999)?;
            let result =
                verify_complete_core_policy_patch_frame(&encoded, wrong, admission, charge);
            let expected = CompletePolicyRequestError::Encoding(
                CompletePolicyEncodingError::Schema(PolicySchemaError::ProfileMismatch),
            );
            assert_eq!(result.err().ok_or("expected profile mismatch")?, expected);
            assert_eq!((charge.g(), charge.p()), (before + 8_706, 0));
        } else {
            assert_eq!((intent.len(), encoded.len()), (8_223, 16_477));
            assert_eq!(
                &<[u8; 32]>::from(Sha256::digest(&intent)),
                b"\x83\xe5\xd6\x97\x73\x12\x14\x08\x0c\x98\xc5\xaa\xd9\x60\x4f\x30\xfe\xb3\x00\x92\x6f\xf6\x50\xd5\x0f\xc2\x54\xd0\xa9\x3f\x96\x14"
            );
            assert_eq!(
                &<[u8; 32]>::from(Sha256::digest(&encoded)),
                b"\xb3\x10\xd2\x47\x72\x36\xa4\x08\x42\xe0\x06\x12\x5a\xff\x1e\x9d\x3e\xe3\x05\x46\x3e\x9c\x22\x23\xce\xf0\xb5\x1f\xfe\x03\x54\x8b"
            );
            let mut expected_scope = b"FWSC\0\x01".to_vec();
            expected_scope.extend_from_slice(&count);
            expected_scope.extend_from_slice(&tenant);
            expected_scope.extend_from_slice(b"\0\0\0\x01g\0\0\0\x01r");
            let prepared: [u8; 32] = Sha256::digest(&encoded).into();
            {
                let value =
                    verify_complete_core_policy_patch_frame(&encoded, limits, admission, charge)?;
                let log = LogIdentity::new(1, 2, AppliedPosition::new(3));
                let header = value.header(log);
                assert_eq!(value.encoded(), encoded.as_slice());
                assert_eq!(value.original_intent(), intent.as_slice());
                assert_eq!(value.isolation_scope(), expected_scope.as_slice());
                assert_eq!(value.isolation_scope().len(), 8_192);
                assert_eq!(value.digest(), PreparedDigest::new(prepared));
                assert_eq!(header.scope.tenant().as_bytes(), tenant.as_slice());
                assert_eq!(header.scope.group().as_bytes(), b"g");
                assert_eq!(header.scope.recovery().as_bytes(), b"r");
                assert_eq!(header.request.as_bytes(), b"q");
                assert_eq!(header.intent.version(), 2);
                assert_eq!(header.intent.digest(), &original);
                assert_eq!(
                    header.stamp,
                    CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?
                );
                assert_eq!(header.format_version, 1);
                assert_eq!(
                    header.identity,
                    EntryIdentity::new(log, PreparedDigest::new(prepared))
                );
                assert_eq!(header.encoded_entry, encoded.as_slice());
                assert_eq!(header.encoded_isolation_scope, expected_scope.as_slice());
                assert_eq!((charge.g(), charge.p()), (before + 94_304, 0));
            }
            encoded[digest_offset] ^= 1;
            assert_core_patch_frame_refusal(
                &encoded,
                CompletePolicyRequestError::IntentMismatch,
                42_900,
                limits,
                charge,
            )?;
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_frame_account_case(
    encoded: &[u8],
    intent: &[u8],
    maximum: u64,
    target: u64,
    attempts: u64,
    peak: bool,
) -> TestResult {
    let limits = profile(maximum)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let mut indexed_expected = minimal_data();
    indexed_expected.indexed_attributes = Some(vec![b"a".to_vec()]);
    let held = if peak {
        let mut indexed = minimal_data();
        indexed.indexed_attributes = Some(vec![b"a".to_vec()]);
        Some(restore_complete_policy(1, indexed, limits, &mut charge)?)
    } else {
        None
    };
    let (setup, scratch) = if peak { (1_928, 64) } else { (0, 0) };
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (setup, scratch, setup + scratch)
    );
    if let Some(held) = &held {
        assert!(held.data() == &indexed_expected);
        assert_eq!(held.generation(), 1);
    }
    let mut outputs = Vec::new();
    for index in 0..attempts {
        let result =
            verify_complete_core_policy_patch_frame(encoded, limits, admission, &mut charge);
        let refused = index + 1 == attempts && maximum < target;
        if refused {
            let error = result
                .err()
                .ok_or("expected final frame hash work refusal")?;
            let expected = CompletePolicyRequestError::Encoding(
                CompletePolicyEncodingError::Schema(PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum,
                }),
            );
            assert_eq!(error, expected);
        } else {
            let value = result?;
            assert_core_patch_default_frame(&value, encoded, intent);
            assert_eq!(value.encoded().as_ptr(), encoded.as_ptr());
            assert_eq!(value.original_intent().as_ptr(), encoded[83..].as_ptr());
            outputs.push(value);
        }
        for value in &outputs {
            assert_core_patch_default_frame(value, encoded, intent);
        }
        assert_eq!(u64::try_from(outputs.len())?, index + u64::from(!refused));
        let delta = if intent.len() == 116 { 6_791 } else { 4_423 };
        let final_fee = 128 + u64::try_from(encoded.len())?;
        let g = setup + (index + 1) * delta - if refused { final_fee } else { 0 };
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (g, scratch, g + scratch)
        );
        if let Some(held) = &held {
            assert!(held.data() == &indexed_expected);
            assert_eq!(held.generation(), 1);
        }
    }
    let before = (charge.g(), charge.p(), charge.total());
    let wrong = profile(maximum + 1)?;
    let result = verify_complete_core_policy_patch_frame(encoded, wrong, admission, &mut charge);
    let expected = CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
        PolicySchemaError::ProfileMismatch,
    ));
    assert_eq!(result.err().ok_or("expected profile mismatch")?, expected);
    assert_eq!((charge.g(), charge.p(), charge.total()), before);
    for value in &outputs {
        assert_core_patch_default_frame(value, encoded, intent);
    }
    if let Some(held) = &held {
        assert!(held.data() == &indexed_expected);
        assert_eq!(held.generation(), 1);
    }
    Ok(())
}
