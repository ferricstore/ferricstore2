//! Fixed whole frames, independent recipes and held mathematical owners.

use super::super::{
    MINIMAL_INTENT_BYTES, PRESENT_INTENT_BYTES, TestResult, minimal_data, present_data, profile,
};
use sha2::{Digest, Sha256};
use wf_domain::policy::BackoffKind;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyRequestError, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits,
};
use wf_domain::policy::complete::{
    DenialAuditMode, GovernanceData, GovernanceMode, restore_complete_policy,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::EntryIdentity;
use wf_domain::workflow::control::PreparedDigest;
use wf_ports::log::VerifiedCompletePolicyReplaceFrame;
use wf_ports::log::{prepare_complete_policy_replace_frame, verify_complete_policy_replace_frame};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::boundaries::replace_frame_count_field;
use super::cases::replace_frame_governance_list;
use std::collections::BTreeMap;

pub(super) fn replace_frame_hex(hex: &str) -> TestResult<Vec<u8>> {
    let mut bytes = Vec::new();
    for pair in hex.as_bytes().as_chunks::<2>().0 {
        bytes.push(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?);
    }
    assert_eq!(bytes.len() * 2, hex.len());
    Ok(bytes)
}

pub(super) fn replace_frame_literal(
    intent: &[u8],
    ids: [&[u8]; 4],
    stamp: CommittedTime,
) -> TestResult<Vec<u8>> {
    let mut expected = b"FWCP\0\x01\x06".to_vec();
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

pub(super) fn assert_replace_frame_verified(
    verified: &VerifiedCompletePolicyReplaceFrame<'_>,
    expected: &[u8],
    log: LogIdentity,
) {
    let mut cursor = 7;
    let mut ids = Vec::new();
    let mut scope = b"FWSC\0\x01".to_vec();
    for index in 0..4 {
        assert_eq!(&expected[cursor..cursor + 2], &[0, 0]);
        let length = usize::from(u16::from_be_bytes(
            expected[cursor + 2..cursor + 4].as_chunks::<2>().0[0],
        ));
        if index < 3 {
            scope.extend_from_slice(&expected[cursor..cursor + 4]);
        }
        cursor += 4;
        ids.push(&expected[cursor..cursor + length]);
        if index < 3 {
            scope.extend_from_slice(ids[index]);
        }
        cursor += length;
    }
    let original = &expected[cursor + 56..];
    let digest = PreparedDigest::new(Sha256::digest(expected).into());
    let intent: [u8; 32] = Sha256::digest(original).into();
    let physical = u64::from_be_bytes(expected[cursor + 34..cursor + 42].as_chunks::<8>().0[0]);
    let logical = u16::from_be_bytes(expected[cursor + 42..cursor + 44].as_chunks::<2>().0[0]);
    let wall = u64::from_be_bytes(expected[cursor + 44..cursor + 52].as_chunks::<8>().0[0]);
    let header = verified.header(log);
    assert_eq!(verified.encoded(), expected);
    assert_eq!(verified.original_intent(), original);
    assert_eq!(verified.digest(), digest);
    assert_eq!(verified.isolation_scope(), scope);
    assert_eq!(header.format_version, 1);
    assert_eq!(header.scope.tenant().as_bytes(), ids[0]);
    assert_eq!(header.scope.group().as_bytes(), ids[1]);
    assert_eq!(header.scope.recovery().as_bytes(), ids[2]);
    assert_eq!(header.request.as_bytes(), ids[3]);
    assert_eq!(header.intent.version(), 2);
    assert_eq!(*header.intent.digest(), intent);
    assert_eq!(header.stamp.hlc().physical().get(), physical);
    assert_eq!(header.stamp.hlc().logical(), logical);
    assert_eq!(header.stamp.wall().get(), wall);
    assert_eq!(header.identity, EntryIdentity::new(log, digest));
    assert_eq!(header.encoded_entry, expected);
    assert_eq!(header.encoded_isolation_scope, scope);
}

pub(super) fn assert_replace_frame_account(
    maximum: u64,
    preparing: bool,
    repeated: bool,
    held_peak: bool,
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
    let expected = replace_frame_hex(FROZEN_COMPLETE_REPLACE_FRAMES[0].0)?;
    let mut produced = Vec::new();
    let mut verified = Vec::new();
    let fee = if preparing { 4_612 } else { 4_423 };
    let attempts = if repeated { 2 } else { 1 };
    let initial = if held_peak { 1_928 } else { 0 };
    let peak = if held_peak { 64 } else { 0 };
    assert_eq!((charge.g(), charge.p()), (initial, peak));
    for attempt in 1..=attempts {
        let target = initial + fee * attempt + peak;
        let result = if preparing {
            prepare_complete_policy_replace_frame(
                super::super::input(minimal_data(), None)?,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )
            .map(|value| produced.push(value))
        } else {
            verify_complete_policy_replace_frame(
                &expected,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )
            .map(|value| verified.push(value))
        };
        if maximum < target {
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
                (target - peak - 263, peak, target - 263)
            );
        } else {
            result?;
            assert_eq!(
                (charge.g(), charge.p(), charge.total()),
                (target - peak, peak, target)
            );
        }
    }
    for value in &produced {
        assert_eq!(value.as_bytes(), expected);
        assert_eq!(
            value.digest(),
            PreparedDigest::new(Sha256::digest(&expected).into())
        );
    }
    for value in &verified {
        assert_replace_frame_verified(
            value,
            &expected,
            LogIdentity::new(1, 2, AppliedPosition::new(3)),
        );
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

pub(super) fn assert_replace_frame_enum_tags(
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
        let mut data = present_data();
        data.retry
            .as_mut()
            .and_then(|retry| retry.backoff.as_mut())
            .ok_or("backoff fixture")?
            .kind = kind;
        let mut intent = PRESENT_INTENT_BYTES.to_vec();
        intent[65] = tag;
        let expected = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let prepared = prepare_complete_policy_replace_frame(
            super::super::input(data, None)?,
            limits,
            AdmissionLimits::default(),
            charge,
        )?;
        assert_eq!(prepared.as_bytes(), expected);
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
    for tag in 0..3 {
        for field in [0, 1] {
            let mut data = present_data();
            let governance = data.governance.as_mut().ok_or("governance fixture")?;
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
            let expected = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let prepared = prepare_complete_policy_replace_frame(
                super::super::input(data, None)?,
                limits,
                AdmissionLimits::default(),
                charge,
            )?;
            assert_eq!(prepared.as_bytes(), expected);
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

pub(super) fn assert_replace_frame_list_order(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for state in [false, true] {
        let mut data = present_data();
        replace_frame_governance_list(&mut data, 6, state)?;
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
        let expected = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let prepared = prepare_complete_policy_replace_frame(
            super::super::input(data, None)?,
            limits,
            AdmissionLimits::default(),
            charge,
        )?;
        assert_eq!(prepared.as_bytes(), expected);
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
    Ok(())
}

pub(super) fn replace_frame_rule_count(
    governance: &mut GovernanceData,
    case: usize,
    names: Vec<Vec<u8>>,
) -> TestResult {
    match case {
        8 => governance.limits = Some(BTreeMap::new()),
        9 => governance.budgets = Some(BTreeMap::new()),
        10 => governance.circuits = Some(BTreeMap::new()),
        _ => return Err("unknown rule count fixture".into()),
    }
    for name in names {
        let mut fixture = present_data().governance.ok_or("original rule fixture")?;
        match case {
            8 => {
                let rule = fixture
                    .limits
                    .as_mut()
                    .and_then(BTreeMap::pop_first)
                    .ok_or("limit rule fixture")?
                    .1;
                governance
                    .limits
                    .as_mut()
                    .ok_or("limit map fixture")?
                    .insert(name, rule);
            }
            9 => {
                let rule = fixture
                    .budgets
                    .as_mut()
                    .and_then(BTreeMap::pop_first)
                    .ok_or("budget rule fixture")?
                    .1;
                governance
                    .budgets
                    .as_mut()
                    .ok_or("budget map fixture")?
                    .insert(name, rule);
            }
            10 => {
                let rule = fixture
                    .circuits
                    .as_mut()
                    .and_then(BTreeMap::pop_first)
                    .ok_or("circuit rule fixture")?
                    .1;
                governance
                    .circuits
                    .as_mut()
                    .ok_or("circuit map fixture")?
                    .insert(name, rule);
            }
            _ => return Err("unknown rule count fixture".into()),
        }
    }
    Ok(())
}

pub(super) fn assert_replace_frame_map_order(
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for case in [0, 8, 9, 10] {
        let mut data = if case == 0 {
            minimal_data()
        } else {
            present_data()
        };
        replace_frame_count_field(&mut data, case, 2)?;
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
        let expected = replace_frame_literal(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
        let prepared = prepare_complete_policy_replace_frame(
            super::super::input(data, None)?,
            limits,
            AdmissionLimits::default(),
            charge,
        )?;
        assert_eq!(prepared.as_bytes(), expected);
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
        for repeated in [false, true] {
            let mut reversed = source.to_vec();
            let stride = 6 + suffix.len();
            let mut wire = if case == 0 { Vec::new() } else { vec![1] };
            wire.extend_from_slice(&2_u32.to_be_bytes());
            wire.extend_from_slice(&entries[stride..]);
            wire.extend_from_slice(if repeated {
                &entries[stride..]
            } else {
                &entries[..stride]
            });
            drop(reversed.splice(start..end, wire));
            let encoded = replace_frame_literal(&reversed, [b"t", b"g", b"r", b"q"], stamp)?;
            let result = verify_complete_policy_replace_frame(
                &encoded,
                limits,
                AdmissionLimits::default(),
                charge,
            );
            assert_eq!(
                result.err().ok_or("ordered map refusal")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed)
            );
        }
    }
    Ok(())
}

pub(super) const FROZEN_COMPLETE_REPLACE_FRAMES: &[(&str, &str)] = &[
    (
        "46574350000106000000017400000001670000000172000000017100026d4b9979337b028b79a46163f49c9266f687be9a65f1ec65245451ff5c90b12f0000000000000000000000000000000000000000003457462d494e54454e5400434f4d504c4554452d5245504c4143450056320000000001740000000001720000000000000000000000",
        "d7a89a8bf334bd47813544d2b78b0425334e41b38ade08b986ce2921576ead2b",
    ),
    (
        "465743500001060000000174000000016700000001720000000171000297e0027df478ee12f63ad52c8a7808157cc7b2e64a209e5de1b543aac5e661010000000000000000000000000000000000000000018a57462d494e54454e5400434f4d504c4554452d5245504c414345005632000000000174000000000172010100000000000000000100000000000000010101000001030000000000000001000000000000000200010000000178010100000000000000010100000001000000010000000173010101000000010100000000000000010100000001010000000000000001000000020000000161000000017a01000000016d0101020101000000020000000161000000017a0100000000010001010001020100000001000000016c00000000000000000100010000000000000001010000000100000001620000000000000001000000000000000101010100000000000000010100000001750100000001000000016300000000000000010000000000000001010000000000000001010000000000000001013201000000000000000101000000000000000101000000000000000101000000020000000161000000017a01010000000100000001610100000001000000017301000000010000000175010000000000000001",
        "afaec346ae7bd946f7a3dd88e729b646106c02fd93cf5c736841508778d88bc5",
    ),
];
