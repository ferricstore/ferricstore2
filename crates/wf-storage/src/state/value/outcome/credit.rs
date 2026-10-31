//! Explicit private credit receipts; legacy value selectors remain closed.

use super::{ScannedRetainedOutcome, StateValueError, add, claim_error, encode_fields, with_frame};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedOutcome, RetainedStatus};
use wf_ports::log::{
    catalog_ack_retained_summary, catalog_membership_retained_summary,
    catalog_progress_retained_summary, catalog_prune_retained_summary, credit_retained_summary,
};
use wf_types::identity::RequestId;

pub(super) fn encode(
    outcome: &RetainedOutcome,
    maximum: usize,
    admission: AdmissionLimits,
) -> Result<Vec<u8>, StateValueError> {
    let (status, _) = summary(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
        admission,
    )
    .map_err(claim_error)?;
    if maximum == 0 || status != outcome.status() {
        return Err(StateValueError::InvalidValue);
    }
    encode_fields(outcome, maximum)
}

pub(super) fn scan<'a>(
    bytes: &'a [u8],
    scope: &RequestScope,
    expected_request: &RequestId,
    admission: AdmissionLimits,
    maximum: usize,
) -> Result<ScannedRetainedOutcome<'a>, StateValueError> {
    with_frame(
        bytes,
        maximum,
        |fields,
         version,
         digest,
         original_position,
         original_time,
         expires_at,
         response_version| {
            if !matches!(
                (version, response_version),
                (12 | 17, 10) | (13, 11) | (14, 12) | (15, 13) | (16, 14)
            ) {
                return Err(StateValueError::UnsupportedVersion);
            }
            let [tenant, group, recovery, request, response] = fields;
            if tenant != scope.tenant().as_bytes()
                || group != scope.group().as_bytes()
                || recovery != scope.recovery().as_bytes()
                || request != expected_request.as_bytes()
                || expires_at <= original_time
            {
                return Err(StateValueError::InvalidValue);
            }
            let intent = if version == 17 {
                ClientIntent::public_credit(digest)
            } else if version == 16 {
                ClientIntent::governance_catalog_prune(digest)
            } else if version == 15 {
                ClientIntent::governance_catalog_progress(digest)
            } else if version == 14 {
                ClientIntent::governance_catalog(digest)
            } else if version == 13 {
                ClientIntent::credit_catalog_ack(digest)
            } else {
                ClientIntent::governance_credit(digest)
            };
            let (status, response_count) =
                summary(intent, response_version, response, admission).map_err(claim_error)?;
            let mut owned_bytes = 512;
            for field in fields {
                add(&mut owned_bytes, field.len())?;
            }
            Ok(ScannedRetainedOutcome {
                tenant,
                group,
                recovery,
                request,
                response,
                intent,
                original_position,
                original_time,
                expires_at,
                response_version,
                status,
                limits: admission,
                owned_bytes,
                response_count,
            })
        },
    )
}

fn summary(
    intent: ClientIntent,
    version: u16,
    response: &[u8],
    admission: AdmissionLimits,
) -> Result<(RetainedStatus, Option<usize>), wf_ports::log::CodecError> {
    if intent.version() == 16 {
        catalog_prune_retained_summary(intent, version, response, admission)
            .map(|_| (RetainedStatus::Accepted, None))
    } else if intent.version() == 15 {
        catalog_progress_retained_summary(intent, version, response, admission)
    } else if intent.version() == 14 {
        catalog_membership_retained_summary(intent, version, response, admission)
    } else if intent.version() == 13 {
        catalog_ack_retained_summary(intent, version, response, admission)
    } else {
        credit_retained_summary(intent, version, response, admission)
    }
}
