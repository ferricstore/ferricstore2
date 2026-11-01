#![forbid(unsafe_code)]
//! Original create-attribute scenarios; capture values confer no runtime authority.
use wf_domain::metadata::OriginalMetadataInput as I;
use wf_domain::workflow::caller::create::{
    OriginalCreateContext, OriginalCreateLimits, OriginalCreateOutcome as Outcome,
};
use wf_ports::projection::query_row::{PreparedOriginalCreate, prepare_original_create};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
mod resources;
mod source;
mod start;
mod start_validation;
mod validation;
fn limits() -> OriginalCreateLimits {
    OriginalCreateLimits {
        maximum_input_bytes: 4_000_000,
        maximum_output_bytes: 4_000_000,
        maximum_work_units: 1_000_000_000,
        maximum_temporary_bytes: 16_000_000,
    }
}
fn accepted<'r, 'a>(
    id: &'a [u8],
    options: I<'r, 'a>,
) -> TestResult<PreparedOriginalCreate<'r, 'a>> {
    match prepare_original_create(
        I::Binary(id),
        options,
        OriginalCreateContext::default(),
        limits(),
    )? {
        Outcome::Accepted { value, .. } => Ok(value),
        other => Err(format!("expected accepted create: {other:?}").into()),
    }
}
fn rejected(id: &[u8], options: I<'_, '_>, message: &str) -> TestResult {
    assert_eq!(rejection_message(id, options)?, message);
    Ok(())
}
fn rejection_message(id: &[u8], options: I<'_, '_>) -> TestResult<std::borrow::Cow<'static, str>> {
    match prepare_original_create(
        I::Binary(id),
        options,
        OriginalCreateContext::default(),
        limits(),
    )? {
        Outcome::Rejected {
            message: actual,
            work_units,
            ..
        } => {
            assert!(work_units > 0);
            Ok(actual)
        }
        other => Err(format!("expected source rejection: {other:?}").into()),
    }
}
