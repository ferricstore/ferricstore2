//! Bound retained originals and supervised resubmission of one immutable frame.
use super::{
    PreparedPublicCreditCall, PublicCreditCallError as E, PublicCreditCallLimits as Limits,
    PublicCreditCallRequest as Request, PublicCreditCallResult as R, PublicCreditRetryError,
};
use std::sync::Arc;
use wf_domain::{
    governance::caller::{CreditCallKind as K, CreditIdList},
    workflow::request::{ClientIntent, RequestScope, RetainedOutcome, RetainedStatus},
};
use wf_ports::{
    consensus::{MutationSubmission as M, MutationUnknown as U, PublicCreditSubmitPort},
    log::{PublicCreditReplyExpectation as Expected, inspect_public_credit_reply},
};
use wf_types::identity::RequestId;

pub(super) fn lookup(result: M, request: Request<'_>, intent: ClientIntent, limits: Limits) -> R {
    match result {
        M::Retained(outcome) => {
            let (shard, amount) = super::prepare::operands(request.caller);
            let expected = Expected {
                scope: request.caller.scope(),
                kind: request.caller.action().kind(),
                shard,
                amount,
                ids: None,
            };
            if valid(
                &outcome,
                (
                    request.source,
                    request.request,
                    intent,
                    request.caller.now_ms(),
                ),
                expected,
                limits,
            ) {
                R::Retained(outcome)
            } else {
                R::Unresolved(U::InvalidReply)
            }
        }
        M::IntentConflict => R::IntentConflict,
        M::Expired => R::Expired,
        M::NotSubmitted(reason) => R::NotSubmitted(reason),
        M::Unknown(reason) => R::Unresolved(reason),
    }
}

pub(super) fn submit<P: PublicCreditSubmitPort>(
    original: Box<PreparedPublicCreditCall>,
    port: &mut P,
    limits: Limits,
) -> Result<R, PublicCreditRetryError> {
    let result = port.submit_public_credit(
        &original.source,
        &original.request,
        Arc::clone(&original.frame),
        limits.publication.execution_workspace_bytes,
    );
    Ok(match result {
        M::Retained(outcome) => retained(outcome, original, port, limits),
        M::IntentConflict => R::IntentConflict,
        M::Expired => R::Expired,
        M::NotSubmitted(reason) => {
            return Err(PublicCreditRetryError {
                error: E::Preparation(reason),
                original,
            });
        }
        M::Unknown(reason) => R::Unknown { reason, original },
    })
}

fn retained<P: PublicCreditSubmitPort>(
    outcome: Box<RetainedOutcome>,
    original: Box<PreparedPublicCreditCall>,
    port: &mut P,
    limits: Limits,
) -> R {
    let expected = Expected {
        scope: &original.credit_scope,
        kind: original.kind,
        shard: original.shard,
        amount: original.amount,
        ids: (original.kind == K::Spend).then_some(CreditIdList::Owned(&original.ids)),
    };
    if !valid(
        &outcome,
        (
            &original.source,
            &original.request,
            original.intent,
            Some(original.stamp.hlc().physical().get()),
        ),
        expected,
        limits,
    ) {
        return R::Unknown {
            reason: U::InvalidReply,
            original,
        };
    }
    if original.kind == K::Lease {
        let publication = super::super::repair_credit_catalog(
            port,
            &original.source,
            &original.target,
            &original.catalog,
            &original.credit_scope,
            limits.publication,
        );
        if let Err(reason) = publication
            && outcome.status() == RetainedStatus::Accepted
        {
            return R::PublicationUnconfirmed { outcome, reason };
        }
    }
    R::Retained(outcome)
}

fn valid(
    outcome: &RetainedOutcome,
    binding: (&RequestScope, &RequestId, ClientIntent, Option<u64>),
    expected: Expected<'_>,
    limits: Limits,
) -> bool {
    outcome.scope() == binding.0
        && outcome.request() == binding.1
        && outcome.intent() == binding.2
        && binding
            .3
            .is_none_or(|minimum| outcome.original_time().get() >= minimum)
        && outcome.response().len() <= limits.maximum_response_bytes
        && inspect_public_credit_reply(
            outcome.intent(),
            outcome.response_version(),
            outcome.response(),
            expected,
            limits.publication.read.credit,
            limits.publication.codec.admission(),
        )
        .is_ok_and(|status| status == outcome.status())
}
