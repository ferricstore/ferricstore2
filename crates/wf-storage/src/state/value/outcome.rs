//! One retained batch5 response grammar, owned by the ports response parser.

use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome};
use wf_ports::log::record_batch_retained_status;
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, LogIdentity, TimestampMillis};

use super::{
    StateValueError, add, allocate, copy, finish, put_raw, raw, raw_size, read_u16, read_u64,
    start, take,
};

/// Encodes original metadata and a single canonical batch5/response1 body.
///
/// # Errors
/// Rejects other profiles, inconsistent status/body, byte limits or allocation failure.
pub fn encode_outcome(
    outcome: &RetainedOutcome,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    if maximum_bytes == 0 {
        return Err(StateValueError::InvalidValue);
    }
    let status = record_batch_retained_status(
        outcome.intent(),
        outcome.response_version(),
        outcome.response(),
    )?;
    if status != outcome.status() {
        return Err(StateValueError::InvalidValue);
    }
    let scope = outcome.scope();
    let identities = [
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        outcome.request().as_bytes(),
    ];
    let mut size = 6 + 2 + 32 + 24 + 16 + 2;
    for value in identities.into_iter().chain([outcome.response()]) {
        add(&mut size, raw_size(value)?)?;
    }
    let mut output = allocate(size, maximum_bytes, *b"FWSO")?;
    for value in identities {
        put_raw(&mut output, value)?;
    }
    output.extend_from_slice(&outcome.intent().version().to_be_bytes());
    output.extend_from_slice(outcome.intent().digest());
    let log = outcome.original_position();
    output.extend_from_slice(&log.term().to_be_bytes());
    output.extend_from_slice(&log.leader_node().to_be_bytes());
    output.extend_from_slice(&log.index().get().to_be_bytes());
    output.extend_from_slice(&outcome.original_time().get().to_be_bytes());
    output.extend_from_slice(&outcome.expires_at().get().to_be_bytes());
    output.extend_from_slice(&outcome.response_version().to_be_bytes());
    put_raw(&mut output, outcome.response())?;
    Ok(output)
}

/// Binds a complete FWSO1 frame to an expected original scope and request.
///
/// All frame, scope, profile, response and expiry checks precede field copies.
///
/// # Errors
/// Rejects unsupported/malformed responses, foreign scope, limits and allocation failure.
pub fn decode_outcome(
    bytes: &[u8],
    expected_scope: &RequestScope,
    expected_request: &RequestId,
    limits: AdmissionLimits,
    maximum_bytes: usize,
) -> Result<RetainedOutcome, StateValueError> {
    let mut offset = start(bytes, *b"FWSO", maximum_bytes)?;
    let tenant = raw(bytes, &mut offset)?;
    let group = raw(bytes, &mut offset)?;
    let recovery = raw(bytes, &mut offset)?;
    let request = raw(bytes, &mut offset)?;
    let intent_version = read_u16(bytes, &mut offset)?;
    let digest: [u8; 32] = take(bytes, &mut offset, 32)?
        .try_into()
        .map_err(|_| StateValueError::Malformed)?;
    let term = read_u64(bytes, &mut offset)?;
    let node = read_u64(bytes, &mut offset)?;
    let index = read_u64(bytes, &mut offset)?;
    let original_time = read_u64(bytes, &mut offset)?;
    let expires_at = read_u64(bytes, &mut offset)?;
    let response_version = read_u16(bytes, &mut offset)?;
    let response = raw(bytes, &mut offset)?;
    finish(bytes, offset)?;
    if tenant != expected_scope.tenant().as_bytes()
        || group != expected_scope.group().as_bytes()
        || recovery != expected_scope.recovery().as_bytes()
        || request != expected_request.as_bytes()
        || expires_at <= original_time
        || response.len() > limits.decision_bytes()
    {
        return Err(StateValueError::InvalidValue);
    }
    if intent_version != 5 {
        return Err(StateValueError::UnsupportedVersion);
    }
    let intent = ClientIntent::record_batch(digest);
    let status = record_batch_retained_status(intent, response_version, response)?;
    let scope = RequestScope::new(
        TenantId::new(copy(tenant)?).map_err(|_| StateValueError::InvalidValue)?,
        GroupId::new(copy(group)?).map_err(|_| StateValueError::InvalidValue)?,
        RecoveryId::new(copy(recovery)?).map_err(|_| StateValueError::InvalidValue)?,
    );
    RetainedOutcome::new(
        RetainedMetadata {
            scope,
            request: RequestId::new(copy(request)?).map_err(|_| StateValueError::InvalidValue)?,
            intent,
            original_position: LogIdentity::new(term, node, AppliedPosition::new(index)),
            original_time: TimestampMillis::new(original_time),
            expires_at: TimestampMillis::new(expires_at),
            status,
        },
        response_version,
        copy(response)?,
        limits,
    )
    .map_err(|_| StateValueError::InvalidValue)
}
