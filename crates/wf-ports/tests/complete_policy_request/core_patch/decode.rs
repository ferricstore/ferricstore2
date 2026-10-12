//! Consuming original-command reconstruction, fixed literals and paid ownership.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchRequestInput, CompletePolicyEncodingError, CompletePolicyRequestError,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, prepare_complete_core_policy_patch,
};
use wf_domain::policy::definition::FieldPatch;
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_ports::log::{
    decode_complete_core_policy_patch_frame, prepare_complete_core_policy_patch_frame,
};
use wf_types::version::{CommittedTime, HlcTimestamp};

use super::super::{TestResult, minimal_data, profile};
use super::prepare::cases::{
    core_patch_prepare_command, core_patch_prepare_equal_current, decode_core_patch_literal,
};
use super::prepare::literals::{FROZEN_CORE_PATCH_FRAMES, FROZEN_CORE_PATCH_HEADERS};

mod boundaries;
mod cases;

use boundaries::assert_core_patch_decode_boundaries;
use cases::{
    CORE_PATCH_RESTORATION_FEES, assert_core_patch_decode_account,
    assert_core_patch_decode_domain_owners, assert_core_patch_decode_presence,
    assert_core_patch_decoded,
};

#[test]
fn core_patch_decode_matches_frozen_original_commands_and_paid_reproduction() -> TestResult {
    let limits = PolicySchemaLimits::new(PolicyLimits::new(2)?, 20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (case, (frame, hash, producer_fee, verifier_fee)) in
        FROZEN_CORE_PATCH_FRAMES.iter().enumerate()
    {
        let expected = decode_core_patch_literal(frame)?;
        let command = core_patch_prepare_command(case)?;
        let before = charge.g();
        let decoded =
            decode_complete_core_policy_patch_frame(&expected, limits, admission, &mut charge)?;
        assert_core_patch_decoded(
            &decoded,
            &expected,
            hash,
            &command,
            [b"t", b"g", b"r", b"q"],
            stamp,
        )?;
        assert_eq!(
            (charge.g(), charge.p()),
            (before + verifier_fee + CORE_PATCH_RESTORATION_FEES[case], 0)
        );
        assert_eq!(decoded.encoded().as_ptr(), expected.as_ptr());
        assert_eq!(decoded.original_intent().as_ptr(), expected[83..].as_ptr());
        let (scope, request, moved_stamp, moved) = decoded.into_input().into_parts();
        assert_eq!(scope.tenant().as_bytes(), b"t");
        assert_eq!(scope.group().as_bytes(), b"g");
        assert_eq!(scope.recovery().as_bytes(), b"r");
        assert_eq!(request.as_bytes(), b"q");
        assert_eq!(moved_stamp, stamp);
        assert_eq!(moved.workflow_type, command.workflow_type);
        assert_eq!(moved.expected_generation, command.expected_generation);
        assert_eq!(moved.patch, command.patch);
        let input = CompleteCorePolicyPatchRequestInput::new(scope, request, moved_stamp, moved);
        let before = charge.g();
        let reproduced =
            prepare_complete_core_policy_patch_frame(input, limits, admission, &mut charge)?;
        assert_eq!(reproduced.as_bytes(), expected.as_slice());
        assert_eq!(
            reproduced.digest(),
            wf_domain::workflow::control::PreparedDigest::new(Sha256::digest(&expected).into())
        );
        assert_eq!((charge.g(), charge.p()), (before + producer_fee, 0));
    }
    Ok(())
}

#[test]
fn core_patch_decode_preserves_every_header_and_moves_domain_owners() -> TestResult {
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    for (case, ids) in [*b"tgrq", *b"tGrq", *b"tgRq", *b"tgrQ", *b"tgrq", *b"ugrq"]
        .into_iter()
        .enumerate()
    {
        let stamp = CommittedTime::new(
            HlcTimestamp::new(
                if case == 4 { 99 } else { 0 },
                if case == 4 { 7 } else { 0 },
            )?,
            if case == 4 { 98 } else { 0 },
        )?;
        let expected = decode_core_patch_literal(FROZEN_CORE_PATCH_HEADERS[case].0)?;
        let before = charge.g();
        let decoded =
            decode_complete_core_policy_patch_frame(&expected, limits, admission, &mut charge)?;
        assert_core_patch_decoded(
            &decoded,
            &expected,
            FROZEN_CORE_PATCH_HEADERS[case].1,
            &core_patch_prepare_command(0)?,
            [&ids[0..1], &ids[1..2], &ids[2..3], &ids[3..4]],
            stamp,
        )?;
        assert_eq!((charge.g(), charge.p()), (before + 5_500, 0));
        assert_eq!(decoded.encoded().as_ptr(), expected.as_ptr());
    }
    assert_core_patch_decode_domain_owners(limits, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_decode_preserves_presence_and_each_partial_backoff_kind() -> TestResult {
    let limits = profile(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    assert_core_patch_decode_presence(limits, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_decode_refuses_wire_before_original_materialization() -> TestResult {
    let limits = PolicySchemaLimits::new(PolicyLimits::new(2)?, 20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let literal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let mut bad_digest = literal.clone();
    bad_digest[29] ^= 1;
    let wire = |error| CompletePolicyRequestError::Encoding(error);
    let before = (charge.g(), charge.p(), charge.total());
    let result = decode_complete_core_policy_patch_frame(
        &bad_digest,
        profile(20_000_001)?,
        AdmissionLimits::new(1_048_576, 134, 1_000)?,
        &mut charge,
    );
    assert_eq!(
        result.err().ok_or("expected profile before cap/wire")?,
        wire(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    );
    assert_eq!((charge.g(), charge.p(), charge.total()), before);
    let result = decode_complete_core_policy_patch_frame(
        &bad_digest,
        limits,
        AdmissionLimits::new(1_048_576, 134, 1_000)?,
        &mut charge,
    );
    assert_eq!(
        result.err().ok_or("expected raw full-frame cap")?,
        wire(CompletePolicyEncodingError::ByteLimit)
    );
    assert_eq!((charge.g(), charge.p(), charge.total()), before);
    assert_core_patch_decode_boundaries(limits, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_decode_preserves_each_restoration_debit_repeat_and_prior_peak() -> TestResult {
    for (maximum, target, attempts, peak) in [
        (5_500, 5_500, 1, false),
        (5_499, 5_500, 1, false),
        (11_000, 11_000, 2, false),
        (10_999, 11_000, 2, false),
        (7_492, 7_492, 1, true),
        (7_491, 7_492, 1, true),
    ] {
        assert_core_patch_decode_account(maximum, target, attempts, peak)?;
    }
    let encoded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    for (target, retained) in [
        (4_423, 4_160),
        (4_551, 4_423),
        (4_679, 4_551),
        (4_712, 4_679),
        (4_840, 4_712),
        (4_844, 4_840),
        (4_845, 4_844),
        (4_846, 4_845),
        (4_974, 4_846),
        (5_102, 4_974),
        (5_106, 5_102),
        (5_107, 5_106),
        (5_236, 5_107),
        (5_237, 5_236),
        (5_238, 5_237),
        (5_239, 5_238),
        (5_240, 5_239),
        (5_244, 5_240),
        (5_372, 5_244),
        (5_500, 5_372),
    ] {
        let maximum = target - 1;
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let result = decode_complete_core_policy_patch_frame(
            &encoded,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        );
        assert_eq!(
            result
                .err()
                .ok_or("expected distinct reconstruction-event refusal")?,
            CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
                PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum
                }
            ))
        );
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (retained, 0, retained)
        );
    }
    for (case, target, retained) in [(7, 9_796, 9_795), (9, 9_452, 9_451)] {
        let encoded = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[case].0)?;
        let limits = profile(target - 1)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let result = decode_complete_core_policy_patch_frame(
            &encoded,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        );
        assert_eq!(
            result
                .err()
                .ok_or("expected final structured-row debit refusal")?,
            CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
                PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum: target - 1
                }
            ))
        );
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (retained, 0, retained)
        );
    }
    Ok(())
}

#[test]
fn core_patch_decode_keeps_original_identity_distinct_from_equal_merged_values() -> TestResult {
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let current = core_patch_prepare_equal_current(limits, &mut charge)?;
    let mut expected_data = minimal_data();
    expected_data.max_active_ms = Some(7);
    let gen1 = decode_core_patch_literal(
        "46575056000200000000000000010000001e465750530002000000017200010000000000000007000000000000000000",
    )?;
    let gen2 = decode_core_patch_literal(
        "46575056000200000000000000020000001e465750530002000000017200010000000000000007000000000000000000",
    )?;
    let second = decode_core_patch_literal(
        "46574350000105000000017400000001670000000172000000017100022c36a05a4fc6a1e7b9f11304286dcf6578b99d2c8c57304bf2bdd01a19b9e7f00000000000000000000000000000000000000000003c57462d494e54454e5400434f4d504c4554452d434f52452d504154434800563200000000017400000000017200010000000000000007000000000000",
    )?;
    let first = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let mut identities = Vec::new();
    for (index, (encoded, hash, fee)) in [
        (&first, FROZEN_CORE_PATCH_FRAMES[0].1, 5_500),
        (
            &second,
            "bfc10eb864b5aed7a6318c79daf4275c12b51dc35fec8f286750f1bf0b001ca8",
            5_788,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let mut expected = core_patch_prepare_command(0)?;
        if index == 1 {
            expected.patch.max_active_ms = FieldPatch::Set(7);
        }
        let before = charge.g();
        let decoded =
            decode_complete_core_policy_patch_frame(encoded, limits, admission, &mut charge)?;
        assert_core_patch_decoded(
            &decoded,
            encoded,
            hash,
            &expected,
            [b"t", b"g", b"r", b"q"],
            CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?,
        )?;
        assert_eq!((charge.g(), charge.p()), (before + fee, 0));
        identities.push((
            decoded
                .header(wf_types::version::LogIdentity::new(
                    1,
                    2,
                    wf_types::version::AppliedPosition::new(3),
                ))
                .intent,
            decoded.digest(),
        ));
        let (_, _, _, command) = decoded.into_input().into_parts();
        let merged = prepare_complete_core_policy_patch(
            Some(&current),
            0,
            command,
            1_000_000,
            limits,
            &mut charge,
        )?;
        assert_eq!(merged.generation(), 2);
        assert!(merged.data() == &expected_data);
        assert_eq!(merged.encoded(), gen2.as_slice());
        assert_eq!(current.generation(), 1);
        assert!(current.data() == &expected_data);
        assert_eq!(current.encoded(), gen1.as_slice());
        assert_eq!(charge.p(), 0);
    }
    assert_ne!(first, second);
    assert_ne!(identities[0].0, identities[1].0);
    assert_ne!(identities[0].1, identities[1].1);
    Ok(())
}
