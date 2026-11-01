//! Original create caller normalization from explicitly captured source data.

mod budget;
mod model;
mod normalize;
mod number;
mod options;
mod schedule;
pub use model::{
    OriginalCreateCaller, OriginalCreateContext, OriginalCreateError, OriginalCreateLimits,
    OriginalCreateOutcome, OriginalCreateScheduleMetadata,
};

use crate::metadata::OriginalMetadataInput as I;

/// Normalizes original keyword options without clocks, routing or authorization.
/// # Errors
/// Operational refusals remain distinct from measured source errors and raises.
pub fn normalize_original_create<'r, 'a>(
    id: I<'r, 'a>,
    options: I<'r, 'a>,
    context: OriginalCreateContext<'r, 'a>,
    limits: OriginalCreateLimits,
) -> Result<OriginalCreateOutcome<OriginalCreateCaller<'r, 'a>>, OriginalCreateError> {
    let mut budget = budget::Budget::new(limits)?;
    budget.input(&[
        id,
        options,
        context.history_hot_maximum,
        context.history_maximum,
    ])?;
    let result = normalize::run(id, options, context, &mut budget);
    let work_units = budget.work;
    let temporary_bytes = budget.peak;
    match result {
        Ok(value) => Ok(OriginalCreateOutcome::Accepted {
            value,
            work_units,
            temporary_bytes,
        }),
        Err(budget::Failure::Source(message)) => {
            if matches!(message, std::borrow::Cow::Owned(_)) {
                budget.output(message.len())?;
            }
            Ok(OriginalCreateOutcome::Rejected {
                message,
                work_units,
                temporary_bytes,
            })
        }
        Err(budget::Failure::Raised) => Ok(OriginalCreateOutcome::RaisedArgumentError {
            work_units,
            temporary_bytes,
        }),
        Err(budget::Failure::Operational(error)) => Err(error),
    }
}
