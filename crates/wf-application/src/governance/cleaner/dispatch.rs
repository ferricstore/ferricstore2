//! Original scoped bytes and complete retained replies; no optimistic write credit.
use super::{CreditCleanupLimits as L, CreditCleanupRequest as R, CreditCleanupSource};
use wf_domain::{
    governance::{
        catalog::{CatalogMembershipAction, CatalogMembershipChange, CatalogProgressChange},
        identified::{IdentifiedCreditMutation, IdentifiedCreditRequest},
    },
    workflow::request::{ClientIntent, RequestScope, RetainedOutcome, RetainedStatus},
};
use wf_ports::{
    consensus::{CreditCatalogPublicationPort, MutationSubmission},
    log::{
        catalog_membership_request_intent, catalog_membership_retained_summary,
        catalog_progress_request_intent, catalog_progress_retained_summary, credit_cleanup_intent,
        credit_cleanup_retained_summary, encode_catalog_membership_request,
        encode_catalog_progress_request, encode_credit_request,
    },
};
use wf_types::{identity::RequestId, version::CommittedTime};

pub(super) fn cleanup<P: CreditCatalogPublicationPort>(
    port: &mut P,
    request: R<'_>,
    source: CreditCleanupSource<'_>,
    name: &[u8],
    limits: L,
) -> Option<(u16, bool)> {
    let intent = credit_cleanup_intent(name, source.shard_count).ok()?;
    let (id, stamp) = prepare(port, source.scope, request.now_ms)?;
    let command = IdentifiedCreditRequest {
        scope: name,
        shard_count: source.shard_count,
        configuration: None,
        mutation: IdentifiedCreditMutation::Cleanup,
    };
    let frame = encode_credit_request(
        source.scope,
        &id,
        stamp,
        command,
        limits.codec,
        owner_limit(limits)?,
    )
    .ok()?;
    let MutationSubmission::Retained(outcome) =
        port.submit_catalog_command(source.scope, &id, frame, limits.execution_workspace_bytes)
    else {
        return None;
    };
    if !bound(&outcome, source.scope, &id, intent, stamp) {
        return None;
    }
    credit_cleanup_retained_summary(
        intent,
        outcome.response_version(),
        outcome.response(),
        name,
        limits.read.credit,
        limits.codec.admission(),
    )
    .ok()
}
pub(super) fn prune<P: CreditCatalogPublicationPort>(
    port: &mut P,
    request: R<'_>,
    member: &[u8],
    limits: L,
) -> bool {
    let command = CatalogMembershipChange {
        catalog: request.catalog,
        action: CatalogMembershipAction::Unregister,
        members: &[member],
    };
    let Ok(intent) = catalog_membership_request_intent(command) else {
        return false;
    };
    let Some((id, stamp)) = prepare(port, request.target, request.now_ms) else {
        return false;
    };
    let Some(owner) = owner_limit(limits) else {
        return false;
    };
    let Ok(frame) =
        encode_catalog_membership_request(request.target, &id, stamp, command, limits.codec, owner)
    else {
        return false;
    };
    let MutationSubmission::Retained(outcome) =
        port.submit_catalog_command(request.target, &id, frame, limits.execution_workspace_bytes)
    else {
        return false;
    };
    bound(&outcome, request.target, &id, intent, stamp)
        && catalog_membership_retained_summary(
            intent,
            outcome.response_version(),
            outcome.response(),
            limits.codec.admission(),
        ) == Ok((RetainedStatus::Accepted, None))
}
pub(super) fn progress<P: CreditCatalogPublicationPort>(
    port: &mut P,
    request: R<'_>,
    cursor: Option<&[u8]>,
    limits: L,
) -> bool {
    let command = CatalogProgressChange {
        catalog: request.catalog,
        cursor,
    };
    let Ok(intent) = catalog_progress_request_intent(command) else {
        return false;
    };
    let Some((id, stamp)) = prepare(port, request.target, request.now_ms) else {
        return false;
    };
    let Some(owner) = owner_limit(limits) else {
        return false;
    };
    let Ok(frame) =
        encode_catalog_progress_request(request.target, &id, stamp, command, limits.codec, owner)
    else {
        return false;
    };
    let MutationSubmission::Retained(outcome) =
        port.submit_catalog_command(request.target, &id, frame, limits.execution_workspace_bytes)
    else {
        return false;
    };
    bound(&outcome, request.target, &id, intent, stamp)
        && catalog_progress_retained_summary(
            intent,
            outcome.response_version(),
            outcome.response(),
            limits.codec.admission(),
        ) == Ok((RetainedStatus::Accepted, None))
}
fn prepare<P: CreditCatalogPublicationPort>(
    port: &mut P,
    scope: &RequestScope,
    floor: u64,
) -> Option<(RequestId, CommittedTime)> {
    let prepared = port.prepare_catalog_command_at(scope, floor).ok()?;
    (prepared.1.wall().get() >= floor).then_some(prepared)
}
fn bound(
    outcome: &RetainedOutcome,
    scope: &RequestScope,
    id: &RequestId,
    intent: ClientIntent,
    stamp: CommittedTime,
) -> bool {
    outcome.scope() == scope
        && outcome.request() == id
        && outcome.intent() == intent
        && outcome.status() == RetainedStatus::Accepted
        && outcome.original_time() >= stamp.wall()
}
fn owner_limit(limits: L) -> Option<usize> {
    limits.codec.admission().decision_bytes().checked_add(2048)
}
