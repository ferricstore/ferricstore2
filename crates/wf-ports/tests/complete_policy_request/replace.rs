//! Original Complete Replace frames, mathematical ownership and paid refusals.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyRequestError, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, prepare_complete_policy_replace_frame_caller,
    prepare_complete_policy_value_v2, restore_complete_policy,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::{ClientIntent, RequestValueError};
use wf_ports::log::{prepare_complete_policy_replace_frame, verify_complete_policy_replace_frame};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::{
    MINIMAL_INTENT_BYTES, PRESENT_INTENT_BYTES, TestResult, minimal_data, present_data, profile,
};

mod boundaries;
mod cases;
mod literals;

use boundaries::{assert_replace_frame_boundary, assert_replace_frame_raw_limits};
use cases::{
    assert_replace_frame_canonical_lists, replace_frame_empty_field, replace_frame_input,
    replace_frame_widths,
};
use literals::{
    FROZEN_COMPLETE_REPLACE_FRAMES, assert_replace_frame_account, assert_replace_frame_enum_tags,
    assert_replace_frame_verified, replace_frame_hex, replace_frame_literal,
};

#[test]
fn complete_replace_frame_preserves_frozen_minimal_and_complete_original_intents() -> TestResult {
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    for (case, (hex, hash)) in FROZEN_COMPLETE_REPLACE_FRAMES.iter().enumerate() {
        let expected = replace_frame_hex(hex)?;
        let digest: [u8; 32] = replace_frame_hex(hash)?
            .try_into()
            .map_err(|_| "fixed digest width")?;
        let original = if case == 0 {
            MINIMAL_INTENT_BYTES
        } else {
            PRESENT_INTENT_BYTES
        };
        let data = if case == 0 {
            minimal_data()
        } else {
            present_data()
        };
        assert_eq!(&expected[83..], original);
        assert_eq!(<[u8; 32]>::from(Sha256::digest(&expected)), digest);
        let input = super::input(data, None)?;
        let before = charge.g();
        let prepared = prepare_complete_policy_replace_frame(
            input,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(prepared.as_bytes(), expected);
        assert_eq!(prepared.digest(), PreparedDigest::new(digest));
        let (producer, verification_work, peak) = if case == 0 {
            (4_612, 4_423, 0)
        } else {
            (47_249, 22_427, 128)
        };
        assert_eq!((charge.g(), charge.p()), (before + producer, peak));
        let verified = verify_complete_policy_replace_frame(
            prepared.as_bytes(),
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(
            (charge.g(), charge.p()),
            (before + producer + verification_work, peak)
        );
        assert_replace_frame_verified(
            &verified,
            &expected,
            LogIdentity::new(1, 2, AppliedPosition::new(3)),
        );
        assert_eq!(verified.encoded().as_ptr(), prepared.as_bytes().as_ptr());
        assert_eq!(
            verified.original_intent().as_ptr(),
            prepared.as_bytes()[83..].as_ptr()
        );
    }
    Ok(())
}

#[test]
fn complete_replace_frame_binds_header_operands_without_binding_assigned_generation() -> TestResult
{
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let first = prepare_complete_policy_value_v2(
        restore_complete_policy(1, minimal_data(), limits, &mut charge)?,
        limits,
        &mut charge,
    )?;
    let later = prepare_complete_policy_value_v2(
        restore_complete_policy(9, minimal_data(), limits, &mut charge)?,
        limits,
        &mut charge,
    )?;
    let mut expected_value = replace_frame_hex(
        "46575056000200000000000000010000001646575053000200000001720000000000000000000000",
    )?;
    assert_eq!(first.encoded(), expected_value);
    expected_value[6..14].copy_from_slice(&9_u64.to_be_bytes());
    assert_eq!(later.encoded(), expected_value);
    assert!(first.data() == later.data());
    assert_eq!((first.generation(), later.generation()), (1, 9));
    for (case, ids) in [*b"tgrq", *b"tGrq", *b"tgRq", *b"tgrQ", *b"tgrq", *b"ugrq"]
        .into_iter()
        .enumerate()
    {
        let stamp = if case == 4 {
            CommittedTime::new(HlcTimestamp::new(99, 7)?, 98)?
        } else {
            CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?
        };
        let ids = [&ids[0..1], &ids[1..2], &ids[2..3], &ids[3..4]];
        let mut intent = MINIMAL_INTENT_BYTES.to_vec();
        intent[34] = ids[0][0];
        let expected = replace_frame_literal(&intent, ids, stamp)?;
        let prepared = prepare_complete_policy_replace_frame(
            replace_frame_input(minimal_data(), None, ids, stamp)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(prepared.as_bytes(), expected);
        let verified = verify_complete_policy_replace_frame(
            prepared.as_bytes(),
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        for log in [
            LogIdentity::new(1, 2, AppliedPosition::new(3)),
            LogIdentity::new(9, 8, AppliedPosition::new(7)),
        ] {
            assert_replace_frame_verified(&verified, &expected, log);
        }
        let original_hash: [u8; 32] = Sha256::digest(&intent).into();
        assert_eq!(original_hash.as_slice(), &expected[29..61]);
        assert_eq!(case == 5, original_hash != super::MINIMAL_INTENT_DIGEST);
    }
    for guard in [None, Some(0), Some(u64::MAX)] {
        let mut intent = MINIMAL_INTENT_BYTES.to_vec();
        if let Some(value) = guard {
            let mut payload = vec![1];
            payload.extend_from_slice(&value.to_be_bytes());
            drop(intent.splice(35..36, payload));
        }
        let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
        let expected = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let (caller, bytes, frame) = prepare_complete_policy_replace_frame_caller(
            super::input(minimal_data(), guard)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(caller.input().expected_generation(), guard);
        assert!(caller.input().data() == &minimal_data());
        assert!(caller.schema_limits() == limits);
        assert_eq!(caller.admission_limits(), AdmissionLimits::default());
        assert_eq!((bytes, frame), (intent.len(), expected.len()));
        let prepared = prepare_complete_policy_replace_frame(
            super::input(minimal_data(), guard)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(prepared.as_bytes(), expected);
        assert_eq!(
            prepared.digest(),
            PreparedDigest::new(Sha256::digest(&expected).into())
        );
        assert_eq!(&prepared.as_bytes()[83..], intent);
    }
    assert_eq!(
        ClientIntent::new(2, [0; 32]),
        Err(RequestValueError::UnsupportedVersion)
    );
    Ok(())
}

#[test]
fn complete_replace_frame_preserves_empty_groups_and_full_governance_widths() -> TestResult {
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (case, (start, end, empty)) in [
        (41, 51, &[1, 0, 0, 0, 0, 0][..]),
        (83, 89, &[1, 0, 0, 0, 0][..]),
        (157, 163, &[1, 0, 0, 0, 0][..]),
        (260, 266, &[1, 0, 0, 0, 0][..]),
        (142, 157, &[1, 0, 0, 0, 0][..]),
        (167, 182, &[1, 0, 0, 0, 0][..]),
        (182, 187, &[1, 0, 0, 0, 0][..]),
        (339, 354, &[1, 0, 0, 0, 0][..]),
        (355, 365, &[1, 0, 0, 0, 0][..]),
        (365, 375, &[1, 0, 0, 0, 0][..]),
        (375, 385, &[1, 0, 0, 0, 0][..]),
        (194, 223, &[1, 0, 0, 0, 0][..]),
        (223, 266, &[1, 0, 0, 0, 0][..]),
        (266, 354, &[1, 0, 0, 0, 0][..]),
        (166, 189, &[1, 0, 0, 0][..]),
        (189, 194, &[1, 0, 0][..]),
        (354, 394, &[1, 0, 0, 0, 0][..]),
        (115, 119, &[1, 0, 0, 0][..]),
        (119, 134, &[1, 0, 0][..]),
        (134, 142, &[1, 0, 0, 0, 0, 0, 0, 0][..]),
        (60, 89, &[1, 0, 0, 0][..]),
        (89, 104, &[1, 0, 0][..]),
        (163, 394, &[1, 0, 0, 0, 0, 0, 0, 0][..]),
    ]
    .into_iter()
    .enumerate()
    {
        let mut held = Vec::new();
        for present in [false, true] {
            let mut intent = PRESENT_INTENT_BYTES.to_vec();
            let payload = if present { empty } else { &[0][..] };
            drop(intent.splice(start..end, payload.iter().copied()));
            let expected = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let mut data = present_data();
            replace_frame_empty_field(&mut data, case, present)?;
            let prepared = prepare_complete_policy_replace_frame(
                super::input(data, None)?,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )?;
            assert_eq!(prepared.as_bytes(), expected);
            assert_eq!(
                prepared.digest(),
                PreparedDigest::new(Sha256::digest(&expected).into())
            );
            let verified = verify_complete_policy_replace_frame(
                prepared.as_bytes(),
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )?;
            assert_replace_frame_verified(
                &verified,
                &expected,
                LogIdentity::new(1, 2, AppliedPosition::new(3)),
            );
            held.push(prepared);
        }
        assert_ne!(held[0].as_bytes(), held[1].as_bytes());
        assert_ne!(held[0].digest(), held[1].digest());
    }
    assert_replace_frame_enum_tags(limits, &mut charge)?;
    Ok(())
}

#[test]
fn complete_replace_frame_enforces_whole_scope_raw_count_and_list_canonicality_priority()
-> TestResult {
    let limits = PolicySchemaLimits::new(PolicyLimits::new(4)?, 1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let expected = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
    let wrong = profile(1_000_000_001)?;
    let before = (charge.g(), charge.p());
    let result = prepare_complete_policy_replace_frame(
        super::input(minimal_data(), None)?,
        wrong,
        AdmissionLimits::default(),
        &mut charge,
    );
    assert_eq!(
        result.err().ok_or("profile producer refusal")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    );
    let result = verify_complete_policy_replace_frame(
        &expected,
        wrong,
        AdmissionLimits::default(),
        &mut charge,
    );
    assert_eq!(
        result.err().ok_or("profile verifier refusal")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    );
    assert_eq!((charge.g(), charge.p()), before);
    for maximum in [134, 135] {
        let admission = AdmissionLimits::new(1_048_576, maximum, 1_000)?;
        let before = charge.g();
        let result = prepare_complete_policy_replace_frame(
            super::input(minimal_data(), None)?,
            limits,
            admission,
            &mut charge,
        );
        if maximum == 134 {
            assert_eq!(
                result.err().ok_or("producer F cap")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
            );
            assert_eq!(charge.g(), before + 1_543);
        } else {
            assert_eq!(result?.as_bytes(), expected);
            assert_eq!(charge.g(), before + 4_612);
        }
        let before = charge.g();
        let result =
            verify_complete_policy_replace_frame(&expected, limits, admission, &mut charge);
        if maximum == 134 {
            assert_eq!(
                result.err().ok_or("verifier F cap")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
            );
            assert_eq!(charge.g(), before);
        } else {
            assert_eq!(charge.g(), before + 4_423);
            assert_replace_frame_verified(
                &result?,
                &expected,
                LogIdentity::new(1, 2, AppliedPosition::new(3)),
            );
        }
    }
    for case in 0..4 {
        assert_replace_frame_boundary(case, limits, &mut charge)?;
    }
    assert_replace_frame_canonical_lists(limits, &mut charge)?;
    assert_replace_frame_raw_limits(limits, &mut charge)?;
    Ok(())
}

#[test]
fn complete_replace_frame_retains_each_same_account_phase_and_prior_scratch() -> TestResult {
    for producer in [false, true] {
        let single = if producer { 4_612 } else { 4_423 };
        for (maximum, repeated, held_peak) in [
            (single, false, false),
            (single - 1, false, false),
            (2 * single, true, false),
            (2 * single - 1, true, false),
            (single + 1_992, false, true),
            (single + 1_991, false, true),
        ] {
            assert_replace_frame_account(maximum, producer, repeated, held_peak)?;
        }
    }
    for (producer, target, retained) in [
        (true, 128, 0),
        (true, 1_671, 1_543),
        (true, 2_363, 1_671),
        (true, 2_543, 2_363),
        (true, 2_806, 2_543),
        (true, 3_657, 2_806),
        (true, 4_349, 3_657),
        (true, 4_612, 4_349),
        (false, 1_673, 1_543),
        (false, 1_853, 1_673),
        (false, 2_045, 1_853),
        (false, 2_173, 2_045),
        (false, 2_301, 2_173),
        (false, 2_430, 2_301),
        (false, 2_559, 2_430),
        (false, 2_688, 2_559),
        (false, 2_817, 2_688),
        (false, 2_945, 2_817),
        (false, 3_478, 2_945),
        (false, 3_627, 3_478),
        (false, 4_160, 3_627),
        (false, 4_423, 4_160),
    ] {
        let maximum = target - 1;
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let expected = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
        let result = if producer {
            prepare_complete_policy_replace_frame(
                super::input(minimal_data(), None)?,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )
            .map(|_| ())
        } else {
            verify_complete_policy_replace_frame(
                &expected,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )
            .map(|_| ())
        };
        assert_eq!(
            result,
            Err(CompletePolicyRequestError::Encoding(
                CompletePolicyEncodingError::Schema(PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum
                })
            ))
        );
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (retained, 0, retained)
        );
    }
    let limits = profile(1_543)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = prepare_complete_policy_replace_frame(
        super::input(minimal_data(), None)?,
        limits,
        AdmissionLimits::new(1_048_576, 134, 1_000)?,
        &mut charge,
    );
    assert_eq!(
        result.err().ok_or("F cap before Q")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
    );
    assert_eq!((charge.g(), charge.p()), (1_543, 0));
    Ok(())
}

#[test]
fn complete_replace_frame_keeps_business_invalid_original_fields_available_for_fresh_successor()
-> TestResult {
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for maximum in [false, true] {
        let mut data = present_data();
        let scalar = if maximum { u64::MAX } else { 0 };
        replace_frame_widths(&mut data, maximum)?;
        let mut intent = PRESENT_INTENT_BYTES.to_vec();
        for offset in [
            43, 52, 66, 74, 91, 121, 204, 215, 233, 241, 252, 276, 284, 293, 302, 313, 322, 331,
            386,
        ] {
            intent[offset..offset + 8].copy_from_slice(&scalar.to_be_bytes());
        }
        for offset in [100, 130] {
            intent[offset..offset + 4]
                .copy_from_slice(&if maximum { u32::MAX } else { 0 }.to_be_bytes());
        }
        intent[62..64].copy_from_slice(&if maximum { u16::MAX } else { 0 }.to_be_bytes());
        for offset in [82, 311] {
            intent[offset] = if maximum { 255 } else { 0 };
        }
        for offset in [114, 188, 191, 213, 250] {
            intent[offset] = u8::from(maximum);
        }
        for offset in [165, 193] {
            intent[offset] = if maximum { 2 } else { 0 };
        }
        let expected = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let prepared = prepare_complete_policy_replace_frame(
            super::input(data, None)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(prepared.as_bytes(), expected);
        let verified = verify_complete_policy_replace_frame(
            prepared.as_bytes(),
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_replace_frame_verified(
            &verified,
            &expected,
            LogIdentity::new(1, 2, AppliedPosition::new(3)),
        );
    }
    for (field, start, end, payload) in [
        (0, 36, 41, &[0, 0, 0, 0][..]),
        (1, 41, 51, &[1, 0, 0, 0, 0, 0][..]),
        (2, 41, 51, &[1, 0, 0, 0, 0, 2, 0, 255][..]),
        (3, 147, 152, &[0, 0, 0, 0][..]),
    ] {
        let mut data = present_data();
        match field {
            0 => data.workflow_type.clear(),
            1 => data.version = Some(PolicyVersion::Bytes(Vec::new())),
            2 => data.version = Some(PolicyVersion::Bytes(vec![0, 255])),
            3 => data.indexed_attributes.as_mut().ok_or("indexed fixture")?[0].clear(),
            _ => return Err("unknown business fixture".into()),
        }
        let mut intent = PRESENT_INTENT_BYTES.to_vec();
        drop(intent.splice(start..end, payload.iter().copied()));
        let expected = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let prepared = prepare_complete_policy_replace_frame(
            super::input(data, None)?,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(prepared.as_bytes(), expected);
        let verified = verify_complete_policy_replace_frame(
            prepared.as_bytes(),
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_replace_frame_verified(
            &verified,
            &expected,
            LogIdentity::new(1, 2, AppliedPosition::new(3)),
        );
    }
    Ok(())
}

mod decode;

mod apply;
