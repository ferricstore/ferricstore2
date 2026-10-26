//! Actual intent8 association with the four existing headerless body grammars.

use wf_domain::workflow::admission::{AdmissionLimits, MAX_BATCH_ITEMS, MAX_INLINE_BYTES};
use wf_domain::workflow::control::ControlPlan;
use wf_domain::workflow::indexed::{IndependentIndexedDecision, IndexedDecision};
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedOutcome, RetainedStatus};
use wf_types::version::{LogIdentity, TimestampMillis};

use crate::consensus::PreparedOperation;
use crate::log::{CodecError, DecodedRequest, RecordBatchApplyDecision};
use crate::state::StateTransactionError;

use crate::log::public_mutation::response as public;
use crate::log::record_batch::{atomic_start::response as atomic, response as batch};

pub(in crate::log) fn effective_profile(decoded: &DecodedRequest<'_>) -> Result<u16, CodecError> {
    if decoded.request.intent.version() != 8 {
        return Ok(decoded.request.intent.version());
    }
    let mode = u16::from(
        decoded
            .workflow_value_metadata()
            .ok_or(CodecError::Malformed)?
            .profile,
    );
    let items = decoded
        .request
        .operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?;
    if !matches!(mode, 1 | 5 | 6 | 7)
        || items.is_empty()
        || (matches!(mode, 1 | 7) && items.len() != 1)
        || (mode == 1)
            != matches!(
                decoded.request.operation,
                PreparedOperation::PublicMutation { .. }
            )
    {
        return Err(CodecError::UnsupportedVersion);
    }
    Ok(mode)
}

pub(in crate::log) fn response_version(
    decoded: &DecodedRequest<'_>,
    legacy_version: u16,
) -> Result<u16, CodecError> {
    if decoded.request.intent.version() != 8 {
        return Ok(legacy_version);
    }
    match effective_profile(decoded)? {
        1 => Ok(8),
        mode @ 5..=7 => Ok(mode),
        _ => Err(CodecError::UnsupportedVersion),
    }
}

/// Validates actual value-profile identity and the complete original response.
/// The response version selects one closed contained grammar, without relabelling.
/// # Errors
/// Rejects unsupported associations, malformed bytes and immutable profile limits.
pub fn workflow_value_retained_summary(
    intent: ClientIntent,
    response_version: u16,
    response: &[u8],
    position: LogIdentity,
    time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    if intent.version() != 8 {
        return Err(CodecError::UnsupportedVersion);
    }
    if response.len() > admission.decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let summary = match response_version {
        5 | 6 => batch::body_summary(response_version, response)?,
        7 => atomic::body_summary(response, position, time, admission)?,
        8 => (public::body_status(response)?, None),
        _ => return Err(CodecError::UnsupportedVersion),
    };
    if summary
        .1
        .is_some_and(|count| count > admission.batch_items())
    {
        return Err(CodecError::ItemLimit);
    }
    Ok(summary)
}

pub(in crate::log) fn check_replay(
    decoded: &DecodedRequest<'_>,
    original: &RetainedOutcome,
) -> Result<(), crate::log::RecordBatchApplyError> {
    let mode = effective_profile(decoded)?;
    if decoded.request.intent.version() != 8
        || original.intent() != decoded.request.intent
        || original.response_version() != response_version(decoded, 0)?
    {
        return Err(CodecError::UnsupportedVersion.into());
    }
    let (status, count) = workflow_value_retained_summary(
        original.intent(),
        original.response_version(),
        original.response(),
        original.original_position(),
        original.original_time(),
        decoded.limits.admission,
    )?;
    let expected = match (mode, status) {
        (6, RetainedStatus::Accepted) => Some(
            decoded
                .request
                .operation
                .mutation_items()
                .ok_or(CodecError::UnsupportedTag)?
                .len(),
        ),
        (7, RetainedStatus::Accepted) => Some(1),
        _ => None,
    };
    if status != original.status() || count != expected {
        return Err(wf_domain::workflow::batch::ApplyError::InvalidAuthority.into());
    }
    Ok(())
}

pub(crate) fn validate_changes(
    scope: &RequestScope,
    control: ControlPlan,
    decision: &RecordBatchApplyDecision<'_>,
) -> Result<(), StateTransactionError> {
    let (RecordBatchApplyDecision::Fresh { outcome, .. }
    | RecordBatchApplyDecision::IndependentFresh { outcome, .. }) = decision
    else {
        return Err(StateTransactionError::InvalidChanges);
    };
    let position = control
        .next()
        .last_entry()
        .ok_or(StateTransactionError::InvalidChanges)?
        .log();
    let time = control.execution_time();
    if outcome.intent().version() != 8
        || outcome.scope() != scope
        || outcome.original_position() != position
        || outcome.original_time() != time
    {
        return Err(StateTransactionError::InvalidChanges);
    }
    let admission =
        AdmissionLimits::new(MAX_INLINE_BYTES, outcome.response().len(), MAX_BATCH_ITEMS)
            .map_err(|_| StateTransactionError::InvalidChanges)?;
    let (status, _) = workflow_value_retained_summary(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        position,
        time,
        admission,
    )
    .map_err(|_| StateTransactionError::InvalidChanges)?;
    let matches = match decision {
        RecordBatchApplyDecision::Fresh { operation, .. } => {
            indexed_matches(operation, outcome, position, time, admission)
        }
        RecordBatchApplyDecision::IndependentFresh { operation, .. } => {
            independent_matches(operation, outcome)
        }
        _ => return Err(StateTransactionError::InvalidChanges),
    }
    .map_err(|_| StateTransactionError::InvalidChanges)?;
    if outcome.status() != status || !matches {
        return Err(StateTransactionError::InvalidChanges);
    }
    Ok(())
}

fn indexed_matches(
    operation: &IndexedDecision<'_>,
    outcome: &RetainedOutcome,
    position: LogIdentity,
    time: TimestampMillis,
    admission: AdmissionLimits,
) -> Result<bool, CodecError> {
    match outcome.response_version() {
        5 => {
            let (status, count) = batch::body_summary(5, outcome.response())?;
            let expected = match operation {
                IndexedDecision::Accepted(_) => RetainedStatus::Accepted,
                IndexedDecision::Rejected(reason) => RetainedStatus::IndexedRejected(*reason),
            };
            Ok(status == expected && count.is_none())
        }
        7 => atomic::atomic_start_response_matches(
            operation,
            outcome.response(),
            position,
            time,
            admission,
        ),
        _ => Err(CodecError::UnsupportedVersion),
    }
}

fn independent_matches(
    operation: &IndependentIndexedDecision<'_>,
    outcome: &RetainedOutcome,
) -> Result<bool, CodecError> {
    match outcome.response_version() {
        6 => batch::independent_body_matches(operation, outcome.response()),
        8 => public::public_mutation_response_matches(operation, outcome.response()),
        _ => Err(CodecError::UnsupportedVersion),
    }
}
