//! Independent logical charge, profile, stream and moved-owner controls.

use std::collections::BTreeMap;

use wf_domain::policy::RetryOverride;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompleteStatePolicyData, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, decode_complete_policy_value_v2, prepare_complete_policy_value_v2,
    verify_complete_policy_value_v2,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::policy::installation::PolicyLimits;

use super::{TestResult, candidate, data, limits};

#[test]
fn v2_prepare_verify_decode_share_one_nonrollback_charge() -> TestResult {
    let expected =
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0";
    // Frozen fees: restore642; prepare642+552+168+552; verify642+552+592;
    // decode552+(406+256+129)+642+168. P is canonical scratch only.
    let profile = limits(6_495)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let input = candidate(
        1,
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    assert_eq!((charge.g(), charge.p(), charge.total()), (642, 0, 642));
    let value = prepare_complete_policy_value_v2(input, profile, &mut charge)?;
    assert_eq!(value.encoded(), expected);
    assert_eq!((charge.g(), charge.p(), charge.total()), (2_556, 0, 2_556));
    verify_complete_policy_value_v2(&value, profile, &mut charge)?;
    assert_eq!((charge.g(), charge.p(), charge.total()), (4_342, 0, 4_342));
    let decoded = decode_complete_policy_value_v2(expected, profile, &mut charge)?;
    assert!(decoded == value);
    assert_eq!((charge.g(), charge.p(), charge.total()), (6_495, 0, 6_495));
    assert!(matches!(
        decode_complete_policy_value_v2(expected, profile, &mut charge),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes {
                bytes: 6_623,
                maximum: 6_495,
            }
        ))
    ));
    assert_eq!((charge.g(), charge.p(), charge.total()), (6_495, 0, 6_495));
    let short = limits(2_555)?;
    let mut charge = PolicySchemaCharge::new(short);
    let input = candidate(
        1,
        data(b"r".to_vec(), None, BTreeMap::new()),
        short,
        &mut charge,
    )?;
    assert!(matches!(
        prepare_complete_policy_value_v2(input, short, &mut charge),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes {
                bytes: 2_556,
                maximum: 2_555,
            }
        ))
    ));
    assert_eq!((charge.g(), charge.p(), charge.total()), (2_555, 0, 2_555));
    // One canonical attribute: scan1595 + material1697 + restore1928 + copy177.
    let attribute = b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x1fFWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\x01\0\0\0\x01\0\0\0\x01a\0\0";
    assert_eq!(attribute.len(), 49);
    for maximum in [5_461, 5_460] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let result = decode_complete_policy_value_v2(attribute, profile, &mut charge);
        if maximum == 5_461 {
            let value = result?;
            let mut expected_data = data(b"r".to_vec(), None, BTreeMap::new());
            expected_data.indexed_attributes = Some(vec![b"a".to_vec()]);
            assert!(value.data() == &expected_data);
            assert_eq!(value.encoded(), attribute);
            assert_eq!((charge.g(), charge.p(), charge.total()), (5_397, 64, 5_461));
        } else {
            assert!(matches!(
                result,
                Err(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::WorkBytes {
                        bytes: 5_461,
                        maximum: 5_460,
                    }
                ))
            ));
            assert_eq!((charge.g(), charge.p(), charge.total()), (5_220, 64, 5_284));
        }
    }
    Ok(())
}

#[test]
fn v2_current_profiles_and_resource_errors_remain_distinct() -> TestResult {
    let minimal =
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0";
    let profile = limits(10_000_000)?;
    let other = limits(9_999_999)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        decode_complete_policy_value_v2(&vec![0; 263_169], other, &mut charge),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    ));
    assert_eq!((charge.g(), charge.p()), (0, 0));
    let input = candidate(
        1,
        data(b"r".to_vec(), None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    assert!(matches!(
        prepare_complete_policy_value_v2(input, other, &mut charge),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    ));
    assert_eq!((charge.g(), charge.p()), (642, 0));
    let value = decode_complete_policy_value_v2(minimal, profile, &mut charge)?;
    let before = (charge.g(), charge.p());
    assert_eq!(
        verify_complete_policy_value_v2(&value, other, &mut charge),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    );
    assert_eq!((charge.g(), charge.p()), before);
    let two_states = b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x28FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\x02\0\0\0\x01a\0\0\0\0\0\0\0\x01z\0\0\0\0\0\0\0";
    let value = decode_complete_policy_value_v2(two_states, profile, &mut charge)?;
    let narrow = PolicySchemaLimits::new(PolicyLimits::new(1)?, 10_000_000)?;
    let mut current = PolicySchemaCharge::new(narrow);
    assert_eq!(
        verify_complete_policy_value_v2(&value, narrow, &mut current),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::InvalidAuthority
        ))
    );
    assert_eq!((current.g(), current.p()), (128, 0));
    // Typed leaf admission succeeds; wire prefixes make this snapshot too large.
    let mut charge = PolicySchemaCharge::new(profile);
    let input = candidate(
        1,
        data(vec![b'r'; 262_144], None, BTreeMap::new()),
        profile,
        &mut charge,
    )?;
    assert!(matches!(
        prepare_complete_policy_value_v2(input, profile, &mut charge),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::SnapshotBytes
        ))
    ));
    // A length-only existing binding is not a correspondence proof.
    for (bytes, delta) in [(vec![0; 40], 1_330), (vec![0; 39], 1_194)] {
        let mut charge = PolicySchemaCharge::new(profile);
        let input = candidate(
            1,
            data(b"r".to_vec(), None, BTreeMap::new()),
            profile,
            &mut charge,
        )?;
        let value = input.bind_encoded_snapshot(0..bytes.len(), bytes, profile, &mut charge)?;
        let before = charge.g();
        assert_eq!(
            verify_complete_policy_value_v2(&value, profile, &mut charge),
            Err(CompletePolicyEncodingError::EncodingMismatch)
        );
        assert_eq!(charge.g(), before + delta);
    }
    assert_eq!(
        CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow).to_string(),
        "complete policy byte length overflow"
    );
    // Public bounded owners cannot execute u64 overflow or an oversized safe Value.
    Ok(())
}

#[test]
fn v2_admits_each_copy_and_streams_verify_without_second_output() -> TestResult {
    let expected =
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0";
    // The final envelope copy is one 128+40 debit after 552+791+642 work.
    let short = limits(2_152)?;
    let mut charge = PolicySchemaCharge::new(short);
    assert!(matches!(
        decode_complete_policy_value_v2(expected, short, &mut charge),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes {
                bytes: 2_153,
                maximum: 2_152,
            }
        ))
    ));
    assert_eq!((charge.g(), charge.p()), (1_985, 0));
    // Reserve precedes emission: 642+642+552=1836, then 128+40=168.
    let short = limits(2_003)?;
    let mut charge = PolicySchemaCharge::new(short);
    let input = candidate(
        1,
        data(b"r".to_vec(), None, BTreeMap::new()),
        short,
        &mut charge,
    )?;
    assert!(matches!(
        prepare_complete_policy_value_v2(input, short, &mut charge),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes {
                bytes: 2_004,
                maximum: 2_003,
            }
        ))
    ));
    assert_eq!((charge.g(), charge.p()), (1_836, 0));
    let profile = limits(3_939)?; // Decode2153 plus streaming verification1786.
    let mut charge = PolicySchemaCharge::new(profile);
    let value = decode_complete_policy_value_v2(expected, profile, &mut charge)?;
    let expected_data = data(b"r".to_vec(), None, BTreeMap::new());
    assert!(value.data() == &expected_data);
    assert_eq!(value.generation(), 1);
    assert_eq!(value.encoded(), expected);
    assert_eq!((charge.g(), charge.p()), (2_153, 0));
    let owners = (
        value.data().workflow_type.as_ptr(),
        value.encoded().as_ptr(),
    );
    assert_ne!(value.encoded().as_ptr(), expected.as_ptr());
    assert_ne!(
        value.data().workflow_type.as_ptr(),
        expected[28..29].as_ptr()
    );
    verify_complete_policy_value_v2(&value, profile, &mut charge)?;
    assert!(value.data() == &expected_data);
    assert_eq!(value.encoded(), expected);
    assert_eq!((charge.g(), charge.p(), charge.total()), (3_939, 0, 3_939));
    assert_eq!(
        (
            value.data().workflow_type.as_ptr(),
            value.encoded().as_ptr()
        ),
        owners
    );
    // Owner addresses and logical fees do not instrument native allocations/RSS.
    // The ordered a/z key pair pays 128 plus both actual one-byte key lengths.
    let expected = b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x28FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\x02\0\0\0\x01a\0\0\0\0\0\0\0\x01z\0\0\0\0\0\0\0";
    assert_eq!(expected.len(), 58);
    let profile = limits(1_204)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        decode_complete_policy_value_v2(expected, profile, &mut charge),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::WorkBytes {
                bytes: 1_205,
                maximum: 1_204,
            }
        ))
    ));
    assert_eq!((charge.g(), charge.p()), (1_075, 0));
    Ok(())
}

#[test]
fn v2_round_trip_compares_values_before_original_owner_descriptors() -> TestResult {
    let expected = [
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x3bFWPS\0\x02\0\0\0\x01r".as_slice(),
        b"\x01\0\0\0\0\x01v\0\0\0\0\0\0\x01\0\0\0\x01s\0\x01\0\0\x01\0\0\0\x01q\0\0",
        b"\x01\0\0\0\x01\0\0\0\x01a\x01\0\0\0\x01m\0",
    ]
    .concat();
    assert_eq!(expected.len(), 77);
    let mut input = data(
        b"r".to_vec(),
        Some(PolicyVersion::Bytes(b"v".to_vec())),
        BTreeMap::new(),
    );
    input.states.insert(
        b"s".to_vec(),
        CompleteStatePolicyData {
            mode: None,
            retry: Some(RetryOverride {
                max_retries: None,
                backoff: None,
                exhausted_to: Some(b"q".to_vec()),
            }),
            retention: None,
            governance: None,
        },
    );
    input.indexed_attributes = Some(vec![b"a".to_vec()]);
    input.indexed_state_meta = Some(b"m".to_vec());
    let profile = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let decoded = decode_complete_policy_value_v2(&expected, profile, &mut charge)?;
    assert!(decoded.data() == &input && decoded.generation() == 1);
    assert_eq!(decoded.encoded(), expected);
    let Some(PolicyVersion::Bytes(version)) = &input.version else {
        return Err("missing version".into());
    };
    let (key, state) = input.states.first_key_value().ok_or("missing state")?;
    let exhausted = state
        .retry
        .as_ref()
        .and_then(|r| r.exhausted_to.as_ref())
        .ok_or("missing target")?;
    let attributes = input
        .indexed_attributes
        .as_ref()
        .ok_or("missing attributes")?;
    let metadata = input
        .indexed_state_meta
        .as_ref()
        .ok_or("missing metadata")?;
    let owners = (
        input.workflow_type.as_ptr(),
        version.as_ptr(),
        key.as_ptr(),
        exhausted.as_ptr(),
        attributes.as_ptr(),
        attributes[0].as_ptr(),
        metadata.as_ptr(),
    );
    let input = candidate(1, input, profile, &mut charge)?;
    let value = prepare_complete_policy_value_v2(input, profile, &mut charge)?;
    verify_complete_policy_value_v2(&value, profile, &mut charge)?;
    assert!(value == decoded);
    assert_eq!(value.encoded(), expected);
    let moved = value.data();
    let Some(PolicyVersion::Bytes(version)) = &moved.version else {
        return Err("missing moved version".into());
    };
    let (key, state) = moved
        .states
        .first_key_value()
        .ok_or("missing moved state")?;
    let exhausted = state
        .retry
        .as_ref()
        .and_then(|r| r.exhausted_to.as_ref())
        .ok_or("missing moved target")?;
    let attributes = moved
        .indexed_attributes
        .as_ref()
        .ok_or("missing moved attributes")?;
    let metadata = moved
        .indexed_state_meta
        .as_ref()
        .ok_or("missing moved metadata")?;
    assert_eq!(
        (
            moved.workflow_type.as_ptr(),
            version.as_ptr(),
            key.as_ptr(),
            exhausted.as_ptr(),
            attributes.as_ptr(),
            attributes[0].as_ptr(),
            metadata.as_ptr()
        ),
        owners
    );
    assert_eq!(charge.p(), 64);
    Ok(())
}
