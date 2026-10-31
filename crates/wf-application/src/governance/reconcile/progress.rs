//! Sparse marks precede prefix ACK; only complete known original replies advance hints.
use super::{ReleaseReconcileLimits as L, model::ReleaseStatus as S};
use wf_domain::{
    governance::release::{ReleaseMaintenanceReply as R, ReleaseMaintenanceRequest as C},
    workflow::request::{RequestScope, RetainedStatus},
};
use wf_ports::{
    consensus::{MutationSubmission, WorkflowReleaseSubmitPort},
    log::{
        decode_release_maintenance_reply, encode_release_maintenance_request,
        release_maintenance_request_intent,
    },
    state::WorkflowReleasePage,
};

pub(super) fn persist<P: WorkflowReleaseSubmitPort>(
    port: &mut P,
    scope: &RequestScope,
    page: &WorkflowReleasePage,
    decisions: &[S; 256],
    now: u64,
    limits: L,
) -> bool {
    let prefix = if page
        .entries
        .first()
        .is_some_and(|e| e.sequence == page.meta.head)
    {
        page.entries
            .iter()
            .zip(decisions)
            .take_while(|(_, s)| matches!(s, S::Released | S::Completed))
            .last()
            .map(|(entry, _)| entry.sequence)
    } else {
        None
    };
    let mut sparse = Vec::new();
    if sparse.try_reserve_exact(page.entries.len()).is_err() {
        return false;
    }
    for (entry, decisions) in page.entries.iter().zip(decisions) {
        if *decisions == S::Released && prefix.is_none_or(|p| entry.sequence > p) {
            sparse.push(entry.sequence);
        }
    }
    if !sparse.is_empty()
        && !command(
            port,
            scope,
            &C::MarkCompleted { sequences: sparse },
            now,
            limits,
        )
    {
        return false;
    }
    prefix.is_none_or(|up_to| {
        command(
            port,
            scope,
            &C::Acknowledge {
                expected_head: page.meta.head,
                up_to,
            },
            now,
            limits,
        )
    })
}
fn command<P: WorkflowReleaseSubmitPort>(
    port: &mut P,
    scope: &RequestScope,
    command: &C,
    now: u64,
    limits: L,
) -> bool {
    let Ok(intent) = release_maintenance_request_intent(command) else {
        return false;
    };
    let Ok((id, stamp)) = port.prepare_workflow_release_command(scope, now) else {
        return false;
    };
    if stamp.wall().get() < now {
        return false;
    }
    let codec = limits.credit.publication.codec;
    let Ok(frame) = encode_release_maintenance_request(
        scope,
        &id,
        stamp,
        command,
        codec,
        limits.maximum_maintenance_bytes,
    ) else {
        return false;
    };
    let MutationSubmission::Retained(outcome) =
        port.submit_workflow_release_command(scope, &id, frame, limits.execution_workspace_bytes)
    else {
        return false;
    };
    if outcome.scope() != scope
        || outcome.request() != &id
        || outcome.intent() != intent
        || outcome.response_version() != 15
        || outcome.status() != RetainedStatus::Accepted
        || outcome.original_time().get() < now
    {
        return false;
    }
    let Ok(R::Accepted { meta, affected }) = decode_release_maintenance_reply(
        outcome.response(),
        codec.admission().decision_bytes(),
        1024,
    ) else {
        return false;
    };
    match command {
        C::Acknowledge {
            expected_head,
            up_to,
        } => {
            meta.head > *up_to
                && (affected == 0 || u64::from(affected) == up_to + 1 - expected_head)
        }
        C::MarkCompleted { sequences } => {
            sequences.iter().all(|s| *s <= meta.tail)
                && usize::from(affected) == sequences.iter().filter(|s| **s >= meta.head).count()
        }
    }
}
