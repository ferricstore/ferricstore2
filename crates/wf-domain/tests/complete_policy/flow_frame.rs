//! Registry-three mathematical identity, exact transport and fresh account controls.

use std::ops::Range;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as Error, CompletePolicyRequestError as RequestError,
    OriginalFlowPolicyReplaceFrameHeader, OriginalFlowPolicyReplaceInput, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits, RawFlowPolicyValue as Raw,
    prepare_original_flow_policy_replace_frame_bytes,
    restore_original_flow_policy_replace_frame_header, scan_original_flow_policy_replace_frame,
};
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RequestValueError};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, HlcTimestamp};

use super::TestResult;

mod cases;
mod refusals;

const KEY: &[u8] = b"f:{f}:policy:r";
const DIGESTS: [&str; 3] = [
    "ad8cf36588c9b2fd1b75f533212cdbfb079deb6314007f0a8760e43e51e34cd1",
    "3c6c5715729e8e6af64148e6d51b0bb1dd6c9b4aad80fb6b2e018835ed83d057",
    "517f9f95de58bb788834412968a965d5e0ed3be7dd76a53d153f487b1d5097ef",
];
const FRAME_0: &str = concat!(
    "4657435000010800000001610000000162000000016300000001710003ad8cf36588c9b2fd1b75f533212cdbfb079deb",
    "6314007f0a8760e43e51e34cd1000000000000000000000000000000000000000000500000001457462d494e54454e54",
    "004f524947494e414c2d464c4f572d5245504c4143450056310000000001610000000e663a7b667d3a706f6c6963793a",
    "7200000005000000001104000000000000000000000011000000010000000c000000140000003f00000000",
);

const FRAME_1: &str = concat!(
    "46574350000108000000016100000001620000000163000000017100033c6c5715729e8e6af64148e6d51b0bb1dd6c9b",
    "4aad80fb6b2e018835ed83d0570000000000000000000000000000000000000000007e0000002457462d494e54454e54",
    "004f524947494e414c2d464c4f572d5245504c4143450056310000000001610000000e663a7b667d3a706f6c6963793a",
    "7200000005000000003f040000000000000002000000210000002b000000300000003a0000003f0000000a0300000001",
    "6100000005000000000a0300000001620000000500000000010000000c000000240000003f0000000200000001000000",
    "000000000100000000",
);

const FRAME_2: &str = concat!(
    "4657435000010800000001610000000162000000016300000001710003517f9f95de58bb788834412968a965d5e0ed3b",
    "e7dd76a53d153f487b1d5097ef000000000000000000000000000000000000000000950000002c57462d494e54454e54",
    "004f524947494e414c2d464c4f572d5245504c4143450056310000000001610000000e663a7b667d3a706f6c6963793a",
    "720000000500000000560400000000000000030000002900000033000000380000004200000047000000510000005600",
    "00000a03000000016100000005000000000a03000000016200000005000000000a030000000163000000050000000001",
    "0000000c0000002c0000003f00000003000000020000000000000001000000010000000200000000",
);

#[test]
fn original_flow_frame_canonicalizes_mathematical_terms() -> TestResult {
    assert_golden(Raw::Nil, Raw::Map(&[]), 0, 6_473, 272)?;
    let two = [(Raw::Binary(b"b"), Raw::Nil), (Raw::Binary(b"a"), Raw::Nil)];
    assert_golden(Raw::Nil, Raw::Map(&two), 1, 13_823, 432)?;
    let three = [
        (Raw::Binary(b"c"), Raw::Nil),
        (Raw::Binary(b"a"), Raw::Nil),
        (Raw::Binary(b"b"), Raw::Nil),
    ];
    assert_golden(Raw::Nil, Raw::Map(&three), 2, 19_810, 448)?;
    cases::assert_integer_and_atom_aliases()?;
    cases::assert_list_and_recursive_map_identity()?;
    cases::assert_duplicate_semantic_keys()?;
    cases::assert_mixed_class_ranks()?;
    Ok(())
}

#[test]
fn original_flow_frame_preserves_raw_presence_and_source_order() -> TestResult {
    cases::assert_presence_and_order()?;
    cases::assert_header_binding()?;
    // This mathematical registry cannot be minted with the public registry-one ctor.
    assert_eq!(
        ClientIntent::new(3, [0; 32]),
        Err(RequestValueError::UnsupportedVersion)
    );
    Ok(())
}

#[test]
fn original_flow_frame_rejects_all_structural_tampering() -> TestResult {
    cases::assert_outer_and_node_tampering()?;
    cases::assert_scalar_canonical_tampering()?;
    cases::assert_three_cycle_and_nested_key_tampering()?;
    refusals::assert_profiles_and_visible_caps()?;
    Ok(())
}

#[test]
fn original_flow_frame_preserves_each_account_event() -> TestResult {
    refusals::assert_all_positive_events()?;
    assert_big_comparator_events()?;
    refusals::assert_continuous_owners()?;
    Ok(())
}

fn profile(work: u64) -> TestResult<PolicySchemaLimits> {
    Ok(PolicySchemaLimits::new(PolicyLimits::default(), work)?)
}

fn input<'a>(expected: Raw<'a>, patch: Raw<'a>) -> TestResult<OriginalFlowPolicyReplaceInput<'a>> {
    input_with(
        expected,
        patch,
        KEY,
        [b"a", b"b", b"c", b"q"],
        zero_stamp()?,
    )
}

fn input_with<'a>(
    expected: Raw<'a>,
    patch: Raw<'a>,
    key: &'a [u8],
    ids: [&[u8]; 4],
    stamp: CommittedTime,
) -> TestResult<OriginalFlowPolicyReplaceInput<'a>> {
    let scope = RequestScope::new(
        TenantId::new(ids[0].to_vec())?,
        GroupId::new(ids[1].to_vec())?,
        RecoveryId::new(ids[2].to_vec())?,
    );
    Ok(OriginalFlowPolicyReplaceInput::new(
        scope,
        RequestId::new(ids[3].to_vec())?,
        stamp,
        key,
        expected,
        patch,
    ))
}

fn zero_stamp() -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?)
}

fn prepare(expected: Raw<'_>, patch: Raw<'_>) -> TestResult<(Vec<u8>, usize, Range<usize>)> {
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    Ok(prepare_original_flow_policy_replace_frame_bytes(
        input(expected, patch)?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?)
}

fn intent(expected: Raw<'_>, patch: Raw<'_>) -> TestResult<Vec<u8>> {
    let (frame, _, range) = prepare(expected, patch)?;
    let limits = profile(1_000_000_000)?;
    let mut verification = PolicySchemaCharge::new(limits);
    let header = restore(&frame, [0; 32], limits, &mut verification)?;
    assert_eq!(header.original_intent(), &frame[range.clone()]);
    Ok(frame[range].to_vec())
}

fn hex(value: &str) -> TestResult<Vec<u8>> {
    let (pairs, remainder) = value.as_bytes().as_chunks::<2>();
    assert_eq!(remainder, []);
    pairs
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn fixture(case: usize) -> TestResult<Vec<u8>> {
    hex([FRAME_0, FRAME_1, FRAME_2][case])
}

fn digest(case: usize) -> TestResult<[u8; 32]> {
    hex(DIGESTS[case])?
        .try_into()
        .map_err(|_| "fixed digest width".into())
}

fn restore<'a>(
    frame: &'a [u8],
    digest: [u8; 32],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalFlowPolicyReplaceFrameHeader<'a>, RequestError> {
    let scanned =
        scan_original_flow_policy_replace_frame(frame, limits, AdmissionLimits::default(), charge)?;
    restore_original_flow_policy_replace_frame_header(
        scanned,
        digest,
        limits,
        AdmissionLimits::default(),
        charge,
    )
}

fn assert_golden(
    expected: Raw<'_>,
    patch: Raw<'_>,
    case: usize,
    work: u64,
    peak: u64,
) -> TestResult {
    let mut fixed = fixture(case)?;
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let (scaffold, offset, range) = prepare_original_flow_policy_replace_frame_bytes(
        input(expected, patch)?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!((offset, range.clone()), (29, 87..[167, 213, 236][case]));
    assert_eq!(&fixed[range.clone()], &scaffold[range]);
    // Domain emits a scaffold; ports separately performs the two real SHA operations.
    fixed[offset..offset + 32].fill(0);
    assert_eq!(scaffold, fixed);
    assert_eq!((charge.g(), charge.p()), (work, peak));
    let original = fixture(case)?;
    let mut verification = PolicySchemaCharge::new(limits);
    let header = restore(&original, digest(case)?, limits, &mut verification)?;
    assert_eq!(
        (verification.g(), verification.p()),
        ([4_683, 5_655, 6_270][case], 0)
    );
    assert_eq!(header.encoded().as_ptr(), original.as_ptr());
    assert_eq!(header.original_intent().as_ptr(), original[87..].as_ptr());
    assert_eq!(header.scope().tenant().as_bytes(), b"a");
    assert_eq!(header.scope().group().as_bytes(), b"b");
    assert_eq!(header.scope().recovery().as_bytes(), b"c");
    assert_eq!(header.request().as_bytes(), b"q");
    assert_eq!(header.stamp(), zero_stamp()?);
    assert_eq!(header.intent().version(), 3);
    assert_eq!(header.intent().digest(), &digest(case)?);
    Ok(())
}

fn assert_alias(left: Raw<'_>, right: Raw<'_>) -> TestResult {
    assert_eq!(intent(left, Raw::Map(&[]))?, intent(right, Raw::Map(&[]))?);
    Ok(())
}

fn assert_distinct(left: Raw<'_>, right: Raw<'_>) -> TestResult {
    assert_ne!(intent(left, Raw::Map(&[]))?, intent(right, Raw::Map(&[]))?);
    Ok(())
}

fn assert_malformed(frame: &[u8]) -> TestResult {
    let limits = profile(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_eq!(
        scan_original_flow_policy_replace_frame(
            frame,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        )
        .err(),
        Some(RequestError::Encoding(Error::Malformed))
    );
    Ok(())
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
}

fn assert_big_comparator_events() -> TestResult {
    // Independently frozen before implementation: both heap/adjacent Big trims are fresh.
    let rows = [
        (Raw::Integer(2), Raw::Nil),
        (
            Raw::BigInteger {
                negative: false,
                little_endian: &[1, 0],
            },
            Raw::Nil,
        ),
    ];
    for (maximum, retained, target) in [(4_236, 3_675, 4_237), (5_136, 4_575, 5_137)] {
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        assert_eq!(
            prepare_original_flow_policy_replace_frame_bytes(
                input(Raw::Nil, Raw::Map(&rows))?,
                limits,
                AdmissionLimits::default(),
                &mut charge,
            )
            .err(),
            Some(RequestError::Encoding(Error::Schema(
                PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum
                },
            )))
        );
        assert_eq!((charge.g(), charge.p()), (retained, 432));
    }
    let limits = profile(15_047)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let (frame, _, range) = prepare_original_flow_policy_replace_frame_bytes(
        input(Raw::Nil, Raw::Map(&rows))?,
        limits,
        AdmissionLimits::default(),
        &mut charge,
    )?;
    assert_eq!((frame.len(), range.clone()), (251, 87..215));
    assert_eq!((charge.g(), charge.p()), (14_615, 432));
    let normalized = [(Raw::Integer(1), Raw::Nil), (Raw::Integer(2), Raw::Nil)];
    assert_eq!(&frame[range], intent(Raw::Nil, Raw::Map(&normalized))?);
    let mut verification = PolicySchemaCharge::new(limits);
    let header = restore(&frame, [0; 32], limits, &mut verification)?;
    assert_eq!(header.encoded().as_ptr(), frame.as_ptr());
    assert_eq!((verification.g(), verification.p()), (5_661, 0));
    Ok(())
}
