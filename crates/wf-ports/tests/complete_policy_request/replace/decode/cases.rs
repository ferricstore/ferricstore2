//! Fixed reconstruction accounts, original governance and consumed Header controls.

use sha2::{Digest, Sha256};
use wf_domain::policy::BackoffKind;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyRequestError, DenialAuditMode, GovernanceMode,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    decode_complete_policy_replace_frame_input, restore_complete_policy,
    restore_complete_policy_replace_frame_header, scan_complete_policy_replace_frame,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_ports::log::decode_complete_policy_replace_frame;
use wf_types::version::{CommittedTime, HlcTimestamp};

use super::super::super::{
    MINIMAL_INTENT_BYTES, PRESENT_INTENT_BYTES, TestResult, minimal_data, present_data, profile,
};
use super::super::boundaries::replace_frame_count_field;
use super::super::cases::replace_frame_governance_list;
use super::super::literals::{
    FROZEN_COMPLETE_REPLACE_FRAMES, replace_frame_hex, replace_frame_literal,
};
use super::assert_replace_decoded;

pub(super) fn assert_replace_decode_account(
    maximum: u64,
    attempts: u64,
    held_peak: bool,
    full: bool,
) -> TestResult {
    let limits = profile(maximum)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let mut seed_data = minimal_data();
    seed_data.indexed_attributes = Some(vec![b"a".to_vec()]);
    let seed = if held_peak {
        Some(restore_complete_policy(1, seed_data, limits, &mut charge)?)
    } else {
        None
    };
    let initial = if held_peak { 1_928 } else { 0 };
    let initial_peak = if held_peak { 64 } else { 0 };
    assert_eq!((charge.g(), charge.p()), (initial, initial_peak));
    let frame = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[usize::from(full)].0)?;
    let expected = if full { present_data() } else { minimal_data() };
    let (fee, peak, last) = if full {
        (43_062, 128, 8)
    } else {
        (5_500, initial_peak, 1)
    };
    let mut outputs = Vec::new();
    for attempt in 1..=attempts {
        let target = initial + fee * attempt + peak;
        let result = decode_complete_policy_replace_frame(
            &frame,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )
        .map(|decoded| outputs.push(decoded));
        if maximum < target {
            assert_eq!(
                result,
                Err(CompletePolicyRequestError::Encoding(
                    CompletePolicyEncodingError::Schema(PolicySchemaError::WorkBytes {
                        bytes: target,
                        maximum,
                    })
                ))
            );
            assert_eq!(
                (charge.g(), charge.p(), charge.total()),
                (target - peak - last, peak, target - last)
            );
        } else {
            result?;
            assert_eq!(
                (charge.g(), charge.p(), charge.total()),
                (target - peak, peak, target)
            );
        }
    }
    let target = initial + fee * attempts + peak;
    assert_eq!(
        u64::try_from(outputs.len())?,
        attempts - u64::from(maximum < target)
    );
    for decoded in &outputs {
        assert_replace_decoded(decoded, &frame, &expected, None)?;
    }
    if let Some(seed) = seed {
        let mut expected = minimal_data();
        expected.indexed_attributes = Some(vec![b"a".to_vec()]);
        assert!(seed.data() == &expected);
        assert_eq!(seed.generation(), 1);
    }
    assert_eq!(charge.p(), peak);
    Ok(())
}

pub(super) fn assert_replace_decode_domain_moves() -> TestResult {
    let limits = profile(100_000_000)?;
    let stamp = CommittedTime::new(HlcTimestamp::new(99, 7)?, 98)?;
    for guard in [None, Some(0), Some(u64::MAX)] {
        let mut intent = MINIMAL_INTENT_BYTES.to_vec();
        if let Some(guard) = guard {
            let mut optional = vec![1];
            optional.extend_from_slice(&guard.to_be_bytes());
            drop(intent.splice(35..36, optional));
        }
        let frame = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let mut header_charge = PolicySchemaCharge::new(limits);
        let scanned = scan_complete_policy_replace_frame(
            &frame,
            limits,
            AdmissionLimits::default(),
            &mut header_charge,
        )?;
        let header = restore_complete_policy_replace_frame_header(
            scanned,
            Sha256::digest(&intent).into(),
            limits,
            AdmissionLimits::default(),
            &mut header_charge,
        )?;
        let extra = u64::from(guard.is_some());
        assert_eq!(
            (header_charge.g(), header_charge.p()),
            (4_423 + 152 * extra, 0)
        );
        assert_eq!(header.encoded(), frame);
        assert_eq!(header.original_intent(), intent);
        let pointers = [
            header.scope().tenant().as_bytes().as_ptr(),
            header.scope().group().as_bytes().as_ptr(),
            header.scope().recovery().as_bytes().as_ptr(),
            header.request().as_bytes().as_ptr(),
        ];
        let mut reconstruction = PolicySchemaCharge::new(limits);
        let input = decode_complete_policy_replace_frame_input(
            header,
            limits,
            AdmissionLimits::default(),
            &mut reconstruction,
        )?;
        assert!(input.data() == &minimal_data());
        assert_eq!(input.expected_generation(), guard);
        assert_eq!(input.scope().tenant().as_bytes(), b"t");
        assert_eq!(input.scope().group().as_bytes(), b"g");
        assert_eq!(input.scope().recovery().as_bytes(), b"r");
        assert_eq!(input.request().as_bytes(), b"q");
        assert_eq!(input.stamp(), stamp);
        assert_eq!(
            (reconstruction.g(), reconstruction.p()),
            (1_077 + 136 * extra, 0)
        );
        assert_eq!(
            (header_charge.g(), header_charge.p()),
            (4_423 + 152 * extra, 0)
        );
        let mut raw_charge = PolicySchemaCharge::new(limits);
        let decoded = decode_complete_policy_replace_frame(
            &frame,
            limits,
            AdmissionLimits::default(),
            &mut raw_charge,
        )?;
        assert_replace_decoded(&decoded, &frame, &minimal_data(), guard)?;
        assert_eq!((raw_charge.g(), raw_charge.p()), (5_500 + 288 * extra, 0));
        assert_eq!(
            [
                input.scope().tenant().as_bytes().as_ptr(),
                input.scope().group().as_bytes().as_ptr(),
                input.scope().recovery().as_bytes().as_ptr(),
                input.request().as_bytes().as_ptr()
            ],
            pointers
        );
        let type_start = 83 + 40 + usize::from(guard.is_some()) * 8;
        assert_ne!(
            input.data().workflow_type.as_ptr(),
            frame[type_start..].as_ptr()
        );
    }
    Ok(())
}

pub(super) fn assert_replace_decode_enums(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (kind, tag) in [
        (BackoffKind::None, 0),
        (BackoffKind::Fixed, 1),
        (BackoffKind::Linear, 2),
        (BackoffKind::Exponential, 3),
    ] {
        let mut expected = present_data();
        expected
            .retry
            .as_mut()
            .and_then(|retry| retry.backoff.as_mut())
            .ok_or("backoff fixture")?
            .kind = kind;
        let mut intent = PRESENT_INTENT_BYTES.to_vec();
        intent[65] = tag;
        let frame = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let decoded = decode_complete_policy_replace_frame(
            &frame,
            limits,
            AdmissionLimits::default(),
            charge,
        )?;
        assert_replace_decoded(&decoded, &frame, &expected, None)?;
    }
    for tag in 0..3 {
        for field in [0, 1] {
            let mut expected = present_data();
            let governance = expected.governance.as_mut().ok_or("governance fixture")?;
            let offset = if field == 0 {
                governance.mode = Some(
                    [
                        GovernanceMode::Minimal,
                        GovernanceMode::Ledger,
                        GovernanceMode::Full,
                    ][tag],
                );
                165
            } else {
                governance.audit.as_mut().ok_or("audit fixture")?.denials = Some(
                    [
                        DenialAuditMode::Off,
                        DenialAuditMode::Sampled,
                        DenialAuditMode::All,
                    ][tag],
                );
                193
            };
            let mut intent = PRESENT_INTENT_BYTES.to_vec();
            intent[offset] = u8::try_from(tag)?;
            let frame = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let decoded = decode_complete_policy_replace_frame(
                &frame,
                limits,
                AdmissionLimits::default(),
                charge,
            )?;
            assert_replace_decoded(&decoded, &frame, &expected, None)?;
        }
    }
    Ok(())
}

pub(super) fn assert_replace_decode_list_order(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for state in [false, true] {
        let mut expected = present_data();
        replace_frame_governance_list(&mut expected, 6, state)?;
        let mut intent = PRESENT_INTENT_BYTES.to_vec();
        intent[147..157].copy_from_slice(b"\0\0\0\x01z\0\0\0\x01a");
        let mut governance = PRESENT_INTENT_BYTES[163..].to_vec();
        for (start, end, wire) in [
            (167, 182, &b"\x01\0\0\0\x02\0\0\0\x01z\0\0\0\x01a"[..]),
            (339, 354, &b"\x01\0\0\0\x02\0\0\0\x01z\0\0\0\x01a"[..]),
            (365, 375, &b"\x01\0\0\0\x01\0\0\0\x01a"[..]),
            (375, 385, &b"\x01\0\0\0\x01\0\0\0\x01a"[..]),
        ] {
            governance[start - 163..end - 163].copy_from_slice(wire);
        }
        if state {
            drop(intent.splice(134..142, governance));
        } else {
            drop(intent.splice(163..394, governance));
        }
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

pub(super) fn assert_replace_decode_map_order(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for case in [0, 8, 9, 10] {
        let mut expected = if case == 0 {
            minimal_data()
        } else {
            present_data()
        };
        replace_frame_count_field(&mut expected, case, 2)?;
        let (start, end, suffix) = match case {
            0 => (45, 49, &[0; 4][..]),
            8 => (194, 223, &PRESENT_INTENT_BYTES[204..223]),
            9 => (223, 266, &PRESENT_INTENT_BYTES[233..266]),
            10 => (266, 354, &PRESENT_INTENT_BYTES[276..354]),
            _ => return Err("unknown ordered map".into()),
        };
        let source = if case == 0 {
            MINIMAL_INTENT_BYTES
        } else {
            PRESENT_INTENT_BYTES
        };
        let mut entries = Vec::new();
        for name in 0_u16..2 {
            entries.extend_from_slice(b"\0\0\0\x02");
            entries.extend_from_slice(&name.to_be_bytes());
            entries.extend_from_slice(suffix);
        }
        let mut wire = if case == 0 { Vec::new() } else { vec![1] };
        wire.extend_from_slice(&2_u32.to_be_bytes());
        wire.extend_from_slice(&entries);
        let mut intent = source.to_vec();
        drop(intent.splice(start..end, wire));
        let frame = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let decoded = decode_complete_policy_replace_frame(
            &frame,
            limits,
            AdmissionLimits::default(),
            charge,
        )?;
        assert_replace_decoded(&decoded, &frame, &expected, None)?;
        for repeated in [false, true] {
            let stride = 6 + suffix.len();
            let mut wire = if case == 0 { Vec::new() } else { vec![1] };
            wire.extend_from_slice(&2_u32.to_be_bytes());
            wire.extend_from_slice(&entries[stride..]);
            wire.extend_from_slice(if repeated {
                &entries[stride..]
            } else {
                &entries[..stride]
            });
            let mut intent = source.to_vec();
            drop(intent.splice(start..end, wire));
            let frame = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            assert_eq!(
                decode_complete_policy_replace_frame(
                    &frame,
                    limits,
                    AdmissionLimits::default(),
                    charge,
                )
                .err()
                .ok_or("map grammar precedes reconstruction")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed)
            );
        }
    }
    Ok(())
}

pub(super) fn assert_replace_decode_errors(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let minimum = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
    for end in 0..minimum.len() {
        assert_eq!(
            decode_complete_policy_replace_frame(
                &minimum[..end],
                limits,
                AdmissionLimits::default(),
                charge,
            )
            .err()
            .ok_or("proper prefix is incomplete")?,
            CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed)
        );
    }
    for (offset, value, error) in [
        (0, b'X', CompletePolicyEncodingError::Malformed),
        (5, 2, CompletePolicyEncodingError::UnsupportedVersion),
        (6, 5, CompletePolicyEncodingError::UnsupportedTag),
        (28, 1, CompletePolicyEncodingError::UnsupportedVersion),
        (83, b'X', CompletePolicyEncodingError::Malformed),
        (118, 2, CompletePolicyEncodingError::InvalidOptionality),
        (124, 2, CompletePolicyEncodingError::InvalidOptionality),
    ] {
        let mut frame = minimum.clone();
        frame[offset] = value;
        assert_eq!(
            decode_complete_policy_replace_frame(
                &frame,
                limits,
                AdmissionLimits::default(),
                charge,
            )
            .err()
            .ok_or("structural priority")?,
            CompletePolicyRequestError::Encoding(error)
        );
    }
    let full = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[1].0)?;
    for (offset, value) in [
        (42, 2),
        (65, 4),
        (114, 2),
        (165, 3),
        (188, 2),
        (191, 2),
        (193, 3),
        (213, 2),
        (250, 2),
    ] {
        let mut frame = full.clone();
        frame[83 + offset] = value;
        assert_eq!(
            decode_complete_policy_replace_frame(
                &frame,
                limits,
                AdmissionLimits::default(),
                charge,
            )
            .err()
            .ok_or("enum precedes stale SHA")?,
            CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::UnsupportedTag)
        );
    }
    let mut frame = minimum.clone();
    frame.push(0);
    assert_eq!(
        decode_complete_policy_replace_frame(&frame, limits, AdmissionLimits::default(), charge,)
            .err()
            .ok_or("trailing frame")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::TrailingBytes)
    );
    let mut frame = minimum;
    frame[29] ^= 1;
    let short = profile(2_045)?;
    let mut digest_charge = PolicySchemaCharge::new(short);
    assert_eq!(
        decode_complete_policy_replace_frame(
            &frame,
            short,
            AdmissionLimits::default(),
            &mut digest_charge,
        )
        .err()
        .ok_or("digest before any Data owner")?,
        CompletePolicyRequestError::IntentMismatch
    );
    assert_eq!((digest_charge.g(), digest_charge.p()), (2_045, 0));
    Ok(())
}

pub(super) fn assert_replace_decode_header_limits() -> TestResult {
    let limits = profile(100_000_000)?;
    let wrong = profile(100_000_001)?;
    let frame = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
    for (supplied, maximum, error) in [
        (limits, 134, CompletePolicyEncodingError::ByteLimit),
        (
            wrong,
            1,
            CompletePolicyEncodingError::Schema(PolicySchemaError::ProfileMismatch),
        ),
    ] {
        let mut header_charge = PolicySchemaCharge::new(limits);
        let scanned = scan_complete_policy_replace_frame(
            &frame,
            limits,
            AdmissionLimits::default(),
            &mut header_charge,
        )?;
        let header = restore_complete_policy_replace_frame_header(
            scanned,
            Sha256::digest(MINIMAL_INTENT_BYTES).into(),
            limits,
            AdmissionLimits::default(),
            &mut header_charge,
        )?;
        assert_eq!((header_charge.g(), header_charge.p()), (4_423, 0));
        let mut reconstruction = PolicySchemaCharge::new(limits);
        let result = decode_complete_policy_replace_frame_input(
            header,
            supplied,
            AdmissionLimits::new(1_048_576, maximum, 1_000)?,
            &mut reconstruction,
        );
        assert_eq!(
            result
                .err()
                .ok_or("consumed math Header cannot skip checks")?,
            CompletePolicyRequestError::Encoding(error)
        );
        assert_eq!((reconstruction.g(), reconstruction.p()), (0, 0));
        assert_eq!((header_charge.g(), header_charge.p()), (4_423, 0));
    }
    Ok(())
}
