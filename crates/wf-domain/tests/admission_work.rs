//! Actual encoded value counts/bytes and stable pre-copy output/work charges.
#![forbid(unsafe_code)]

use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits, admit_command};
use wf_domain::workflow::{CommittedCommand, Create, Operation};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn empty_encoded_values_have_finite_schema_work_and_present_bytes_have_exact_limits() -> TestResult
{
    let command = CommittedCommand {
        id: RecordId::new(b"i".to_vec())?,
        now: TimestampMillis::new(0),
        operation: Operation::Create(Create {
            workflow_type: b"t".to_vec(),
            state: b"q".to_vec(),
            payload: Some(vec![]),
            run_at: None,
        }),
    };
    let limits = AdmissionLimits::new(4, 4_096, 2)?;
    let empty: &[u8] = &[];
    assert_eq!(
        admit_command(&command, b"k", &[empty; 3], limits),
        Err(AdmissionError::EncodedValueCount)
    );
    assert_eq!(admit_command(&command, b"k", &[b"1234"], limits)?, 8);
    assert_eq!(
        admit_command(&command, b"k", &[b"12345"], limits),
        Err(AdmissionError::ValueTooLarge {
            bytes: 5,
            maximum: 4
        })
    );
    assert_eq!(admit_command(&command, b"k", &[empty; 2], limits)?, 4);
    assert!(AdmissionLimits::new(0, 1, 1).is_err());
    assert!(AdmissionLimits::new(1, 0, 1).is_err());
    assert!(AdmissionLimits::new(1, 1, 0).is_err());
    Ok(())
}

#[test]
fn stable_plan_charge_has_inclusive_boundaries_and_checked_arithmetic() -> TestResult {
    let limits = AdmissionLimits::new(4, 1_032, 1)?;
    assert_eq!(limits.charge(2, 1)?, 1_032);
    assert_eq!(limits.charge(3, 1), Err(AdmissionError::DecisionBytes));
    let largest = AdmissionLimits::new(4, usize::MAX, 1)?;
    assert_eq!(
        largest.charge(usize::MAX, 0),
        Err(AdmissionError::LengthOverflow)
    );
    assert_eq!(
        largest.charge(0, usize::MAX),
        Err(AdmissionError::LengthOverflow)
    );
    let defaults = AdmissionLimits::default();
    assert_eq!(defaults.inline_bytes(), 1_048_576);
    assert_eq!(defaults.decision_bytes(), 16 * 1_024 * 1_024);
    assert_eq!(defaults.batch_items(), 1_000);
    assert!(AdmissionLimits::new(512 * 1_024 * 1_024, 1, 100_000).is_ok());
    assert!(AdmissionLimits::new(512 * 1_024 * 1_024 + 1, 1, 1).is_err());
    assert!(AdmissionLimits::new(1, 1, 100_001).is_err());
    Ok(())
}
