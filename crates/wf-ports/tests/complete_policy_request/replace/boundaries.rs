//! Full scope/region/count/RAW bounds and structural refusal priority.

use super::super::{
    MINIMAL_INTENT_BYTES, PRESENT_INTENT_BYTES, TestResult, minimal_data, present_data,
};
use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{CompletePolicyData, CompleteStatePolicyData};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyRequestError, PolicySchemaCharge, PolicySchemaLimits,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::PreparedDigest;
use wf_ports::log::{prepare_complete_policy_replace_frame, verify_complete_policy_replace_frame};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::cases::{replace_frame_input, replace_frame_raw_field};
use super::literals::{
    FROZEN_COMPLETE_REPLACE_FRAMES, assert_replace_frame_map_order, assert_replace_frame_verified,
    replace_frame_hex, replace_frame_literal, replace_frame_rule_count,
};

pub(super) fn assert_replace_frame_boundary(
    case: usize,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    if case != 0 {
        return match case {
            1 => assert_replace_frame_count_caps(limits, charge),
            2 => assert_replace_frame_global_bytes(limits, charge),
            3 => assert_replace_frame_malformed(limits, charge),
            _ => Err("unknown independent boundary case".into()),
        };
    }
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for length in [8_172, 8_173] {
        let tenant = vec![b't'; length];
        let mut intent = MINIMAL_INTENT_BYTES.to_vec();
        let mut raw = u32::try_from(length)?.to_be_bytes().to_vec();
        raw.extend_from_slice(&tenant);
        drop(intent.splice(30..35, raw));
        let ids = [&tenant[..], b"g", b"r", b"q"];
        let expected = replace_frame_literal(&intent, ids, stamp)?;
        let before = charge.g();
        let result = prepare_complete_policy_replace_frame(
            replace_frame_input(minimal_data(), None, ids, stamp)?,
            limits,
            AdmissionLimits::default(),
            charge,
        );
        if length == 8_173 {
            assert_eq!(
                result.err().ok_or("scope +1 producer")?,
                CompletePolicyRequestError::ScopeLimit
            );
            assert_eq!(charge.g(), before + 128);
            let before = charge.g();
            assert_eq!(
                verify_complete_policy_replace_frame(
                    &expected,
                    limits,
                    AdmissionLimits::default(),
                    charge
                )
                .err()
                .ok_or("scope +1 verifier")?,
                CompletePolicyRequestError::ScopeLimit
            );
            assert_eq!(charge.g(), before + 8_706);
        } else {
            let prepared = result?;
            assert_eq!(prepared.as_bytes(), expected);
            assert_eq!(charge.g(), before + 86_322);
            let before = charge.g();
            let verified = verify_complete_policy_replace_frame(
                prepared.as_bytes(),
                limits,
                AdmissionLimits::default(),
                charge,
            )?;
            assert_eq!(charge.g(), before + 94_304);
            assert_replace_frame_verified(
                &verified,
                &expected,
                LogIdentity::new(1, 2, AppliedPosition::new(3)),
            );
        }
    }

    Ok(())
}

pub(super) fn assert_replace_frame_raw_limits(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (field, (start, end, cap, refusal, listed)) in [
        (36, 41, 262_144, 548, false),
        (41, 51, 262_144, 811, false),
        (84, 89, 262_144, 1_748, false),
        (108, 113, 262_144, 2_668, false),
        (147, 152, 64, 4_371, true),
        (158, 163, 64, 4_894, false),
        (172, 177, 65_535, 6_060, true),
        (261, 266, 65_535, 9_349, false),
        (344, 349, 256, 11_352, true),
        (199, 204, 65_535, 7_751, false),
        (228, 233, 65_535, 8_676, false),
        (271, 276, 65_535, 9_871, false),
        (360, 365, 65_535, 12_392, true),
        (370, 375, 65_535, 12_914, true),
        (380, 385, 65_535, 13_436, true),
    ]
    .into_iter()
    .enumerate()
    {
        for length in [cap, cap + 1] {
            let payload = vec![b'x'; length];
            let mut wire = u32::try_from(length)?.to_be_bytes().to_vec();
            wire.extend_from_slice(&payload);
            if field == 1 {
                drop(wire.splice(0..0, [1, 0]));
            }
            let mut intent = PRESENT_INTENT_BYTES.to_vec();
            drop(intent.splice(start..end, wire));
            let expected = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let mut data = present_data();
            replace_frame_raw_field(&mut data, field, payload)?;
            let before = (charge.g(), charge.p());
            let result = prepare_complete_policy_replace_frame(
                super::super::input(data, None)?,
                limits,
                AdmissionLimits::default(),
                charge,
            );
            if length > cap {
                assert_eq!(
                    result.err().ok_or("RAW +1 refusal")?,
                    CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
                );
                assert_eq!((charge.g(), charge.p()), (before.0 + refusal, before.1));
                let result = verify_complete_policy_replace_frame(
                    &expected,
                    limits,
                    AdmissionLimits::default(),
                    charge,
                );
                assert_eq!(
                    result.err().ok_or("RAW +1 verifier")?,
                    CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
                );
            } else {
                let prepared = result?;
                let length = u64::try_from(length)?;
                let fee = if field == 1 {
                    47_609 + 6 * length
                } else {
                    47_249 + (if listed { 7 } else { 6 }) * (length - 1)
                };
                assert_eq!(
                    (charge.g(), charge.p()),
                    (before.0 + fee, before.1.max(128))
                );
                assert_eq!(prepared.as_bytes(), expected);
                assert_eq!(
                    prepared.digest(),
                    PreparedDigest::new(Sha256::digest(&expected).into())
                );
                let verified = verify_complete_policy_replace_frame(
                    prepared.as_bytes(),
                    limits,
                    AdmissionLimits::default(),
                    charge,
                )?;
                assert_replace_frame_verified(
                    &verified,
                    &expected,
                    LogIdentity::new(1, 2, AppliedPosition::new(3)),
                );
            }
        }
    }
    Ok(())
}

pub(super) fn replace_frame_count_field(
    data: &mut CompletePolicyData,
    case: usize,
    count: usize,
) -> TestResult {
    let names = (0..count)
        .map(|name| Ok(u16::try_from(name)?.to_be_bytes().to_vec()))
        .collect::<TestResult<Vec<_>>>()?;
    if case == 0 {
        data.states = names
            .into_iter()
            .map(|name| {
                (
                    name,
                    CompleteStatePolicyData {
                        mode: None,
                        retry: None,
                        retention: None,
                        governance: None,
                    },
                )
            })
            .collect();
        return Ok(());
    }
    if case == 1 {
        data.indexed_attributes = Some(names);
        return Ok(());
    }
    let governance = data.governance.as_mut().ok_or("governance fixture")?;
    if case >= 8 {
        return replace_frame_rule_count(governance, case, names);
    }
    let target = match case {
        2 => {
            &mut governance
                .effects
                .as_mut()
                .ok_or("effects fixture")?
                .allowed
        }
        3 => &mut governance.effects.as_mut().ok_or("effects fixture")?.denied,
        4 => {
            &mut governance
                .circuits
                .as_mut()
                .and_then(|rules| rules.get_mut(b"c".as_slice()))
                .ok_or("circuit fixture")?
                .error_classes
        }
        5 => {
            &mut governance
                .approvals
                .as_mut()
                .ok_or("approvals fixture")?
                .effects
        }
        6 => {
            &mut governance
                .approvals
                .as_mut()
                .ok_or("approvals fixture")?
                .states
        }
        7 => {
            &mut governance
                .approvals
                .as_mut()
                .ok_or("approvals fixture")?
                .assignees
        }
        _ => return Err("unknown list count fixture".into()),
    };
    *target = Some(names);
    Ok(())
}

pub(super) fn assert_replace_frame_count_caps(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (case, (start, end, maximum, suffix)) in [
        (45, 49, limits.policy_limits().max_states(), &[0; 4][..]),
        (49, 50, 3, &[][..]),
        (167, 182, 1_000, &[][..]),
        (182, 187, 1_000, &[][..]),
        (339, 354, 1_000, &[][..]),
        (355, 365, 1_000, &[][..]),
        (365, 375, 1_000, &[][..]),
        (375, 385, 1_000, &[][..]),
        (194, 223, 1_000, &PRESENT_INTENT_BYTES[204..223]),
        (223, 266, 1_000, &PRESENT_INTENT_BYTES[233..266]),
        (266, 354, 1_000, &PRESENT_INTENT_BYTES[276..354]),
    ]
    .into_iter()
    .enumerate()
    {
        for count in [maximum, maximum + 1] {
            let mut wire = if case == 0 { Vec::new() } else { vec![1] };
            wire.extend_from_slice(&u32::try_from(count)?.to_be_bytes());
            for name in 0..count {
                wire.extend_from_slice(b"\0\0\0\x02");
                wire.extend_from_slice(&u16::try_from(name)?.to_be_bytes());
                wire.extend_from_slice(suffix);
            }
            let source = if case < 2 {
                MINIMAL_INTENT_BYTES
            } else {
                PRESENT_INTENT_BYTES
            };
            let mut intent = source.to_vec();
            drop(intent.splice(start..end, wire));
            let expected = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let mut data = if case < 2 {
                minimal_data()
            } else {
                present_data()
            };
            replace_frame_count_field(&mut data, case, count)?;
            let result = prepare_complete_policy_replace_frame(
                super::super::input(data, None)?,
                limits,
                AdmissionLimits::default(),
                charge,
            );
            if count > maximum {
                assert_eq!(
                    result.err().ok_or("producer count +1")?,
                    CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ItemLimit)
                );
                let result = verify_complete_policy_replace_frame(
                    &expected,
                    limits,
                    AdmissionLimits::default(),
                    charge,
                );
                assert_eq!(
                    result.err().ok_or("verifier count +1")?,
                    CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ItemLimit)
                );
            } else {
                let prepared = result?;
                assert_eq!(prepared.as_bytes(), expected);
                assert_eq!(
                    prepared.digest(),
                    PreparedDigest::new(Sha256::digest(&expected).into())
                );
                let verified = verify_complete_policy_replace_frame(
                    prepared.as_bytes(),
                    limits,
                    AdmissionLimits::default(),
                    charge,
                )?;
                assert_replace_frame_verified(
                    &verified,
                    &expected,
                    LogIdentity::new(1, 2, AppliedPosition::new(3)),
                );
            }
        }
    }
    Ok(())
}

pub(super) fn assert_replace_frame_global_bytes(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for length in [262_060, 262_061] {
        let mut data = minimal_data();
        let mut intent = MINIMAL_INTENT_BYTES.to_vec();
        intent[45..49].copy_from_slice(&4_u32.to_be_bytes());
        let mut entries = Vec::new();
        for first in 0..4 {
            let count = if first == 3 { length } else { 262_144 };
            let mut key = vec![0; count];
            key[0] = first;
            entries.extend_from_slice(&u32::try_from(count)?.to_be_bytes());
            entries.extend_from_slice(&key);
            entries.extend_from_slice(&[0; 4]);
            data.states.insert(
                key,
                CompleteStatePolicyData {
                    mode: None,
                    retry: None,
                    retention: None,
                    governance: None,
                },
            );
        }
        drop(intent.splice(49..49, entries));
        let expected = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let before = charge.g();
        let result = prepare_complete_policy_replace_frame(
            super::super::input(data, None)?,
            limits,
            AdmissionLimits::default(),
            charge,
        );
        if length == 262_061 {
            assert_eq!(
                result.err().ok_or("whole intent +1")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
            );
            assert_eq!(charge.g(), before + 1_050_240);
            let before = charge.g();
            let result = verify_complete_policy_replace_frame(
                &expected,
                limits,
                AdmissionLimits::default(),
                charge,
            );
            assert_eq!(
                result.err().ok_or("whole intent +1 verifier")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
            );
            assert_eq!(charge.g(), before + 851);
        } else {
            let prepared = result?;
            assert_eq!(intent.len(), 1_048_576);
            assert_eq!(prepared.as_bytes(), expected);
            assert_eq!(charge.g(), before + 6_299_340);
            let verified = verify_complete_policy_replace_frame(
                prepared.as_bytes(),
                limits,
                AdmissionLimits::default(),
                charge,
            )?;
            assert_replace_frame_verified(
                &verified,
                &expected,
                LogIdentity::new(1, 2, AppliedPosition::new(3)),
            );
        }
    }

    Ok(())
}

pub(super) fn assert_replace_frame_malformed(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let minimum = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
    for end in 0..minimum.len() {
        assert!(
            verify_complete_policy_replace_frame(
                &minimum[..end],
                limits,
                AdmissionLimits::default(),
                charge
            )
            .is_err()
        );
    }
    let mut bytes = minimum.clone();
    bytes.push(0);
    assert_eq!(
        verify_complete_policy_replace_frame(&bytes, limits, AdmissionLimits::default(), charge)
            .err()
            .ok_or("trailing frame")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::TrailingBytes)
    );
    let mut bytes = minimum.clone();
    bytes[34 + 83] = b'u';
    assert_eq!(
        verify_complete_policy_replace_frame(&bytes, limits, AdmissionLimits::default(), charge)
            .err()
            .ok_or("tenant mismatch")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed)
    );
    let mut bytes = minimum.clone();
    bytes[29] ^= 1;
    assert_eq!(
        verify_complete_policy_replace_frame(&bytes, limits, AdmissionLimits::default(), charge)
            .err()
            .ok_or("digest mismatch")?,
        CompletePolicyRequestError::IntentMismatch
    );
    for (declared, error) in [
        (51_u32, CompletePolicyEncodingError::TrailingBytes),
        (53, CompletePolicyEncodingError::Malformed),
        (1_048_577, CompletePolicyEncodingError::ByteLimit),
    ] {
        let mut bytes = minimum.clone();
        bytes[79..83].copy_from_slice(&declared.to_be_bytes());
        assert_eq!(
            verify_complete_policy_replace_frame(
                &bytes,
                limits,
                AdmissionLimits::default(),
                charge
            )
            .err()
            .ok_or("declared region refusal")?,
            CompletePolicyRequestError::Encoding(error)
        );
    }
    assert_replace_frame_tags(limits, charge)?;
    assert_replace_frame_map_order(limits, charge)?;
    Ok(())
}

pub(super) fn assert_replace_frame_tags(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let minimum = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
    for (offset, value, error) in [
        (0, b'X', CompletePolicyEncodingError::Malformed),
        (5, 2, CompletePolicyEncodingError::UnsupportedVersion),
        (6, 5, CompletePolicyEncodingError::UnsupportedTag),
        (28, 1, CompletePolicyEncodingError::UnsupportedVersion),
        (83, b'X', CompletePolicyEncodingError::Malformed),
        (118, 2, CompletePolicyEncodingError::InvalidOptionality),
        (124, 2, CompletePolicyEncodingError::InvalidOptionality),
    ] {
        let mut bytes = minimum.clone();
        bytes[offset] = value;
        let result = verify_complete_policy_replace_frame(
            &bytes,
            limits,
            AdmissionLimits::default(),
            charge,
        );
        assert_eq!(
            result.err().ok_or("malformed mutation")?,
            CompletePolicyRequestError::Encoding(error)
        );
    }
    let mut full = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[1].0)?;
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
        let previous = full[83 + offset];
        full[83 + offset] = value;
        let result =
            verify_complete_policy_replace_frame(&full, limits, AdmissionLimits::default(), charge);
        assert_eq!(
            result.err().ok_or("enum precedes stale digest")?,
            CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::UnsupportedTag)
        );
        full[83 + offset] = previous;
    }
    for start in [61, 71] {
        let mut bytes = minimum.clone();
        bytes[start..start + 8].copy_from_slice(&u64::MAX.to_be_bytes());
        assert_eq!(
            verify_complete_policy_replace_frame(
                &bytes,
                limits,
                AdmissionLimits::default(),
                charge
            )
            .err()
            .ok_or("invalid trusted stamp")?,
            CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed)
        );
    }
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for field in 1..4 {
        for length in [256, 257] {
            let id = vec![b'x'; length];
            let mut ids = [&b"t"[..], &b"g"[..], &b"r"[..], &b"q"[..]];
            ids[field] = &id;
            let expected = replace_frame_literal(MINIMAL_INTENT_BYTES, ids, stamp)?;
            let result = verify_complete_policy_replace_frame(
                &expected,
                limits,
                AdmissionLimits::default(),
                charge,
            );
            if length == 257 {
                assert_eq!(
                    result.err().ok_or("ID RAW +1")?,
                    CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
                );
            } else {
                assert_replace_frame_verified(
                    &result?,
                    &expected,
                    LogIdentity::new(1, 2, AppliedPosition::new(3)),
                );
            }
        }
    }
    Ok(())
}
