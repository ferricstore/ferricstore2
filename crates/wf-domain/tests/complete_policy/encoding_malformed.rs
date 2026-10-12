//! Wire refusals fixed before any semantic owner is materialized.

use wf_domain::policy::complete::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError,
    decode_complete_policy_value_v2,
};
use wf_domain::policy::installation::PolicyError;
use wf_domain::value::{DomainError, InputField};

use super::{TestResult, limits};

#[test]
fn v2_rejects_every_truncation_and_trailing_byte() -> TestResult {
    let minimal =
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0";
    let profile = limits(1_000_000)?;
    for end in 0..minimal.len() {
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(
            matches!(
                decode_complete_policy_value_v2(&minimal[..end], profile, &mut charge),
                Err(CompletePolicyEncodingError::Malformed)
            ),
            "truncation at {end}"
        );
    }
    for snapshot_length in [22_u32, 23] {
        let mut trailing = minimal.to_vec();
        trailing.push(0);
        trailing[14..18].copy_from_slice(&snapshot_length.to_be_bytes());
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(matches!(
            decode_complete_policy_value_v2(&trailing, profile, &mut charge),
            Err(CompletePolicyEncodingError::TrailingBytes)
        ));
    }
    // Outer trailing bytes precede an otherwise unknown snapshot version.
    let mut both = minimal.to_vec();
    both[23] = 9;
    both.push(0);
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        decode_complete_policy_value_v2(&both, profile, &mut charge),
        Err(CompletePolicyEncodingError::TrailingBytes)
    ));
    Ok(())
}

#[test]
fn v2_rejects_unknown_versions_tags_and_option_bytes() -> TestResult {
    let minimal =
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0";
    let profile = limits(1_000_000)?;
    for offset in [5, 23] {
        let mut encoded = minimal.to_vec();
        encoded[offset] = 1;
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(matches!(
            decode_complete_policy_value_v2(&encoded, profile, &mut charge),
            Err(CompletePolicyEncodingError::UnsupportedVersion)
        ));
    }
    for offset in [0, 18] {
        let mut encoded = minimal.to_vec();
        encoded[offset] = b'?';
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(matches!(
            decode_complete_policy_value_v2(&encoded, profile, &mut charge),
            Err(CompletePolicyEncodingError::Malformed)
        ));
    }
    for offset in [29, 30, 31, 32, 37, 38, 39] {
        let mut encoded = minimal.to_vec();
        encoded[offset] = 2;
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(matches!(
            decode_complete_policy_value_v2(&encoded, profile, &mut charge),
            Err(CompletePolicyEncodingError::InvalidOptionality)
        ));
    }
    // Each replacement is an independent field grammar, with only its enum/bool tag invalid.
    for (start, end, replacement) in [
        (29, 30, b"\x01\x02".as_slice()),
        (
            31,
            32,
            b"\x01\0\x01\x04\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0",
        ),
        (33, 37, b"\0\0\0\x01\0\0\0\x01k\x01\x02\0\0\0"),
        (39, 40, b"\x01\x01\x03\0\0\0\0\0\0"),
        (39, 40, b"\x01\0\x01\0\0\x01\x02\0\0\0\0\0"),
        (39, 40, b"\x01\0\0\x01\0\x01\x03\0\0\0\0"),
        (
            39,
            40,
            b"\x01\0\0\0\x01\0\0\0\x01\0\0\0\x01k\0\0\0\0\0\0\0\0\x01\x02\0\0\0\0",
        ),
    ] {
        let mut encoded = minimal.to_vec();
        encoded.splice(start..end, replacement.iter().copied());
        let length = u32::try_from(encoded.len() - 18)?.to_be_bytes();
        encoded[14..18].copy_from_slice(&length);
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(
            matches!(
                decode_complete_policy_value_v2(&encoded, profile, &mut charge),
                Err(CompletePolicyEncodingError::UnsupportedTag)
            ),
            "tag replacement at {start}"
        );
    }
    Ok(())
}

#[test]
fn v2_rejects_overlong_counts_lengths_and_disordered_keys() -> TestResult {
    let minimal =
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0";
    let profile = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        decode_complete_policy_value_v2(&vec![0; 263_169], profile, &mut charge),
        Err(CompletePolicyEncodingError::ByteLimit)
    ));
    for offset in [14, 24] {
        let mut encoded = minimal.to_vec();
        encoded[offset..offset + 4].copy_from_slice(&262_145_u32.to_be_bytes());
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(matches!(
            decode_complete_policy_value_v2(&encoded, profile, &mut charge),
            Err(CompletePolicyEncodingError::ByteLimit)
        ));
    }
    let mut too_many_states = minimal.to_vec();
    too_many_states[33..37].copy_from_slice(&16_385_u32.to_be_bytes());
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        decode_complete_policy_value_v2(&too_many_states, profile, &mut charge),
        Err(CompletePolicyEncodingError::ItemLimit)
    ));
    for (start, end, replacement) in [
        (37, 38, b"\x01\0\0\0\x04".as_slice()),
        (39, 40, b"\x01\0\x01\x01\0\0\x03\xe9\0\0\0\0\0\0\0\0"),
        (39, 40, b"\x01\0\0\0\x01\0\0\x03\xe9\0\0\0"),
    ] {
        let mut encoded = minimal.to_vec();
        encoded.splice(start..end, replacement.iter().copied());
        let length = u32::try_from(encoded.len() - 18)?.to_be_bytes();
        encoded[14..18].copy_from_slice(&length);
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(matches!(
            decode_complete_policy_value_v2(&encoded, profile, &mut charge),
            Err(CompletePolicyEncodingError::ItemLimit)
        ));
    }
    for second_key in *b"az" {
        let mut encoded = minimal.to_vec();
        let rows = b"\0\0\0\x02\0\0\0\x01z\0\0\0\0\0\0\0\x01a\0\0\0\0";
        encoded.splice(33..37, rows.iter().copied());
        encoded[50] = second_key;
        encoded[14..18].copy_from_slice(&40_u32.to_be_bytes());
        let mut charge = PolicySchemaCharge::new(profile);
        assert!(matches!(
            decode_complete_policy_value_v2(&encoded, profile, &mut charge),
            Err(CompletePolicyEncodingError::Malformed)
        ));
    }
    let mut duplicate_list = minimal.to_vec();
    duplicate_list.splice(
        37..38,
        b"\x01\0\0\0\x02\0\0\0\x01z\0\0\0\x01z".iter().copied(),
    );
    duplicate_list[14..18].copy_from_slice(&36_u32.to_be_bytes());
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        decode_complete_policy_value_v2(&duplicate_list, profile, &mut charge),
        Err(CompletePolicyEncodingError::Malformed)
    ));
    assert_eq!(charge.p(), 128);
    // State keys use the core 262144-byte cap, not governance's 65535-byte cap.
    let mut large_state = minimal[..33].to_vec();
    large_state.extend_from_slice(b"\0\0\0\x01\0\x01\0\0");
    large_state.extend_from_slice(&vec![b'a'; 65_536]);
    large_state.extend_from_slice(b"\0\0\0\0\0\0\0");
    large_state[14..18].copy_from_slice(&65_566_u32.to_be_bytes());
    let mut charge = PolicySchemaCharge::new(profile);
    let value = decode_complete_policy_value_v2(&large_state, profile, &mut charge)?;
    assert_eq!(value.data().states.len(), 1);
    let (key, state) = value
        .data()
        .states
        .first_key_value()
        .ok_or("missing large state")?;
    assert_eq!(key.as_slice(), vec![b'a'; 65_536]);
    assert!(
        state.mode.is_none()
            && state.retry.is_none()
            && state.retention.is_none()
            && state.governance.is_none()
    );
    Ok(())
}

#[test]
fn v2_late_structural_refusal_precedes_materialization() -> TestResult {
    // Empty type is a later typed semantic error. The final bad option wins first.
    let encoded =
        b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x15FWPS\0\x02\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x02";
    assert_eq!(encoded.len(), 39);
    let profile = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        decode_complete_policy_value_v2(encoded, profile, &mut charge),
        Err(CompletePolicyEncodingError::InvalidOptionality)
    ));
    let admitted_scan = (charge.g(), charge.p());
    assert!(admitted_scan.0 > 0);
    let mut semantic = encoded.to_vec();
    semantic[38] = 0;
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        decode_complete_policy_value_v2(&semantic, profile, &mut charge),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::Policy(PolicyError::Domain(DomainError::EmptyValue(
                InputField::Type
            )))
        ))
    ));
    assert!(charge.g() > admitted_scan.0);
    assert_eq!(charge.p(), 0);
    // These are logical phase controls, not a native allocation or destructor probe.
    // Maximal snapshot type: the final bad governance option still precedes owners.
    let mut encoded = b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\x04\0\0FWPS\0\x02\0\x03\xff\xeb".to_vec();
    encoded.resize(encoded.len() + 262_123, b'r');
    encoded.extend_from_slice(b"\0\0\0\0\0\0\0\0\0\0\x02");
    assert_eq!(encoded.len(), 262_162);
    let mut charge = PolicySchemaCharge::new(profile);
    assert!(matches!(
        decode_complete_policy_value_v2(&encoded, profile, &mut charge),
        Err(CompletePolicyEncodingError::InvalidOptionality)
    ));
    assert_eq!((charge.g(), charge.p()), (262_674, 0));
    Ok(())
}
