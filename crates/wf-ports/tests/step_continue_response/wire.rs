use super::{fixtures as f, *};
use sha2::{Digest, Sha256};
use wf_domain::value::{DomainError, InputField};
use wf_domain::workflow::admission::AdmissionError;
use wf_domain::workflow::batch::BatchRejection;

#[test]
fn accepted_reply_preserves_step_postimage_and_golden_bytes() -> TestResult {
    let bytes = f::reply()?;
    assert_eq!(bytes, f::hex(f::GOLDEN)?);
    assert_eq!(bytes.len(), 160);
    assert_eq!(
        Sha256::digest(&bytes).as_slice(),
        f::hex("8fa7f0c9ff0f3d3dafb7b5a7b05f79a3250d5643d77dbca040c145615a505d38")?
    );
    assert_eq!(f::summary(&bytes)?, (RetainedStatus::Accepted, Some(1)));
    f::with_step(b"charge", Some(b"\0\xff"), |decision| {
        assert!(step_continue_response_matches(
            decision,
            &bytes,
            f::position(),
            f::time(),
            f::limits()?
        )?);
        Ok(())
    })
}

#[test]
fn reply_rejections_preserve_extended_reason_codes() -> TestResult {
    f::reply()?;
    for reason in [
        IndexRejection::DirectRunningFifoEntry,
        IndexRejection::InvalidPriority,
        IndexRejection::FifoPriority,
        IndexRejection::NotFifoHead,
        IndexRejection::Batch(BatchRejection::DuplicateId),
        IndexRejection::Batch(BatchRejection::CrossGroup),
        IndexRejection::Batch(BatchRejection::Domain(DomainError::StaleLease)),
        IndexRejection::Batch(BatchRejection::Domain(DomainError::EmptyValue(
            InputField::To,
        ))),
        IndexRejection::Batch(BatchRejection::Admission(AdmissionError::ValueTooLarge {
            bytes: 33,
            maximum: 32,
        })),
    ] {
        let decision = IndexedDecision::Rejected(reason);
        let (bytes, status) =
            encode_step_continue_response(&decision, f::position(), f::time(), f::limits()?)?;
        assert_eq!(status, RetainedStatus::IndexedRejected(reason));
        assert_eq!(f::summary(&bytes)?, (status, None));
        assert!(step_continue_response_matches(
            &decision,
            &bytes,
            f::position(),
            f::time(),
            f::limits()?
        )?);
        if reason == IndexRejection::DirectRunningFifoEntry {
            assert_eq!(
                bytes,
                f::hex(concat!(
                    "465753540001000000000000000100000000000000070000000000000009",
                    "0000000000000002010700"
                ))?
            );
        }
    }
    Ok(())
}

#[test]
fn step_reply_preserves_claim_counter_without_incrementing() -> TestResult {
    f::with_step(b"charge", Some(b"\0\xff"), |decision| {
        let IndexedDecision::Accepted(plan) = decision else {
            return Err("actual Step decision".into());
        };
        let record = &plan.batch().items()[0]
            .plan()
            .write()
            .ok_or("actual Step write")?
            .record;
        assert_eq!(record.attempts(), 0);
        let (bytes, status) =
            encode_step_continue_response(decision, f::position(), f::time(), f::limits()?)?;
        assert_eq!(&bytes[126..134], &0_u64.to_be_bytes());
        assert_eq!(f::summary(&bytes)?, (status, Some(1)));
        Ok(())
    })
}
