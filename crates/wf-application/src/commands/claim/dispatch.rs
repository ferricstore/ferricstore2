//! Validate exact originals; keep shared bytes through every unresolved result.
use super::{
    PreparedPublicClaimCall, PublicClaimCallError as E, PublicClaimCallLimits as Limits,
    PublicClaimCallRequest as Request, PublicClaimCallResult as R, PublicClaimRetryError,
};
use std::sync::Arc;
use wf_domain::{
    claim::caller::NormalizedClaimDue,
    workflow::request::{ClientIntent, RequestScope, RetainedOutcome},
};
use wf_ports::{
    consensus::{
        MutationSubmission as M, MutationUnknown as U, PublicClaimRequestLookup as L,
        PublicClaimSubmitPort,
    },
    log::{inspect_public_claim_reply_for_call, prepare_public_claim_request},
};
use wf_types::identity::RequestId;

pub(super) fn lookup(result: M, request: Request<'_>, intent: ClientIntent, limits: Limits) -> R {
    match result {
        M::Retained(outcome) => {
            if valid(
                &outcome,
                (request.scope, request.request, intent),
                request.caller,
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
pub(super) fn submit<P: PublicClaimSubmitPort>(
    original: Box<PreparedPublicClaimCall>,
    port: &mut P,
    limits: Limits,
    inquire: bool,
) -> Result<R, PublicClaimRetryError> {
    let checked = {
        let parsed = match prepare_public_claim_request(
            original.frame(),
            limits.read.codec,
            limits.caller,
            limits.maximum_prepared_bytes,
        ) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Err(PublicClaimRetryError {
                    error: E::Codec(error),
                    original,
                });
            }
        };
        let root = parsed.prepared();
        if root.scope() != &original.scope
            || root.request() != &original.request
            || root.intent() != original.intent
        {
            return Err(PublicClaimRetryError {
                error: E::InvalidOptions,
                original,
            });
        }
        let caller = match root.caller(limits.caller) {
            Ok(caller) => caller,
            Err(error) => {
                return Err(PublicClaimRetryError {
                    error: E::Codec(error),
                    original,
                });
            }
        };
        let prior = if inquire {
            match port.lookup_public_claim(
                &original.scope,
                &original.request,
                original.intent,
                limits.read,
            ) {
                Ok(L::Fresh) => None,
                Ok(L::Resolved(result)) => Some(result),
                Err(reason) => Some(M::NotSubmitted(reason)),
            }
        } else {
            None
        };
        let result = prior.unwrap_or_else(|| {
            port.submit_public_claim(
                &original.scope,
                &original.request,
                Arc::clone(&original.frame),
                limits.execution_workspace_bytes,
            )
        });
        let valid = match &result {
            M::Retained(outcome) => valid(
                outcome,
                (&original.scope, &original.request, original.intent),
                &caller,
                limits,
            ),
            _ => true,
        };
        (result, valid)
    };
    Ok(match checked {
        (M::Retained(outcome), true) => R::Retained(outcome),
        (M::Retained(_), false) => R::Unknown {
            reason: U::InvalidReply,
            original,
        },
        (M::Unknown(reason), _) => R::Unknown { reason, original },
        (M::NotSubmitted(reason), _) => R::Deferred { reason, original },
        (M::IntentConflict, _) => R::IntentConflict,
        (M::Expired, _) => R::Expired,
    })
}
fn valid(
    outcome: &RetainedOutcome,
    binding: (&RequestScope, &RequestId, ClientIntent),
    caller: &NormalizedClaimDue<'_>,
    limits: Limits,
) -> bool {
    outcome.scope() == binding.0
        && outcome.request() == binding.1
        && outcome.intent() == binding.2
        && outcome.response_version() == 18
        && outcome.response().len() <= limits.maximum_response_bytes
        && inspect_public_claim_reply_for_call(
            outcome.response(),
            outcome.original_time(),
            caller,
            limits.reply,
        )
        .is_ok_and(|reply| reply.status() == outcome.status())
}
