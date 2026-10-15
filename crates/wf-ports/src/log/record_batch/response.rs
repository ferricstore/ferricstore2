//! Private scalar acknowledgement and exhaustive deterministic rejection grammar.

use wf_domain::value::{Counter, DomainError, InputField, RetryPolicyField};
use wf_domain::workflow::admission::AdmissionError;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::{IndexRejection, IndexedDecision};
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};

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

/// Derives the exact private batch5 response status without allocating.
///
/// This validates a closed byte grammar, not outcome origin or persistence.
/// # Errors
/// Rejects unsupported profiles/tags, incomplete/trailing bytes and integer overflow.
pub fn record_batch_retained_status(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
) -> Result<RetainedStatus, CodecError> {
    if intent.version() != 5 || response_version != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    let Some(&root) = response.first() else {
        return Err(CodecError::Malformed);
    };
    if root == 0 {
        return if response.len() == 1 {
            Ok(RetainedStatus::Accepted)
        } else {
            Err(CodecError::Malformed)
        };
    }
    if root != 1 {
        return Err(CodecError::UnsupportedTag);
    }
    let (&kind, &code) = response
        .get(1)
        .zip(response.get(2))
        .ok_or(CodecError::Malformed)?;
    let mut size = 3;
    let rejection = match kind {
        0 => {
            let detail = if matches!(code, 8 | 16 | 19) {
                size += 1;
                Some(*response.get(3).ok_or(CodecError::Malformed)?)
            } else {
                None
            };
            IndexRejection::Batch(BatchRejection::Domain(decoded_domain(code, detail)?))
        }
        1 => {
            let error = match code {
                0 => AdmissionError::InvalidLimits,
                1 => AdmissionError::BatchItems,
                2 => AdmissionError::KeyTooLarge,
                3 => {
                    size = 19;
                    AdmissionError::ValueTooLarge {
                        bytes: decoded_size(response.get(3..11).ok_or(CodecError::Malformed)?)?,
                        maximum: decoded_size(response.get(11..19).ok_or(CodecError::Malformed)?)?,
                    }
                }
                4 => AdmissionError::DecisionBytes,
                5 => AdmissionError::LengthOverflow,
                6 => AdmissionError::EncodedValueCount,
                _ => return Err(CodecError::UnsupportedTag),
            };
            IndexRejection::Batch(BatchRejection::Admission(error))
        }
        2..=6 if code == 0 => match kind {
            2 => IndexRejection::Batch(BatchRejection::DuplicateId),
            3 => IndexRejection::Batch(BatchRejection::CrossGroup),
            4 => IndexRejection::InvalidPriority,
            5 => IndexRejection::FifoPriority,
            _ => IndexRejection::NotFifoHead,
        },
        _ => return Err(CodecError::UnsupportedTag),
    };
    if response.len() != size {
        return Err(CodecError::Malformed);
    }
    Ok(RetainedStatus::IndexedRejected(rejection))
}

fn decoded_size(bytes: &[u8]) -> Result<usize, CodecError> {
    usize::try_from(u64::from_be_bytes(
        bytes.try_into().map_err(|_| CodecError::Malformed)?,
    ))
    .map_err(|_| CodecError::LengthOverflow)
}

fn decoded_domain(code: u8, detail: Option<u8>) -> Result<DomainError, CodecError> {
    Ok(match code {
        0 => DomainError::NotFound,
        1 => DomainError::AlreadyExists,
        2 => DomainError::WrongState,
        3 => DomainError::WrongVersion,
        4 => DomainError::StaleLease,
        5 => DomainError::Terminal,
        6 => DomainError::RunningTransition,
        7 => DomainError::TerminalTransition,
        8 => DomainError::EmptyValue(match detail {
            Some(0) => InputField::Type,
            Some(1) => InputField::State,
            Some(2) => InputField::From,
            Some(3) => InputField::To,
            Some(4) => InputField::Worker,
            Some(5) => InputField::LeaseToken,
            Some(6) => InputField::ExpectState,
            _ => return Err(CodecError::UnsupportedTag),
        }),
        9 => DomainError::NowTooLarge,
        10 => DomainError::RunAtTooLarge,
        11 => DomainError::ExpectedVersionTooLarge,
        12 => DomainError::FenceTooLarge,
        13 => DomainError::ZeroLease,
        14 => DomainError::LeaseTooLarge,
        15 => DomainError::LeaseDeadlineTooLarge,
        16 => DomainError::CounterOverflow(match detail {
            Some(0) => Counter::Version,
            Some(1) => Counter::Fence,
            Some(2) => Counter::Attempts,
            _ => return Err(CodecError::UnsupportedTag),
        }),
        17 => DomainError::ReservedId,
        18 => DomainError::ReservedType,
        19 => DomainError::InvalidRetryPolicy(match detail {
            Some(0) => RetryPolicyField::MaxRetries,
            Some(1) => RetryPolicyField::BaseMillis,
            Some(2) => RetryPolicyField::MaxMillis,
            Some(3) => RetryPolicyField::JitterPercent,
            Some(4) => RetryPolicyField::ExhaustedTo,
            Some(5) => RetryPolicyField::RunningExhaustion,
            Some(6) => RetryPolicyField::State,
            Some(7) => RetryPolicyField::RunningState,
            _ => return Err(CodecError::UnsupportedTag),
        }),
        20 => DomainError::PolicyTypeMismatch,
        21 => DomainError::RetryDeadlineOverflow,
        22 => DomainError::UnsupportedOperation,
        _ => return Err(CodecError::UnsupportedTag),
    })
}
