//! Ordered effects and full retained-response validation with one live reply.

use wf_domain::workflow::caller::{MutationCallerError, NormalizedMutation};
use wf_domain::workflow::request::{ClientIntent, RetainedOutcome, RetainedStatus};
use wf_ports::consensus::{
    MutationNotSubmitted, MutationSubmission, MutationSubmitPort, MutationUnknown,
};
use wf_ports::log::{
    CodecError, RecordBatchApplyError, encode_public_mutation, public_mutation_retained_status,
};

use super::{MutationCallLimits, MutationItemResult, MutationItemStatus};

pub(super) fn run<P: MutationSubmitPort>(
    callers: &[Result<NormalizedMutation<'_>, MutationCallerError>],
    results: &mut [MutationItemResult],
    port: &mut P,
    limits: MutationCallLimits,
    global: usize,
) {
    let mut unknown = false;
    for (caller, row) in callers.iter().zip(results) {
        if unknown {
            row.status = MutationItemStatus::NotSubmitted(MutationNotSubmitted::PredecessorUnknown);
            continue;
        }
        row.status = match caller {
            Err(error) => MutationItemStatus::LocalError(*error),
            Ok(caller) => submit(caller, row, port, limits, global),
        };
        unknown = matches!(row.status, MutationItemStatus::Unknown(_));
    }
}

fn submit<P: MutationSubmitPort>(
    caller: &NormalizedMutation<'_>,
    row: &mut MutationItemResult,
    port: &mut P,
    limits: MutationCallLimits,
    global: usize,
) -> MutationItemStatus {
    let stamp = match port.prepare(&row.scope, &row.request, caller) {
        Ok(stamp) => stamp,
        Err(error) => return MutationItemStatus::NotSubmitted(error),
    };
    // Whole-call admission has already proved this subtraction and every E_i.
    let workspace = limits.workspace_bytes - global;
    let (prepared, intent) = match encode_public_mutation(
        &row.scope,
        &row.request,
        caller,
        stamp,
        limits.codec,
        workspace,
    ) {
        Ok(values) => values,
        Err(error) => return MutationItemStatus::NotSubmitted(encoding_error(error)),
    };
    row.intent = Some(intent);
    match port.submit(
        &row.scope,
        &row.request,
        prepared,
        limits.execution_workspace_bytes,
    ) {
        MutationSubmission::Retained(outcome) => project(row, intent, &outcome),
        MutationSubmission::IntentConflict => MutationItemStatus::IntentConflict,
        MutationSubmission::Expired => MutationItemStatus::Expired,
        MutationSubmission::NotSubmitted(reason) => MutationItemStatus::NotSubmitted(reason),
        MutationSubmission::Unknown(reason) => MutationItemStatus::Unknown(reason),
    }
}

fn project(
    row: &MutationItemResult,
    intent: ClientIntent,
    outcome: &RetainedOutcome,
) -> MutationItemStatus {
    if outcome.scope() != &row.scope
        || outcome.request() != &row.request
        || outcome.intent() != intent
    {
        return MutationItemStatus::Unknown(MutationUnknown::InvalidReply);
    }
    let Ok(status) = public_mutation_retained_status(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
    ) else {
        return MutationItemStatus::Unknown(MutationUnknown::InvalidReply);
    };
    if status != outcome.status() {
        return MutationItemStatus::Unknown(MutationUnknown::InvalidReply);
    }
    let position = outcome.original_position();
    let retained_until = outcome.expires_at();
    match status {
        RetainedStatus::Accepted => MutationItemStatus::Success {
            position,
            retained_until,
        },
        RetainedStatus::IndexedRejected(reason) => MutationItemStatus::Rejected {
            reason,
            position,
            retained_until,
        },
        _ => MutationItemStatus::Unknown(MutationUnknown::InvalidReply),
    }
}

fn encoding_error(error: RecordBatchApplyError) -> MutationNotSubmitted {
    match error {
        RecordBatchApplyError::Workspace { .. }
        | RecordBatchApplyError::Encoding(
            CodecError::ByteLimit
            | CodecError::ItemLimit
            | CodecError::ValueLimit
            | CodecError::KeyLimit
            | CodecError::ScopeLimit
            | CodecError::LengthOverflow,
        ) => MutationNotSubmitted::Limit,
        _ => MutationNotSubmitted::Preparation,
    }
}
