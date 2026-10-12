//! Original core-Patch frame producer literals, boundaries and owned output.

use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyRequestError, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, prepare_complete_core_policy_patch_frame_caller,
};
use wf_domain::policy::definition::{FieldPatch, PolicyPatch, PolicyVersion, RetryPatch};
use wf_domain::policy::installation::PolicyLimits;
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, PreparedDigest};
use wf_domain::workflow::request::{ClientIntent, RequestValueError};
use wf_ports::log::{
    prepare_complete_core_policy_patch_frame, verify_complete_core_policy_patch_frame,
};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};

use super::super::{TestResult, minimal_data, profile};

pub(super) mod cases;
pub(super) mod literals;

use cases::{
    assert_core_patch_prepare_account, assert_core_patch_prepare_backoff_kinds,
    assert_core_patch_prepare_global_bytes, assert_core_patch_prepare_raw_caps,
    assert_core_patch_prepare_scope_limits, assert_core_patch_prepare_state_presence,
    assert_core_patch_prepared_literal, core_patch_prepare_command,
    core_patch_prepare_equal_current, core_patch_prepare_input, core_patch_prepare_literal_frame,
    decode_core_patch_literal,
};
use literals::{FROZEN_CORE_PATCH_FRAMES, FROZEN_CORE_PATCH_HEADERS};

#[test]
fn core_patch_prepare_matches_frozen_whole_frames_and_prepared_hashes() -> TestResult {
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (case, (frame, hash, producer_fee, verifier_fee)) in
        FROZEN_CORE_PATCH_FRAMES.iter().enumerate()
    {
        let expected = decode_core_patch_literal(frame)?;
        let digest: [u8; 32] = decode_core_patch_literal(hash)?
            .try_into()
            .map_err(|_| "wrong fixed prepared digest length")?;
        let input = core_patch_prepare_input(
            core_patch_prepare_command(case)?,
            [b"t", b"g", b"r", b"q"],
            stamp,
        )?;
        let before = charge.g();
        let prepared =
            prepare_complete_core_policy_patch_frame(input, limits, admission, &mut charge)?;
        assert_core_patch_prepared_literal(&prepared, &expected);
        assert_eq!(prepared.digest(), PreparedDigest::new(digest));
        assert_eq!((charge.g(), charge.p()), (before + producer_fee, 0));
        let verified = verify_complete_core_policy_patch_frame(
            prepared.as_bytes(),
            limits,
            admission,
            &mut charge,
        )?;
        let log = LogIdentity::new(1, 2, AppliedPosition::new(3));
        let header = verified.header(log);
        assert_eq!(verified.original_intent(), &expected[83..]);
        assert_eq!(verified.encoded(), expected.as_slice());
        assert_eq!(verified.digest(), prepared.digest());
        assert_eq!(header.scope.tenant().as_bytes(), b"t");
        assert_eq!(header.scope.group().as_bytes(), b"g");
        assert_eq!(header.scope.recovery().as_bytes(), b"r");
        assert_eq!(header.request.as_bytes(), b"q");
        assert_eq!(header.format_version, 1);
        assert_eq!(header.intent.version(), 2);
        assert_eq!(header.intent.digest().as_slice(), &expected[29..61]);
        assert_eq!(header.stamp, stamp);
        assert_eq!(header.identity, EntryIdentity::new(log, prepared.digest()));
        assert_eq!(
            verified.isolation_scope(),
            b"FWSC\0\x01\0\0\0\x01t\0\0\0\x01g\0\0\0\x01r"
        );
        assert_eq!(
            (charge.g(), charge.p()),
            (before + producer_fee + verifier_fee, 0)
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
fn core_patch_prepare_binds_every_header_operand_to_frozen_bytes() -> TestResult {
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let ids = [*b"tgrq", *b"tGrq", *b"tgRq", *b"tgrQ", *b"tgrq", *b"ugrq"];
    for (case, ids) in ids.into_iter().enumerate() {
        let stamp = if case == 4 {
            CommittedTime::new(HlcTimestamp::new(99, 7)?, 98)?
        } else {
            CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?
        };
        let input = core_patch_prepare_input(
            core_patch_prepare_command(0)?,
            [&ids[0..1], &ids[1..2], &ids[2..3], &ids[3..4]],
            stamp,
        )?;
        let expected = decode_core_patch_literal(FROZEN_CORE_PATCH_HEADERS[case].0)?;
        let digest: [u8; 32] = decode_core_patch_literal(FROZEN_CORE_PATCH_HEADERS[case].1)?
            .try_into()
            .map_err(|_| "wrong header prepared digest length")?;
        let before = charge.g();
        let prepared =
            prepare_complete_core_policy_patch_frame(input, limits, admission, &mut charge)?;
        assert_core_patch_prepared_literal(&prepared, &expected);
        assert_eq!(prepared.digest(), PreparedDigest::new(digest));
        assert_eq!((charge.g(), charge.p()), (before + 4_484, 0));
        let verified = verify_complete_core_policy_patch_frame(
            prepared.as_bytes(),
            limits,
            admission,
            &mut charge,
        )?;
        for log in [
            LogIdentity::new(1, 2, AppliedPosition::new(3)),
            LogIdentity::new(9, 8, AppliedPosition::new(7)),
        ] {
            let header = verified.header(log);
            assert_eq!(header.scope.tenant().as_bytes(), &ids[0..1]);
            assert_eq!(header.scope.group().as_bytes(), &ids[1..2]);
            assert_eq!(header.scope.recovery().as_bytes(), &ids[2..3]);
            assert_eq!(header.request.as_bytes(), &ids[3..4]);
            assert_eq!(header.stamp, stamp);
            assert_eq!(header.intent.version(), 2);
            assert_eq!(header.intent.digest().as_slice(), &expected[29..61]);
            assert_eq!(header.identity, EntryIdentity::new(log, prepared.digest()));
        }
        assert_eq!((charge.g(), charge.p()), (before + 8_907, 0));
    }
    Ok(())
}

#[test]
fn core_patch_prepare_preserves_partial_presence_and_each_backoff_kind() -> TestResult {
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    let minimal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    for (field, empty_payload, extra_nodes) in [
        (0, &[1, 0, 0, 0, 0, 0][..], 2_u64),
        (1, &[1, 0, 0, 0, 0, 0, 0, 0, 0][..], 1),
        (2, &[1, 0, 0, 0][..], 2),
        (3, &[1, 0, 0][..], 2),
    ] {
        for action in 0..3 {
            let mut command = core_patch_prepare_command(0)?;
            match (field, action) {
                (0, 1) => command.patch.version = FieldPatch::Clear,
                (0, 2) => command.patch.version = FieldPatch::Set(PolicyVersion::Bytes(Vec::new())),
                (1, 1) => command.patch.max_active_ms = FieldPatch::Clear,
                (1, 2) => command.patch.max_active_ms = FieldPatch::Set(0),
                (2, 1) => command.patch.retry = FieldPatch::Clear,
                (2, 2) => command.patch.retry = FieldPatch::Set(RetryPatch::default()),
                (3, 1) => command.patch.retention = FieldPatch::Clear,
                (3, 2) => command.patch.retention = FieldPatch::Set(RetentionOverride::default()),
                _ => {}
            }
            let mut intent = minimal[83..].to_vec();
            let replacement = match action {
                0 => &[0][..],
                1 => &[2][..],
                _ => empty_payload,
            };
            drop(intent.splice(44 + field..45 + field, replacement.iter().copied()));
            let expected =
                core_patch_prepare_literal_frame(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
            let nodes = 5 + if action == 2 { extra_nodes } else { 0 };
            let fee = 384 * nodes + 6 * u64::try_from(intent.len())? + 2_252;
            let input = core_patch_prepare_input(command, [b"t", b"g", b"r", b"q"], stamp)?;
            let before = charge.g();
            let prepared =
                prepare_complete_core_policy_patch_frame(input, limits, admission, &mut charge)?;
            assert_core_patch_prepared_literal(&prepared, &expected);
            assert_eq!((charge.g(), charge.p()), (before + fee, 0));
        }
    }
    assert_core_patch_prepare_state_presence(limits, &mut charge)?;
    assert_core_patch_prepare_backoff_kinds(limits, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_prepare_freezes_profile_scope_raw_and_complete_frame_priority() -> TestResult {
    let limits = PolicySchemaLimits::new(PolicyLimits::new(2)?, 20_000_000)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (decision_bytes, success) in [(135, true), (134, false)] {
        let admission = AdmissionLimits::new(1_048_576, decision_bytes, 1_000)?;
        let input = core_patch_prepare_input(
            core_patch_prepare_command(0)?,
            [b"t", b"g", b"r", b"q"],
            stamp,
        )?;
        let before = charge.g();
        let result =
            prepare_complete_core_policy_patch_frame(input, limits, admission, &mut charge);
        if success {
            let expected = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
            assert_core_patch_prepared_literal(&result?, &expected);
            assert_eq!((charge.g(), charge.p()), (before + 4_484, 0));
        } else {
            assert_eq!(
                result
                    .err()
                    .ok_or("expected measured full frame cap refusal")?,
                CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
            );
            assert_eq!((charge.g(), charge.p()), (before + 1_543, 0));
        }
    }
    let before = (charge.g(), charge.p(), charge.total());
    let input = core_patch_prepare_input(
        core_patch_prepare_command(0)?,
        [b"t", b"g", b"r", b"q"],
        stamp,
    )?;
    let wrong = PolicySchemaLimits::new(PolicyLimits::new(2)?, 20_000_001)?;
    let result = prepare_complete_core_policy_patch_frame(
        input,
        wrong,
        AdmissionLimits::default(),
        &mut charge,
    );
    assert_eq!(
        result.err().ok_or("expected initial profile mismatch")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    );
    assert_eq!((charge.g(), charge.p(), charge.total()), before);
    assert_core_patch_prepare_scope_limits(limits, &mut charge)?;
    assert_core_patch_prepare_raw_caps(limits, &mut charge)?;
    assert_core_patch_prepare_global_bytes(limits, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_prepare_preserves_each_same_account_refusal_and_prior_peak() -> TestResult {
    for (maximum, target, attempts, held_peak) in [
        (4_484, 4_484, 1, false),
        (4_483, 4_484, 1, false),
        (8_968, 8_968, 2, false),
        (8_967, 8_968, 2, false),
        (6_476, 6_476, 1, true),
        (6_475, 6_476, 1, true),
    ] {
        assert_core_patch_prepare_account(maximum, target, attempts, held_peak)?;
    }
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    for (maximum, target, retained) in [
        (127, 128, 0),
        (2_234, 2_235, 1_543),
        (2_414, 2_415, 2_235),
        (2_677, 2_678, 2_415),
        (3_528, 3_529, 2_678),
        (4_220, 4_221, 3_529),
        (4_483, 4_484, 4_221),
    ] {
        let limits = profile(maximum)?;
        let mut charge = PolicySchemaCharge::new(limits);
        let input = core_patch_prepare_input(
            core_patch_prepare_command(0)?,
            [b"t", b"g", b"r", b"q"],
            stamp,
        )?;
        let result = prepare_complete_core_policy_patch_frame(
            input,
            limits,
            AdmissionLimits::default(),
            &mut charge,
        );
        assert_eq!(
            result
                .err()
                .ok_or("expected independent prospective phase refusal")?,
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
    let limits = profile(1_543)?;
    let mut charge = PolicySchemaCharge::new(limits);
    let input = core_patch_prepare_input(
        core_patch_prepare_command(0)?,
        [b"t", b"g", b"r", b"q"],
        stamp,
    )?;
    let result = prepare_complete_core_policy_patch_frame(
        input,
        limits,
        AdmissionLimits::new(1_048_576, 134, 1_000)?,
        &mut charge,
    );
    assert_eq!(
        result
            .err()
            .ok_or("expected frame cap before stream work refusal")?,
        CompletePolicyRequestError::Encoding(CompletePolicyEncodingError::ByteLimit)
    );
    assert_eq!((charge.g(), charge.p(), charge.total()), (1_543, 0, 1_543));
    Ok(())
}

#[test]
fn core_patch_prepare_keeps_partial_identity_distinct_from_equal_merged_values() -> TestResult {
    let limits = profile(20_000_000)?;
    let admission = AdmissionLimits::default();
    let mut charge = PolicySchemaCharge::new(limits);
    let stamp = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    let current = core_patch_prepare_equal_current(limits, &mut charge)?;
    let mut expected_data = minimal_data();
    expected_data.max_active_ms = Some(7);
    let original_value = decode_core_patch_literal(
        "46575056000200000000000000010000001e465750530002000000017200010000000000000007000000000000000000",
    )?;
    let literal = decode_core_patch_literal(FROZEN_CORE_PATCH_FRAMES[0].0)?;
    let input = core_patch_prepare_input(
        core_patch_prepare_command(0)?,
        [b"t", b"g", b"r", b"q"],
        stamp,
    )?;
    let before = charge.g();
    let first = prepare_complete_core_policy_patch_frame(input, limits, admission, &mut charge)?;
    assert_core_patch_prepared_literal(&first, &literal);
    assert_eq!(charge.g(), before + 4_484);
    let mut same = core_patch_prepare_command(0)?;
    same.patch.max_active_ms = FieldPatch::Set(7);
    let mut intent = literal[83..].to_vec();
    drop(intent.splice(45..46, [1, 0, 0, 0, 0, 0, 0, 0, 7]));
    let expected = core_patch_prepare_literal_frame(&intent, [b"t", b"g", b"r", b"q"], stamp)?;
    let input = core_patch_prepare_input(same, [b"t", b"g", b"r", b"q"], stamp)?;
    let before = charge.g();
    let second = prepare_complete_core_policy_patch_frame(input, limits, admission, &mut charge)?;
    assert_core_patch_prepared_literal(&second, &expected);
    assert_eq!(charge.g(), before + 4_916);
    assert_ne!(first.as_bytes(), second.as_bytes());
    assert_ne!(first.digest(), second.digest());
    assert!(current.data() == &expected_data);
    assert_eq!(current.generation(), 1);
    assert_eq!(current.encoded(), original_value.as_slice());
    assert_eq!(charge.p(), 0);
    let input = core_patch_prepare_input(
        core_patch_prepare_command(0)?,
        [b"t", b"g", b"r", b"q"],
        stamp,
    )?;
    let before = charge.g();
    let (caller, intent_bytes, frame_bytes) =
        prepare_complete_core_policy_patch_frame_caller(input, limits, admission, &mut charge)?;
    assert_eq!((intent_bytes, frame_bytes), (52, 135));
    assert_eq!(caller.input().command().patch, PolicyPatch::default());
    assert_eq!(caller.input().scope().tenant().as_bytes(), b"t");
    assert_eq!(caller.input().scope().group().as_bytes(), b"g");
    assert_eq!(caller.input().scope().recovery().as_bytes(), b"r");
    assert_eq!(caller.input().request().as_bytes(), b"q");
    assert_eq!(caller.input().stamp(), stamp);
    assert!(caller.schema_limits() == limits);
    assert_eq!(caller.admission_limits(), admission);
    assert_eq!((charge.g(), charge.p()), (before + 4_484, 0));
    assert_eq!(
        ClientIntent::new(2, [0; 32]),
        Err(RequestValueError::UnsupportedVersion)
    );
    assert_eq!(ClientIntent::new(1, [0; 32])?.version(), 1);
    Ok(())
}
