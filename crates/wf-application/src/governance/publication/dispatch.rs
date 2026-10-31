//! Full original response checks before any dependent ACK preparation.

use super::CreditCatalogPublicationLimits as Limits;
use wf_domain::{
    governance::catalog::{
        CatalogMembershipAction, CatalogMembershipChange, CreditCatalogAckReply,
        CreditCatalogAckRequest,
    },
    workflow::request::{ClientIntent, RequestScope, RetainedOutcome, RetainedStatus},
};
use wf_ports::{
    consensus::{CreditCatalogPublicationPort, MutationSubmission},
    log::{
        catalog_ack_request_intent, catalog_ack_retained_summary,
        catalog_membership_request_intent, catalog_membership_retained_summary,
        decode_catalog_ack_reply, encode_catalog_ack_request, encode_catalog_membership_request,
    },
    state::{EncodedCreditCatalogMembers, encode_credit_catalog_members},
};
use wf_types::identity::RequestId;

pub(super) fn members<P: CreditCatalogPublicationPort>(
    port: &mut P,
    source: &RequestScope,
    target: &RequestScope,
    catalog: &[u8],
    scopes: &[&[u8]],
    limits: Limits,
) -> bool {
    let Ok(encoded) = encode_credit_catalog_members(
        source,
        scopes,
        limits.codec.admission().decision_bytes(),
        limits.maximum_members_bytes,
    ) else {
        return false;
    };
    encoded_members(port, target, catalog, &encoded, limits)
}

pub(in crate::governance) fn encoded_members<P: CreditCatalogPublicationPort>(
    port: &mut P,
    target: &RequestScope,
    catalog: &[u8],
    encoded: &EncodedCreditCatalogMembers,
    limits: Limits,
) -> bool {
    let mut members = Vec::new();
    if members.try_reserve_exact(encoded.members().len()).is_err() {
        return false;
    }
    members.extend(encoded.members());
    let command = CatalogMembershipChange {
        catalog,
        action: CatalogMembershipAction::Register,
        members: &members,
    };
    let Ok(intent) = catalog_membership_request_intent(command) else {
        return false;
    };
    let Ok((request, stamp)) = port.prepare_catalog_command(target) else {
        return false;
    };
    let Ok(frame) = encode_catalog_membership_request(
        target,
        &request,
        stamp,
        command,
        limits.codec,
        limits
            .codec
            .admission()
            .decision_bytes()
            .saturating_add(2048),
    ) else {
        return false;
    };
    let MutationSubmission::Retained(outcome) =
        port.submit_catalog_command(target, &request, frame, limits.execution_workspace_bytes)
    else {
        return false;
    };
    if !bound(&outcome, target, &request, intent) {
        return false;
    }
    matches!(
        catalog_membership_retained_summary(
            intent,
            outcome.response_version(),
            outcome.response(),
            limits.codec.admission()
        ),
        Ok((RetainedStatus::Accepted, None))
    ) && outcome.status() == RetainedStatus::Accepted
}

pub(super) fn ack<P: CreditCatalogPublicationPort>(
    port: &mut P,
    source: &RequestScope,
    command: CreditCatalogAckRequest,
    limits: Limits,
) -> bool {
    let Ok(intent) = catalog_ack_request_intent(command) else {
        return false;
    };
    let Ok((request, stamp)) = port.prepare_catalog_command(source) else {
        return false;
    };
    let Ok(frame) = encode_catalog_ack_request(
        source,
        &request,
        stamp,
        command,
        limits.codec,
        limits
            .codec
            .admission()
            .decision_bytes()
            .saturating_add(2048),
    ) else {
        return false;
    };
    let MutationSubmission::Retained(outcome) =
        port.submit_catalog_command(source, &request, frame, limits.execution_workspace_bytes)
    else {
        return false;
    };
    if !bound(&outcome, source, &request, intent) {
        return false;
    }
    if catalog_ack_retained_summary(
        intent,
        outcome.response_version(),
        outcome.response(),
        limits.codec.admission(),
    ) != Ok((outcome.status(), None))
    {
        return false;
    }
    let Ok(CreditCatalogAckReply::Accepted { meta, deleted }) = decode_catalog_ack_reply(
        outcome.response(),
        limits.codec.admission().decision_bytes(),
        1024,
    ) else {
        return false;
    };
    meta.head() > command.up_to
        && (deleted == 0 || u64::from(deleted) == command.up_to + 1 - command.expected_head)
}

fn bound(
    outcome: &RetainedOutcome,
    scope: &RequestScope,
    request: &RequestId,
    intent: ClientIntent,
) -> bool {
    outcome.scope() == scope && outcome.request() == request && outcome.intent() == intent
}
