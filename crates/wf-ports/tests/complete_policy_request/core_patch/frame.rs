//! Borrowed tag-five original core Patch frame verification controls.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyRequestError, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, restore_complete_core_policy_patch_frame_header,
    scan_complete_core_policy_patch_frame,
};
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, PreparedDigest};
use wf_ports::log::verify_complete_core_policy_patch_frame;
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::super::{TestResult, profile};

mod cases;

use cases::{
    assert_core_patch_default_frame, assert_core_patch_frame_account_case,
    assert_core_patch_frame_prefix_refusals, assert_core_patch_frame_refusal,
    assert_core_patch_frame_scope_limits, core_patch_frame_bytes,
};

#[test]
fn core_patch_frame_verifies_independent_tag_five_header_and_original_intent() -> TestResult {
    let source8 = [
        b"\0\0\0\0\0\0\0\x02".as_slice(),
        b"\0\0\0\x06queued",
        b"\x01\0\x01\0\0\x01",
        b"\0\0\0\x04dead\0",
        b"\0\0\0\x06review\x01\x01\x01\0\0",
    ]
    .concat();
    let maximum = [
        b"\x01\x01".as_slice(),
        &[255; 8],
        b"\x01",
        &[255; 8],
        b"\x01\x01\xff\xff\x01\x01\x03\x01",
        &[255; 8],
        b"\x01",
        &[255; 8],
        b"\x01\xff\x01\0\0\0\x02\0\xff\x01\x01",
        &[255; 8],
        b"\x01",
        &[255; 4],
        b"\0\0\0\0",
    ]
    .concat();
    let mut zero = maximum.clone();
    for byte in &mut zero {
        if *byte == 255 {
            *byte = 0;
        }
    }
    zero[25] = 0;
    zero[47..51].copy_from_slice(&[0; 4]);
    drop(zero.drain(51..53));
    let mut binary = vec![1, 0, 0, 0, 0, 2, 0, 255];
    binary.extend_from_slice(&maximum[10..]);
    let tails: [&[u8]; 11] = [
        b"\0\0\0\0\0\0\0\0",
        b"\0\0\0\0\0\0\0\0",
        b"\0\0\0\0\0\0\0\0",
        b"\0\0\x01\0\0\0\0\0\0\0\0",
        b"\0\0\x01\0\x01\0\0\0\0\0\0\0\0\0\0",
        b"\0\0\0\0\0\0\0\x01\0\0\0\x01a\0",
        b"\0\0\0\0\0\0\0\x01\0\0\0\x01a\x01\0\0\0",
        &source8,
        &maximum,
        &zero,
        &binary,
    ];
    let hashes = [
        b"\x67\x6f\x37\x56\x04\x99\xd1\xe1\x8b\x48\x56\x02\x7d\x6f\x7f\x79\x46\x38\xd5\x67\xe7\x50\x5f\x8b\x86\x62\xe0\x73\xf3\xd1\x6d\x8d",
        b"\xf0\x35\xc2\xa1\x6b\x84\x25\xd8\x44\xe8\x88\xdc\x0d\x3f\xf1\x4d\xe0\x78\xce\xe2\xaa\x3d\x9a\x59\xf9\xaf\xc7\x2d\x26\x5b\x98\x35",
        b"\x0c\x9d\x65\x85\xae\xfe\x39\x89\xe5\x76\x3c\x10\x8d\x85\xf3\x73\x6f\xa3\xc0\xdf\xcd\xa0\x53\x90\x89\x69\x07\x93\xa4\xfe\xb6\x32",
        b"\x31\x21\xf2\x2a\xd4\x2c\x0f\x6c\x02\x4d\x83\x95\x43\x07\x1e\xff\x84\xa2\x79\xb5\x67\x91\x9b\xfb\x3c\x5c\xcd\xf6\x4e\xe5\x75\x95",
        b"\x43\x92\x26\xc0\x2d\xbd\x90\x53\x27\xd2\xa7\x1c\x0f\x1c\x50\x88\x39\xfd\x10\xf8\x25\x01\x01\xae\xb1\x8a\x62\xb9\xf5\x35\x49\xc6",
        b"\x13\x61\x84\x09\xa1\x9d\x6b\xf9\x30\x13\x98\x18\x19\xde\x0c\xbe\x98\x53\xce\x37\xbb\xb6\xdd\x7a\xfa\x8a\xd3\xa0\x65\x21\xfa\xea",
        b"\x96\x63\x9d\x7c\x7b\x19\x26\xa6\x9e\x91\xbc\x07\x4a\xfb\x76\x24\xb5\xd7\xf0\xb3\x17\x3e\xc7\x1e\x0a\x80\xb6\xa7\x55\x3c\x23\xf3",
        b"\x9a\xcd\x29\xda\x15\xea\xbb\xba\xcd\xd7\xac\x2c\x2e\xd9\xd5\x06\x79\x66\x54\x4e\x02\xf1\x21\xcb\x62\x92\x89\x93\x96\xd6\xb2\x5b",
        b"\x4f\xe5\xc2\x7a\x1c\x72\x4e\x51\xf3\xf8\x8a\x11\xc2\x97\x06\x41\xa8\xd9\x0e\xcd\x02\x46\x9c\x0e\x7b\x41\x74\x92\x07\x76\x2a\x3b",
        b"\x60\xdb\xee\xa6\x42\x25\xa8\x0a\xe2\x27\xd1\x70\x22\xe4\x66\x3c\xf5\x98\x64\xb5\x54\x66\x10\xa8\x95\xe0\x03\x41\xa9\x59\x94\xeb",
        b"\xde\x31\x42\x0a\xb6\x05\xcf\xd0\xbe\x5f\x29\xbd\xc8\xbc\x21\xba\xe6\x83\x36\xdd\x12\xe1\x34\x12\x1f\x22\xc8\x37\xd7\x60\x39\x66",
    ];
    let nodes = [5, 6, 6, 7, 9, 7, 8, 16, 22, 22, 23];
    let sizes = [52, 60, 60, 55, 59, 58, 61, 92, 116, 114, 114];
    let fees = [
        4_423, 4_575, 4_575, 4_688, 4_956, 4_697, 4_834, 6_091, 6_791, 6_785, 6_913,
    ];
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    for (case, tail) in tails.into_iter().enumerate() {
        let mut intent = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0\0\0\0\x01t".to_vec();
        intent.push(u8::from(case == 1 || case == 2));
        if case == 1 || case == 2 {
            intent.extend_from_slice(&if case == 1 { 0_u64 } else { u64::MAX }.to_be_bytes());
        }
        intent.extend_from_slice(b"\0\0\0\x01r");
        intent.extend_from_slice(tail);
        assert_eq!(intent.len(), sizes[case]);
        let bytes = u64::try_from(sizes[case])?;
        let comparison = if case == 7 { 140 } else { 0 };
        assert_eq!(
            fees[case],
            3_627 + 128 * nodes[case] + 3 * bytes + comparison
        );
        let encoded = core_patch_frame_bytes(&intent)?;
        assert_eq!(&<[u8; 32]>::from(Sha256::digest(&encoded)), hashes[case]);
        let before = charge.g();
        let value =
            verify_complete_core_policy_patch_frame(&encoded, limits, admission, &mut charge)?;
        assert_core_patch_default_frame(&value, &encoded, &intent);
        assert_eq!((charge.g(), charge.p()), (before + fees[case], 0));
    }
    Ok(())
}

#[test]
fn core_patch_frame_binds_header_scope_request_stamp_and_log_identity() -> TestResult {
    let intent = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0\0\0\0\x01t\0\0\0\0\x01r\0\0\0\0\0\0\0\0";
    let base = core_patch_frame_bytes(intent)?;
    let hashes = [
        b"\x67\x6f\x37\x56\x04\x99\xd1\xe1\x8b\x48\x56\x02\x7d\x6f\x7f\x79\x46\x38\xd5\x67\xe7\x50\x5f\x8b\x86\x62\xe0\x73\xf3\xd1\x6d\x8d",
        b"\x4b\x86\xc0\x74\xb9\x28\x4d\x4e\xe6\x50\xae\xef\x3f\x90\xcd\xdf\x02\x9e\x28\xf5\xf5\xc6\x74\x68\x39\x7b\xf5\x4e\x0e\x20\xeb\x6e",
        b"\x11\x6f\x51\xbe\xef\x8a\xe4\xc5\xfe\x3a\x8b\x10\xd9\x74\xaa\xa6\xbd\x3c\x1d\xe3\x3a\x89\x1a\x9f\xc9\xed\x82\xdd\xfe\x51\xa5\x8b",
        b"\xd0\x48\xdb\x1d\x97\x79\xad\x3e\xc5\x7a\x55\xea\x44\xb0\x13\xf4\xbe\x3c\x36\x8f\xc3\x19\x1b\xf1\xf9\x1b\x7f\x95\x42\x67\xf6\xe6",
        b"\xf0\xe0\xd4\xe5\xfd\xf1\x9b\xfa\x1d\x50\x36\x19\x33\x7a\xe3\x90\xaa\x37\xea\xd9\xc5\xb7\xfe\xcc\xc7\x5a\x1c\x9d\xd0\x47\xcf\x9c",
        b"\xfd\x47\x37\x43\xa7\xcd\x37\x4c\xd4\xdf\x60\x24\x4b\xef\x82\x95\xbf\xfb\x0c\x4e\xbf\xd7\xa0\x63\x1e\x9a\x58\xdc\x42\x6c\x8d\x4e",
    ];
    let ids = [*b"tgrq", *b"tGrq", *b"tgRq", *b"tgrQ", *b"tgrq", *b"ugrq"];
    let limits = profile(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    for (case, expected_ids) in ids.into_iter().enumerate() {
        let mut encoded = base.clone();
        let mut expected_intent = intent.to_vec();
        for (offset, byte) in [11, 16, 21, 26].into_iter().zip(expected_ids) {
            encoded[offset] = byte;
        }
        expected_intent[37] = expected_ids[0];
        encoded[120] = expected_ids[0];
        let original: [u8; 32] = Sha256::digest(&expected_intent).into();
        encoded[29..61].copy_from_slice(&original);
        let expected_original = if case == 5 {
            b"\x33\xf7\x2a\xab\x4d\x11\x4d\xa0\xd9\xf6\xff\xb1\xb7\xc3\xf4\x75\xab\x14\x9c\xef\xdf\x0c\xcd\x2e\xa1\x6a\xed\x5d\x03\xca\x92\x96"
        } else {
            b"\xbe\x44\x4a\x3a\x43\x26\xb0\xd9\x98\xb2\xe1\x30\xa0\x2b\x07\x5b\x72\x81\x6e\xd0\xc1\xc5\xa3\x01\xd4\x6b\xb8\xae\xa9\x9e\xcc\x16"
        };
        assert_eq!(
            &<[u8; 32]>::from(Sha256::digest(&expected_intent)),
            expected_original
        );
        let stamp = if case == 4 {
            encoded[61..69].copy_from_slice(&99_u64.to_be_bytes());
            encoded[69..71].copy_from_slice(&7_u16.to_be_bytes());
            encoded[71..79].copy_from_slice(&98_u64.to_be_bytes());
            CommittedTime::new(HlcTimestamp::new(99, 7)?, 98)?
        } else {
            CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?
        };
        let prepared: [u8; 32] = Sha256::digest(&encoded).into();
        assert_eq!(&<[u8; 32]>::from(Sha256::digest(&encoded)), hashes[case]);
        let mut scope = b"FWSC\0\x01\0\0\0\x01t\0\0\0\x01g\0\0\0\x01r".to_vec();
        for (offset, byte) in [10, 15, 20].into_iter().zip(expected_ids) {
            scope[offset] = byte;
        }
        let before = charge.g();
        let value = verify_complete_core_policy_patch_frame(
            &encoded,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_eq!(value.encoded(), encoded.as_slice());
        assert_eq!(value.original_intent(), expected_intent.as_slice());
        assert_eq!(value.digest(), PreparedDigest::new(prepared));
        assert_eq!(value.isolation_scope(), scope.as_slice());
        for log in [
            LogIdentity::new(1, 2, AppliedPosition::new(3)),
            LogIdentity::new(9, 8, AppliedPosition::new(7)),
        ] {
            let header = value.header(log);
            assert_eq!(header.format_version, 1);
            assert_eq!(header.scope.tenant().as_bytes(), &[expected_ids[0]]);
            assert_eq!(header.scope.group().as_bytes(), &[expected_ids[1]]);
            assert_eq!(header.scope.recovery().as_bytes(), &[expected_ids[2]]);
            assert_eq!(header.request.as_bytes(), &[expected_ids[3]]);
            assert_eq!(header.intent.version(), 2);
            assert_eq!(header.intent.digest(), &original);
            assert_eq!(header.stamp, stamp);
            assert_eq!(
                header.identity,
                EntryIdentity::new(log, PreparedDigest::new(prepared))
            );
            assert_eq!(header.encoded_entry, encoded.as_slice());
            assert_eq!(header.encoded_isolation_scope, scope.as_slice());
        }
        assert_eq!((charge.g(), charge.p()), (before + 4_423, 0));
    }
    let exact = AdmissionLimits::new(1_048_576, 135, 1_000)?;
    let value = verify_complete_core_policy_patch_frame(&base, limits, exact, &mut charge)?;
    assert_core_patch_default_frame(&value, &base, intent);
    let before = charge.g();
    let tight = AdmissionLimits::new(1_048_576, 134, 1_000)?;
    let result = verify_complete_core_policy_patch_frame(&base, limits, tight, &mut charge);
    let error = CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit);
    assert_eq!(result.err().ok_or("expected frame cap refusal")?, error);
    assert_eq!((charge.g(), charge.p()), (before, 0));
    Ok(())
}

#[test]
fn core_patch_frame_rejects_noncanonical_partial_and_late_structure() -> TestResult {
    let limits = PolicySchemaLimits::new(PolicyLimits::new(2)?, 20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_core_patch_frame_prefix_refusals(limits, &mut charge)?;
    let prefix = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0\0\0\0\x01t\0\0\0\0\x01r";
    let tag = CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::UnsupportedTag);
    for (tail, fee) in [
        (b"\x01\x02\0\0\0\0\0\0\0".as_slice(), 1_537),
        (b"\0\0\x01\0\x01\x01\x04\0\0\0\0\0\0\0\0", 2_054),
        (b"\0\0\0\0\0\0\0\x01\0\0\0\x01a\x01\x01\x02\0\0", 2_063),
        (b"\0\0\0\0\0\0\0\x01\0\0\0\x01a\x02", 1_805),
    ] {
        let intent = [prefix.as_slice(), tail].concat();
        let encoded = core_patch_frame_bytes(&intent)?;
        assert_core_patch_frame_refusal(&encoded, tag, fee, limits, &mut charge)?;
    }
    for keys in [*b"aa", *b"ba"] {
        let mut intent = prefix.to_vec();
        intent.extend_from_slice(b"\0\0\0\0\0\0\0\x02");
        for key in keys {
            intent.extend_from_slice(&[0, 0, 0, 1, key, 0]);
        }
        let encoded = core_patch_frame_bytes(&intent)?;
        let error = CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Malformed);
        assert_core_patch_frame_refusal(&encoded, error, 2_196, limits, &mut charge)?;
    }
    let tails: [&[u8]; 4] = [
        b"\x01\0\0\0\0\0\0\0\0\0\0\0\0",
        b"\0\0\x01\0\0\x01\0\0\0\0\0\0\0\0\0",
        b"\0\0\0\0\0\0\0\x01\0\0\0\x01a\x01\x02\0\0",
        b"\0\0\0\0\0\0\0\x04\0\0\0\x01\0\0\0\0\0\x01a\0\0\0\0\x01b\0\0\0\0\x01\xff\0",
    ];
    let hashes = [
        b"\x6c\x24\x92\xef\xff\xcd\x11\xa0\xf8\x61\xfb\xbc\x26\x33\x08\x56\x77\xd2\x79\xc3\x27\x31\x74\xa6\x3b\x90\xaa\x18\x6d\xe7\x33\x67",
        b"\x61\x8b\xdd\x50\x77\x0c\x6f\x90\x35\x74\x3d\xca\x77\x15\xc7\x6e\xad\xf9\x0d\xab\x6b\xf4\x71\x88\xd8\xe3\xba\x0a\x95\x95\x5d\x7a",
        b"\xe4\xd6\xb3\xad\x6a\xca\x39\xdd\x86\x9d\x1a\xf4\x4b\xfd\xc7\x9e\x92\x6d\x25\xdf\x7d\x87\xc7\x7b\x67\x6a\x67\xa3\xdb\xac\xcb\x7c",
        b"\x97\xec\x0a\xec\xf0\x2c\xa4\xe7\x8e\x22\xaf\xf3\x45\x92\xce\xb3\xea\xf6\xfa\x7d\xa4\x87\xfb\xbf\xa5\x9a\x43\x97\x7e\x81\xfc\xfe",
    ];
    for (case, tail) in tails.into_iter().enumerate() {
        let intent = [prefix.as_slice(), tail].concat();
        let encoded = core_patch_frame_bytes(&intent)?;
        assert_eq!(&<[u8; 32]>::from(Sha256::digest(&encoded)), hashes[case]);
        let before = charge.g();
        let value = verify_complete_core_policy_patch_frame(
            &encoded,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )?;
        assert_core_patch_default_frame(&value, &encoded, &intent);
        assert_eq!(
            (charge.g(), charge.p()),
            (before + [4_694, 4_956, 4_834, 5_909][case], 0)
        );
    }
    let mut intent = prefix.to_vec();
    intent.truncate(39);
    intent.extend_from_slice(b"\0\0\0\0\0\0\0\0\0\0\0\0");
    let encoded = core_patch_frame_bytes(&intent)?;
    assert_eq!(
        &<[u8; 32]>::from(Sha256::digest(&encoded)),
        b"\x93\x01\xe2\xe1\x40\x47\xd9\x4e\x98\x58\x7d\xfb\x78\xd4\xc1\xc2\x81\xd5\x3b\x94\x9c\x0f\x86\xd6\x02\xe2\x46\xc7\x8f\x2c\xea\xd3"
    );
    let before = charge.g();
    let value = verify_complete_core_policy_patch_frame(
        &encoded,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_core_patch_default_frame(&value, &encoded, &intent);
    assert_eq!((charge.g(), charge.p()), (before + 4_420, 0));
    Ok(())
}

#[test]
fn core_patch_frame_rederives_intent_before_scope_and_frame_hash() -> TestResult {
    let intent = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0\0\0\0\x01t\0\0\0\0\x01r\0\0\0\0\0\0\0\0";
    let mut encoded = core_patch_frame_bytes(intent)?;
    encoded[29] ^= 1;
    let limits = profile(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_core_patch_frame_refusal(
        &encoded,
        CompletePolicyRequestError::IntentMismatch,
        2_045,
        limits,
        &mut charge,
    )?;
    let tight_limits = profile(2_044)?;
    let mut tight_charge = PolicySchemaCharge::new(tight_limits);
    let result = verify_complete_core_policy_patch_frame(
        &encoded,
        tight_limits,
        AdmissionLimits::default(),
        &mut tight_charge,
    );
    let expected = CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
        PolicySchemaError::WorkBytes {
            bytes: 2_045,
            maximum: 2_044,
        },
    ));
    assert_eq!(
        result.err().ok_or("expected comparison work refusal")?,
        expected
    );
    assert_eq!(
        (tight_charge.g(), tight_charge.p(), tight_charge.total()),
        (1_853, 0, 1_853)
    );
    assert_core_patch_frame_scope_limits(limits, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_frame_keeps_original_bytes_borrowed_without_patch_materialization() -> TestResult {
    let intent = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0\0\0\0\x01t\0\0\0\0\x01r\0\0\0\0\0\0\0\0";
    let encoded = core_patch_frame_bytes(intent)?;
    assert_eq!(
        &<[u8; 32]>::from(Sha256::digest(&encoded)),
        b"\x67\x6f\x37\x56\x04\x99\xd1\xe1\x8b\x48\x56\x02\x7d\x6f\x7f\x79\x46\x38\xd5\x67\xe7\x50\x5f\x8b\x86\x62\xe0\x73\xf3\xd1\x6d\x8d"
    );
    let (header, value) = {
        let limits = profile(20_000_000)?;
        let admission = AdmissionLimits::default();
        let mut charge = PolicySchemaCharge::new(limits);
        let recomputed: [u8; 32] = Sha256::digest(intent).into();
        let scanned =
            scan_complete_core_policy_patch_frame(&encoded, limits, admission, &mut charge)?;
        assert_eq!(scanned.original_intent(), intent.as_slice());
        assert_eq!((charge.g(), charge.p()), (1_853, 0));
        assert_eq!(scanned.original_intent().as_ptr(), encoded[83..].as_ptr());
        let header = restore_complete_core_policy_patch_frame_header(
            scanned,
            recomputed,
            limits,
            admission,
            &mut charge,
        )?;
        assert_eq!((charge.g(), charge.p()), (4_423, 0));
        let value =
            verify_complete_core_policy_patch_frame(&encoded, limits, admission, &mut charge)?;
        assert_core_patch_default_frame(&value, &encoded, intent);
        assert_eq!((charge.g(), charge.p(), charge.total()), (8_846, 0, 8_846));
        for case in [0, 1] {
            let before = charge.g();
            let scanned =
                scan_complete_core_policy_patch_frame(&encoded, limits, admission, &mut charge)?;
            let supplied_limits = if case == 0 {
                profile(19_999_999)?
            } else {
                limits
            };
            let supplied_admission = if case == 1 {
                AdmissionLimits::new(1_048_576, 134, 1_000)?
            } else {
                admission
            };
            let result = restore_complete_core_policy_patch_frame_header(
                scanned,
                recomputed,
                supplied_limits,
                supplied_admission,
                &mut charge,
            );
            let error = if case == 0 {
                CompletePolicyEncodingError::Schema(PolicySchemaError::ProfileMismatch)
            } else {
                CompletePolicyEncodingError::ByteLimit
            };
            let expected = CompletePolicyRequestError::Encoding(error);
            assert_eq!(
                result.err().ok_or("expected restore metadata refusal")?,
                expected
            );
            assert_eq!((charge.g(), charge.p()), (before + 1_853, 0));
        }
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (12_552, 0, 12_552)
        );
        (header, value)
    };
    assert_eq!(header.encoded(), encoded.as_slice());
    assert_eq!(header.original_intent(), intent.as_slice());
    assert_eq!(header.scope().tenant().as_bytes(), b"t");
    assert_eq!(header.scope().group().as_bytes(), b"g");
    assert_eq!(header.scope().recovery().as_bytes(), b"r");
    assert_eq!(header.request().as_bytes(), b"q");
    assert_eq!(
        header.stamp(),
        CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?
    );
    assert_eq!(header.intent().version(), 2);
    let recomputed: [u8; 32] = Sha256::digest(intent).into();
    assert_eq!(header.intent().digest(), &recomputed);
    assert_core_patch_default_frame(&value, &encoded, intent);
    assert_eq!(header.encoded().as_ptr(), encoded.as_ptr());
    assert_eq!(header.original_intent().as_ptr(), encoded[83..].as_ptr());
    assert_eq!(value.encoded().as_ptr(), encoded.as_ptr());
    assert_eq!(value.original_intent().as_ptr(), encoded[83..].as_ptr());
    Ok(())
}

#[test]
fn core_patch_frame_freezes_same_account_refusal_and_repeat_fees() -> TestResult {
    let minimal = b"WF-INTENT\0COMPLETE-CORE-PATCH\0V2\0\0\0\0\x01t\0\0\0\0\x01r\0\0\0\0\0\0\0\0";
    let tail = [
        b"\x01\x01".as_slice(),
        &[255; 8],
        b"\x01",
        &[255; 8],
        b"\x01\x01\xff\xff\x01\x01\x03\x01",
        &[255; 8],
        b"\x01",
        &[255; 8],
        b"\x01\xff\x01\0\0\0\x02\0\xff\x01\x01",
        &[255; 8],
        b"\x01",
        &[255; 4],
        b"\0\0\0\0",
    ]
    .concat();
    let wide = [&minimal[..44], tail.as_slice()].concat();
    let minimal_frame = core_patch_frame_bytes(minimal)?;
    let wide_frame = core_patch_frame_bytes(&wide)?;
    assert_eq!(
        &<[u8; 32]>::from(Sha256::digest(&minimal_frame)),
        b"\x67\x6f\x37\x56\x04\x99\xd1\xe1\x8b\x48\x56\x02\x7d\x6f\x7f\x79\x46\x38\xd5\x67\xe7\x50\x5f\x8b\x86\x62\xe0\x73\xf3\xd1\x6d\x8d"
    );
    assert_eq!(
        &<[u8; 32]>::from(Sha256::digest(&wide_frame)),
        b"\x4f\xe5\xc2\x7a\x1c\x72\x4e\x51\xf3\xf8\x8a\x11\xc2\x97\x06\x41\xa8\xd9\x0e\xcd\x02\x46\x9c\x0e\x7b\x41\x74\x92\x07\x76\x2a\x3b"
    );
    for (maximum, target, attempts, peak, widths) in [
        (4_423, 4_423, 1, false, false),
        (4_422, 4_423, 1, false, false),
        (8_846, 8_846, 2, false, false),
        (8_845, 8_846, 2, false, false),
        (6_415, 6_415, 1, true, false),
        (6_414, 6_415, 1, true, false),
        (6_791, 6_791, 1, false, true),
        (6_790, 6_791, 1, false, true),
    ] {
        let (encoded, intent) = if widths {
            (wide_frame.as_slice(), wide.as_slice())
        } else {
            (minimal_frame.as_slice(), minimal.as_slice())
        };
        assert_core_patch_frame_account_case(encoded, intent, maximum, target, attempts, peak)?;
    }
    let limits = profile(127)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let result = verify_complete_core_policy_patch_frame(
        &minimal_frame,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    );
    let expected = CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
        PolicySchemaError::WorkBytes {
            bytes: 128,
            maximum: 127,
        },
    ));
    assert_eq!(
        result.err().ok_or("expected first root work refusal")?,
        expected
    );
    assert_eq!((charge.g(), charge.p(), charge.total()), (0, 0, 0));
    Ok(())
}
