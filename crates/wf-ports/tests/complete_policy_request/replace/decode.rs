//! Consuming original Complete Replace reconstruction on continuous work accounts.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompletePolicyData, CompletePolicyEncodingError, CompletePolicyRequestError,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, PreparedDigest};
use wf_ports::log::{
    DecodedCompletePolicyReplaceFrame, decode_complete_policy_replace_frame,
    prepare_complete_policy_replace_frame,
};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::super::{
    MINIMAL_INTENT_BYTES, PRESENT_INTENT_BYTES, TestResult, minimal_data, present_data, profile,
};
use super::cases::{replace_frame_empty_field, replace_frame_widths};
use super::literals::{FROZEN_COMPLETE_REPLACE_FRAMES, replace_frame_hex, replace_frame_literal};

mod cases;

use cases::{
    assert_replace_decode_account, assert_replace_decode_domain_moves, assert_replace_decode_enums,
    assert_replace_decode_errors, assert_replace_decode_header_limits,
    assert_replace_decode_list_order, assert_replace_decode_map_order,
};

#[test]
fn complete_replace_decode_reconstructs_frozen_original_owners() -> TestResult {
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    for (case, (hex, hash)) in FROZEN_COMPLETE_REPLACE_FRAMES.iter().enumerate() {
        let frame = replace_frame_hex(hex)?;
        let expected = if case == 0 {
            minimal_data()
        } else {
            present_data()
        };
        let original = if case == 0 {
            MINIMAL_INTENT_BYTES
        } else {
            PRESENT_INTENT_BYTES
        };
        let digest: [u8; 32] = replace_frame_hex(hash)?
            .try_into()
            .map_err(|_| "fixed digest width")?;
        assert_eq!(&frame[83..], original);
        assert_eq!(<[u8; 32]>::from(Sha256::digest(&frame)), digest);
        let before = charge.g();
        let decoded = decode_complete_policy_replace_frame(
            &frame,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        let (fee, peak) = if case == 0 { (5_500, 0) } else { (43_062, 128) };
        assert_eq!((charge.g(), charge.p()), (before + fee, peak));
        assert_replace_decoded(&decoded, &frame, &expected, None)?;
        assert_eq!(decoded.encoded().as_ptr(), frame.as_ptr());
        assert_eq!(decoded.original_intent().as_ptr(), frame[83..].as_ptr());
    }
    Ok(())
}

#[test]
fn complete_replace_decode_preserves_presence_and_governance_fields() -> TestResult {
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
        let mut absent_digest = None;
        for present in [false, true] {
            let mut intent = PRESENT_INTENT_BYTES.to_vec();
            drop(intent.splice(
                start..end,
                if present { empty } else { &[0][..] }.iter().copied(),
            ));
            let frame = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let mut expected = present_data();
            replace_frame_empty_field(&mut expected, case, present)?;
            let decoded = decode_complete_policy_replace_frame(
                &frame,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )?;
            assert_replace_decoded(&decoded, &frame, &expected, None)?;
            if let Some(absent) = absent_digest {
                assert_ne!(decoded.digest(), absent);
            } else {
                absent_digest = Some(decoded.digest());
            }
        }
    }
    assert_replace_decode_enums(limits, &mut charge)?;
    assert_replace_decode_list_order(limits, &mut charge)?;
    assert_replace_decode_map_order(limits, &mut charge)?;
    Ok(())
}

#[test]
fn complete_replace_decode_moves_header_and_original_input_owners() -> TestResult {
    assert_replace_decode_domain_moves()?;
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let frame = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[1].0)?;
    let expected = present_data();
    let decoded = decode_complete_policy_replace_frame(
        &frame,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_replace_decoded(&decoded, &frame, &expected, None)?;
    assert_eq!((charge.g(), charge.p()), (43_062, 128));
    let pointers = [
        decoded.input().scope().tenant().as_bytes().as_ptr(),
        decoded.input().scope().group().as_bytes().as_ptr(),
        decoded.input().scope().recovery().as_bytes().as_ptr(),
        decoded.input().request().as_bytes().as_ptr(),
        decoded.input().data().workflow_type.as_ptr(),
    ];
    let input = decoded.into_input();
    assert!(input.data() == &expected);
    assert_eq!(input.expected_generation(), None);
    assert_eq!(input.scope().tenant().as_bytes(), b"t");
    assert_eq!(input.scope().group().as_bytes(), b"g");
    assert_eq!(input.scope().recovery().as_bytes(), b"r");
    assert_eq!(input.request().as_bytes(), b"q");
    assert_eq!(
        input.stamp(),
        CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?
    );
    assert_eq!(
        [
            input.scope().tenant().as_bytes().as_ptr(),
            input.scope().group().as_bytes().as_ptr(),
            input.scope().recovery().as_bytes().as_ptr(),
            input.request().as_bytes().as_ptr(),
            input.data().workflow_type.as_ptr()
        ],
        pointers
    );
    let (scope, request, stamp, guard, data) = input.into_parts();
    assert!(data == expected);
    assert_eq!(guard, None);
    assert_eq!(scope.tenant().as_bytes(), b"t");
    assert_eq!(scope.group().as_bytes(), b"g");
    assert_eq!(scope.recovery().as_bytes(), b"r");
    assert_eq!(request.as_bytes(), b"q");
    assert_eq!(stamp, CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?);
    assert_eq!(
        [
            scope.tenant().as_bytes().as_ptr(),
            scope.group().as_bytes().as_ptr(),
            scope.recovery().as_bytes().as_ptr(),
            request.as_bytes().as_ptr(),
            data.workflow_type.as_ptr()
        ],
        pointers
    );
    let reproduced = prepare_complete_policy_replace_frame(
        wf_domain::policy::complete::CompletePolicyReplaceInput::new(
            scope, request, stamp, guard, data,
        ),
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!(reproduced.as_bytes(), frame);
    assert_eq!(
        reproduced.digest(),
        PreparedDigest::new(Sha256::digest(&frame).into())
    );
    assert_eq!((charge.g(), charge.p()), (90_311, 128));
    Ok(())
}

#[test]
fn complete_replace_decode_preserves_business_invalid_fields_without_fresh_authority() -> TestResult
{
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for maximum in [false, true] {
        let mut expected = present_data();
        replace_frame_widths(&mut expected, maximum)?;
        let scalar = if maximum { u64::MAX } else { 0 };
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
        let frame = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let decoded = decode_complete_policy_replace_frame(
            &frame,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_replace_decoded(&decoded, &frame, &expected, None)?;
    }
    assert_replace_decode_business(limits, &mut charge)?;
    Ok(())
}

#[test]
fn complete_replace_decode_retains_same_account_work_and_prior_peak() -> TestResult {
    for (maximum, attempts, held, full) in [
        (5_500, 1, false, false),
        (5_499, 1, false, false),
        (11_000, 2, false, false),
        (10_999, 2, false, false),
        (7_492, 1, true, false),
        (7_491, 1, true, false),
        (12_992, 2, true, false),
        (12_991, 2, true, false),
        (43_190, 1, false, true),
        (43_189, 1, false, true),
        (45_118, 1, true, true),
        (45_117, 1, true, true),
        (86_252, 2, false, true),
        (86_251, 2, false, true),
    ] {
        assert_replace_decode_account(maximum, attempts, held, full)?;
    }
    for (target, prior) in [
        (4_551, 4_423),
        (4_581, 4_551),
        (4_709, 4_581),
        (4_713, 4_709),
        (4_714, 4_713),
        (4_715, 4_714),
        (4_843, 4_715),
        (4_971, 4_843),
        (5_099, 4_971),
        (5_103, 5_099),
        (5_104, 5_103),
        (5_233, 5_104),
        (5_234, 5_233),
        (5_235, 5_234),
        (5_236, 5_235),
        (5_237, 5_236),
        (5_241, 5_237),
        (5_369, 5_241),
        (5_497, 5_369),
        (5_498, 5_497),
        (5_499, 5_498),
        (5_500, 5_499),
    ] {
        let maximum = target - 1;
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let frame = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
        let result = decode_complete_policy_replace_frame(
            &frame,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )
        .map(|_| ());
        assert_eq!(
            result,
            Err(CompletePolicyRequestError::Encoding(
                CompletePolicyEncodingError::Schema(PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum,
                })
            ))
        );
        assert_eq!((charge.g(), charge.p(), charge.total()), (prior, 0, prior));
    }
    assert_replace_decode_prior_list_peak()?;
    Ok(())
}

#[test]
fn complete_replace_decode_preserves_verification_and_reconstruction_refusal_priority() -> TestResult
{
    let frame = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let wrong = profile(100_000_001)?;
    let result = decode_complete_policy_replace_frame(
        &[],
        wrong,
        AdmissionLimits::new(1_048_576, 1, 1_000)?,
        &mut charge,
    );
    assert_eq!(
        result.err().ok_or("profile first")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    let result = decode_complete_policy_replace_frame(
        &frame,
        limits,
        AdmissionLimits::new(1_048_576, 134, 1_000)?,
        &mut charge,
    );
    assert_eq!(
        result.err().ok_or("raw cap before reconstruction")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    assert_replace_decode_errors(limits, &mut charge)?;
    assert_replace_decode_bounds(limits, &mut charge)?;
    assert_replace_decode_header_limits()?;
    Ok(())
}

pub(super) fn assert_replace_decoded(
    decoded: &DecodedCompletePolicyReplaceFrame<'_>,
    frame: &[u8],
    expected: &CompletePolicyData,
    guard: Option<u64>,
) -> TestResult {
    let mut cursor = 7;
    let mut ids = Vec::new();
    let mut isolation = b"FWSC\0\x01".to_vec();
    for index in 0..4 {
        let length = usize::try_from(u32::from_be_bytes(
            frame[cursor..cursor + 4].as_chunks::<4>().0[0],
        ))?;
        if index < 3 {
            isolation.extend_from_slice(&frame[cursor..cursor + 4]);
        }
        cursor += 4;
        ids.push(&frame[cursor..cursor + length]);
        if index < 3 {
            isolation.extend_from_slice(ids[index]);
        }
        cursor += length;
    }
    let original = &frame[cursor + 56..];
    let digest = PreparedDigest::new(Sha256::digest(frame).into());
    let original_digest: [u8; 32] = Sha256::digest(original).into();
    let physical = u64::from_be_bytes(frame[cursor + 34..cursor + 42].as_chunks::<8>().0[0]);
    let logical = u16::from_be_bytes(frame[cursor + 42..cursor + 44].as_chunks::<2>().0[0]);
    let wall = u64::from_be_bytes(frame[cursor + 44..cursor + 52].as_chunks::<8>().0[0]);
    assert!(decoded.input().data() == expected);
    assert_eq!(decoded.input().expected_generation(), guard);
    assert_eq!(decoded.encoded(), frame);
    assert_eq!(decoded.original_intent(), original);
    assert_eq!(decoded.digest(), digest);
    assert_eq!(decoded.isolation_scope(), isolation);
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
        assert_eq!(header.intent.version(), 2);
        assert_eq!(*header.intent.digest(), original_digest);
        assert_eq!(
            header.stamp,
            CommittedTime::new(HlcTimestamp::new(physical, logical)?, wall)?
        );
        assert_eq!(header.identity, EntryIdentity::new(log, digest));
        assert_eq!(header.encoded_entry, frame);
        assert_eq!(header.encoded_isolation_scope, isolation);
    }
    Ok(())
}

fn assert_replace_decode_business(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (field, start, end, payload) in [
        (0, 36, 41, &[0, 0, 0, 0][..]),
        (1, 41, 51, &[1, 0, 0, 0, 0, 0][..]),
        (2, 41, 51, &[1, 0, 0, 0, 0, 2, 0, 255][..]),
        (3, 147, 152, &[0, 0, 0, 0][..]),
    ] {
        let mut expected = present_data();
        match field {
            0 => expected.workflow_type.clear(),
            1 => expected.version = Some(PolicyVersion::Bytes(Vec::new())),
            2 => expected.version = Some(PolicyVersion::Bytes(vec![0, 255])),
            3 => expected
                .indexed_attributes
                .as_mut()
                .ok_or("indexed fixture")?[0]
                .clear(),
            _ => return Err("unknown business original".into()),
        }
        let mut intent = PRESENT_INTENT_BYTES.to_vec();
        drop(intent.splice(start..end, payload.iter().copied()));
        let frame = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let decoded = decode_complete_policy_replace_frame(
            &frame,
            limits,
            AdmissionLimits::default(),
            charge,
        )?;
        assert_replace_decoded(&decoded, &frame, &expected, None)?;
    }
    Ok(())
}

fn assert_replace_decode_prior_list_peak() -> TestResult {
    let limits = profile(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let mut data = minimal_data();
    data.indexed_attributes = Some(vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]);
    let mut original = MINIMAL_INTENT_BYTES.to_vec();
    drop(original.splice(
        49..50,
        [
            1, 0, 0, 0, 3, 0, 0, 0, 1, b'a', 0, 0, 0, 1, b'b', 0, 0, 0, 1, b'c',
        ],
    ));
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    let expected_seed = replace_frame_literal(&original, [b"t", b"g", b"r", b"q"], stamp)?;
    let seed = prepare_complete_policy_replace_frame(
        super::super::input(data, None)?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!(seed.as_bytes(), expected_seed);
    assert_eq!(
        seed.digest(),
        PreparedDigest::new(Sha256::digest(&expected_seed).into())
    );
    // B71/N13/Q643; 384N+6B+4H+1920+Q, with actual three-name P192.
    assert_eq!((charge.g(), charge.p()), (8_313, 192));
    let frame = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
    let decoded = decode_complete_policy_replace_frame(
        &frame,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_replace_decoded(&decoded, &frame, &minimal_data(), None)?;
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (13_813, 192, 14_005)
    );
    assert_eq!(seed.as_bytes(), expected_seed);
    Ok(())
}

fn assert_replace_decode_bounds(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    let mut intent = MINIMAL_INTENT_BYTES.to_vec();
    let mut raw = 262_145_u32.to_be_bytes().to_vec();
    raw.extend_from_slice(&vec![b'x'; 262_145]);
    drop(intent.splice(36..41, raw));
    let frame = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
    assert_eq!(
        decode_complete_policy_replace_frame(&frame, limits, AdmissionLimits::default(), charge,)
            .err()
            .ok_or("RAW cap before ownership")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
    );
    let mut intent = MINIMAL_INTENT_BYTES.to_vec();
    let mut names = vec![1, 0, 0, 0, 4];
    for name in b"abcd" {
        names.extend_from_slice(&[0, 0, 0, 1, *name]);
    }
    drop(intent.splice(49..50, names));
    let frame = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
    assert_eq!(
        decode_complete_policy_replace_frame(&frame, limits, AdmissionLimits::default(), charge,)
            .err()
            .ok_or("count cap before ownership")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ItemLimit)
    );
    for governance in [false, true] {
        let mut intent = if governance {
            PRESENT_INTENT_BYTES.to_vec()
        } else {
            MINIMAL_INTENT_BYTES.to_vec()
        };
        let (start, end) = if governance { (167, 182) } else { (49, 50) };
        drop(intent.splice(
            start..end,
            [1, 0, 0, 0, 2, 0, 0, 0, 1, b'a', 0, 0, 0, 1, b'a'],
        ));
        let frame = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        assert_eq!(
            decode_complete_policy_replace_frame(
                &frame,
                limits,
                AdmissionLimits::default(),
                charge,
            )
            .err()
            .ok_or("duplicate before ownership")?,
            CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed)
        );
    }
    let mut frame = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
    frame[83 + 34] = b'u';
    assert_eq!(
        decode_complete_policy_replace_frame(&frame, limits, AdmissionLimits::default(), charge,)
            .err()
            .ok_or("tenant binding before ownership")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed)
    );
    let mut frame = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
    frame[79..83].copy_from_slice(&1_048_577_u32.to_be_bytes());
    let short = profile(851)?;
    let mut descriptor_charge = PolicySchemaCharge::new(short);
    assert_eq!(
        decode_complete_policy_replace_frame(
            &frame,
            short,
            AdmissionLimits::default(),
            &mut descriptor_charge,
        )
        .err()
        .ok_or("whole region cap before availability")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
    );
    assert_eq!((descriptor_charge.g(), descriptor_charge.p()), (851, 0));
    Ok(())
}
