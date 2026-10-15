//! Private scalar acknowledgement and exhaustive deterministic rejection grammar.

use wf_domain::value::{Counter, DomainError, InputField, RetryPolicyField};
use wf_domain::workflow::admission::AdmissionError;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::{IndexRejection, IndexedDecision};
use wf_domain::workflow::request::RetainedStatus;

use super::super::CodecError;

pub(super) fn encode(
    decision: &IndexedDecision<'_>,
) -> Result<(Vec<u8>, RetainedStatus), CodecError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(19)
        .map_err(|_| CodecError::Allocation)?;
    let IndexedDecision::Rejected(reason) = decision else {
        bytes.push(0);
        return Ok((bytes, RetainedStatus::Accepted));
    };
    bytes.push(1);
    match *reason {
        IndexRejection::InvalidPriority => bytes.extend_from_slice(&[4, 0]),
        IndexRejection::FifoPriority => bytes.extend_from_slice(&[5, 0]),
        IndexRejection::NotFifoHead => bytes.extend_from_slice(&[6, 0]),
        IndexRejection::Batch(reason) => match reason {
            BatchRejection::DuplicateId => bytes.extend_from_slice(&[2, 0]),
            BatchRejection::CrossGroup => bytes.extend_from_slice(&[3, 0]),
            BatchRejection::Domain(reason) => {
                let (code, detail) = domain(reason);
                bytes.extend_from_slice(&[0, code]);
                if let Some(detail) = detail {
                    bytes.push(detail);
                }
            }
            BatchRejection::Admission(reason) => {
                bytes.extend_from_slice(&[
                    1,
                    match reason {
                        AdmissionError::InvalidLimits => 0,
                        AdmissionError::BatchItems => 1,
                        AdmissionError::KeyTooLarge => 2,
                        AdmissionError::ValueTooLarge { .. } => 3,
                        AdmissionError::DecisionBytes => 4,
                        AdmissionError::LengthOverflow => 5,
                        AdmissionError::EncodedValueCount => 6,
                    },
                ]);
                if let AdmissionError::ValueTooLarge {
                    bytes: actual,
                    maximum,
                } = reason
                {
                    bytes.extend_from_slice(
                        &u64::try_from(actual)
                            .map_err(|_| CodecError::LengthOverflow)?
                            .to_be_bytes(),
                    );
                    bytes.extend_from_slice(
                        &u64::try_from(maximum)
                            .map_err(|_| CodecError::LengthOverflow)?
                            .to_be_bytes(),
                    );
                }
            }
        },
    }
    Ok((bytes, RetainedStatus::IndexedRejected(*reason)))
}

fn domain(reason: DomainError) -> (u8, Option<u8>) {
    let code = match reason {
        DomainError::NotFound => 0,
        DomainError::AlreadyExists => 1,
        DomainError::WrongState => 2,
        DomainError::WrongVersion => 3,
        DomainError::StaleLease => 4,
        DomainError::Terminal => 5,
        DomainError::RunningTransition => 6,
        DomainError::TerminalTransition => 7,
        DomainError::EmptyValue(_) => 8,
        DomainError::NowTooLarge => 9,
        DomainError::RunAtTooLarge => 10,
        DomainError::ExpectedVersionTooLarge => 11,
        DomainError::FenceTooLarge => 12,
        DomainError::ZeroLease => 13,
        DomainError::LeaseTooLarge => 14,
        DomainError::LeaseDeadlineTooLarge => 15,
        DomainError::CounterOverflow(_) => 16,
        DomainError::ReservedId => 17,
        DomainError::ReservedType => 18,
        DomainError::InvalidRetryPolicy(_) => 19,
        DomainError::PolicyTypeMismatch => 20,
        DomainError::RetryDeadlineOverflow => 21,
        DomainError::UnsupportedOperation => 22,
    };
    let detail = match reason {
        DomainError::EmptyValue(field) => Some(match field {
            InputField::Type => 0,
            InputField::State => 1,
            InputField::From => 2,
            InputField::To => 3,
            InputField::Worker => 4,
            InputField::LeaseToken => 5,
            InputField::ExpectState => 6,
        }),
        DomainError::CounterOverflow(field) => Some(match field {
            Counter::Version => 0,
            Counter::Fence => 1,
            Counter::Attempts => 2,
        }),
        DomainError::InvalidRetryPolicy(field) => Some(match field {
            RetryPolicyField::MaxRetries => 0,
            RetryPolicyField::BaseMillis => 1,
            RetryPolicyField::MaxMillis => 2,
            RetryPolicyField::JitterPercent => 3,
            RetryPolicyField::ExhaustedTo => 4,
            RetryPolicyField::RunningExhaustion => 5,
            RetryPolicyField::State => 6,
            RetryPolicyField::RunningState => 7,
        }),
        _ => None,
    };
    (code, detail)
}
