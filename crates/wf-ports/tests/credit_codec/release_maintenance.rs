//! Independent native maintenance bytes and source prefix/completion boundaries.
use super::TestResult;
use sha2::{Digest, Sha256};
use wf_domain::{
    governance::release::{
        ReleaseMaintenanceRejection as J, ReleaseMaintenanceReply as R,
        ReleaseMaintenanceRequest as C, ReleaseOutboxError, ReleaseOutboxState,
        prepare_release_maintenance,
    },
    workflow::{
        admission::AdmissionLimits,
        request::{ClientIntent, RequestScope, RetainedStatus},
    },
};
use wf_ports::log::{
    CodecError as E, CodecLimits, catalog_ack_retained_summary, decode_release_maintenance_reply,
    encode_release_maintenance_reply, encode_release_maintenance_request, prepare_claim_phase,
    prepare_release_maintenance_request, release_maintenance_retained_summary,
};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity},
};
fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    ))
}
fn limits() -> CodecLimits {
    CodecLimits::new(AdmissionLimits::default())
}
fn raw(out: &mut Vec<u8>, value: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}
fn frame(command: &C) -> TestResult<wf_ports::log::PreparedBytes> {
    Ok(encode_release_maintenance_request(
        &scope()?,
        &RequestId::new(b"request".to_vec())?,
        CommittedTime::new(HlcTimestamp::new(100, 0)?, 100)?,
        command,
        limits(),
        65536,
    )?)
}

#[test]
fn release_maintenance_ack_matches_independent_native_bytes_and_original_owned_parts() -> TestResult
{
    let command = C::Acknowledge {
        expected_head: 1,
        up_to: 2,
    };
    let encoded = frame(&command)?;
    let mut body = vec![0];
    body.extend_from_slice(&1_u64.to_be_bytes());
    body.extend_from_slice(&2_u64.to_be_bytes());
    let mut hash = Sha256::new();
    hash.update(b"wf-release-maintenance-intent-v1\0");
    hash.update(&body);
    let mut expected = b"FWCP\0\x01\x10".to_vec();
    for part in [b"t".as_slice(), b"g", b"r", b"request"] {
        raw(&mut expected, part)?;
    }
    expected.extend_from_slice(&18_u16.to_be_bytes());
    expected.extend_from_slice(&hash.finalize());
    expected.extend_from_slice(&100_u64.to_be_bytes());
    expected.extend_from_slice(&0_u16.to_be_bytes());
    expected.extend_from_slice(&100_u64.to_be_bytes());
    expected.extend_from_slice(&body);
    assert_eq!(encoded.as_bytes(), expected);
    let decoded = prepare_release_maintenance_request(encoded.as_bytes(), limits(), 65536)?;
    assert_eq!(decoded.command(), &command);
    assert_eq!(
        decoded
            .header(LogIdentity::new(1, 1, AppliedPosition::new(0)))
            .intent
            .version(),
        18
    );
    let (namespace, request, intent, original) = decoded.into_retained_parts();
    assert_eq!(namespace, scope()?);
    assert_eq!(request.as_bytes(), b"request");
    assert_eq!(intent.version(), 18);
    assert_eq!(original, command);
    assert!(prepare_claim_phase(encoded.as_bytes(), limits(), 65536).is_err());
    let stamp = CommittedTime::new(HlcTimestamp::new(100, 0)?, 100)?;
    let (id, canonical, digest) = wf_ports::log::verify_release_maintenance_request(
        encoded.as_bytes(),
        &scope()?,
        stamp,
        &command,
        limits(),
    )?;
    assert_eq!(id, b"request");
    assert_eq!(canonical, intent);
    assert_eq!(
        digest,
        prepare_release_maintenance_request(encoded.as_bytes(), limits(), 65536)?
            .header(LogIdentity::new(1, 1, AppliedPosition::new(0)))
            .identity
            .prepared()
    );
    assert!(
        wf_ports::log::verify_release_maintenance_request(
            encoded.as_bytes(),
            &scope()?,
            stamp,
            &C::Acknowledge {
                expected_head: 1,
                up_to: 1
            },
            limits()
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn release_maintenance_complete_frame_checks_all_original_sequences_and_complete_eof() -> TestResult
{
    let command = C::MarkCompleted {
        sequences: vec![3, 1],
    };
    let encoded = frame(&command)?;
    assert_eq!(
        prepare_release_maintenance_request(encoded.as_bytes(), limits(), 65536)?.command(),
        &command
    );
    for end in 0..encoded.as_bytes().len() {
        assert!(
            prepare_release_maintenance_request(&encoded.as_bytes()[..end], limits(), 65536)
                .is_err()
        );
    }
    let mut tail = encoded.as_bytes().to_vec();
    tail.push(0);
    assert_eq!(
        prepare_release_maintenance_request(&tail, limits(), 65536).err(),
        Some(E::TrailingBytes)
    );
    assert_eq!(
        prepare_release_maintenance_request(encoded.as_bytes(), limits(), 1).err(),
        Some(E::OwnedLimit)
    );
    assert!(
        frame(&C::MarkCompleted {
            sequences: vec![1, 1]
        })
        .is_err()
    );
    assert!(frame(&C::MarkCompleted { sequences: vec![] }).is_err());
    assert!(
        frame(&C::Acknowledge {
            expected_head: 1,
            up_to: 257
        })
        .is_err()
    );
    Ok(())
}
#[test]
fn release_maintenance_reply_is_fixed_complete_and_closed_to_old_receipt_profiles() -> TestResult {
    let value = R::Accepted {
        meta: ReleaseOutboxState { head: 3, tail: 9 },
        affected: 2,
    };
    let mut expected = b"FWRM\0\x01\0".to_vec();
    expected.extend_from_slice(&3_u64.to_be_bytes());
    expected.extend_from_slice(&9_u64.to_be_bytes());
    expected.extend_from_slice(&2_u16.to_be_bytes());
    assert_eq!(encode_release_maintenance_reply(value, 25, 1024)?, expected);
    assert_eq!(
        decode_release_maintenance_reply(&expected, 25, 1024)?,
        value
    );
    assert_eq!(
        release_maintenance_retained_summary(
            ClientIntent::workflow_release_maintenance([1; 32]),
            15,
            &expected,
            AdmissionLimits::default()
        )?,
        (RetainedStatus::Accepted, None)
    );
    assert!(
        catalog_ack_retained_summary(
            ClientIntent::workflow_release_maintenance([1; 32]),
            15,
            &expected,
            AdmissionLimits::default()
        )
        .is_err()
    );
    for end in 0..expected.len() {
        assert!(decode_release_maintenance_reply(&expected[..end], 25, 1024).is_err());
    }
    let mut tail = expected.clone();
    tail.push(0);
    assert!(decode_release_maintenance_reply(&tail, 26, 1024).is_err());
    assert_eq!(
        decode_release_maintenance_reply(&expected, 24, 1024).err(),
        Some(E::ByteLimit)
    );
    assert_eq!(
        decode_release_maintenance_reply(&expected, 25, 1023).err(),
        Some(E::OwnedLimit)
    );
    Ok(())
}
#[test]
fn release_maintenance_pure_plan_keeps_idempotent_prefix_and_original_completion_validation()
-> TestResult {
    let command = C::Acknowledge {
        expected_head: 1,
        up_to: 1,
    };
    assert_eq!(
        prepare_release_maintenance(ReleaseOutboxState { head: 3, tail: 2 }, &command)?.reply(),
        R::Accepted {
            meta: ReleaseOutboxState { head: 3, tail: 2 },
            affected: 0
        }
    );
    let duplicate = C::MarkCompleted {
        sequences: vec![1, 1],
    };
    assert_eq!(
        prepare_release_maintenance(ReleaseOutboxState { head: 3, tail: 2 }, &duplicate)?.reply(),
        R::Rejected {
            meta: ReleaseOutboxState { head: 3, tail: 2 },
            reason: J::InvalidCompletion
        }
    );
    assert_eq!(
        prepare_release_maintenance(ReleaseOutboxState { head: 0, tail: 0 }, &command).err(),
        Some(ReleaseOutboxError::Corrupt)
    );
    Ok(())
}
