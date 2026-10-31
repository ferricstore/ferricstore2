//! Shared retained framing with explicit versioned response validation.

use wf_domain::policy::complete::{PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RetainedMetadata, RetainedOutcome, RetainedStatus,
};
use wf_ports::log::{
    CodecError, claim_retained_summary, public_mutation_retained_status,
    record_batch_retained_status,
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, LogIdentity, TimestampMillis};

use super::{
    StateValueError, add, allocate, copy, finish, put_raw, raw, raw_size, read_u16, read_u64,
    start, take,
};

mod atomic_start;
mod credit;
mod governed_claim;
mod policy;
mod release_maintenance;
mod step_continue;
pub(crate) use policy::outcome_policy_validation_bytes_in_layout;
pub use policy::{encode_outcome_profiled, outcome_policy_validation_bytes, scan_outcome_profiled};
pub(crate) use step_continue::{encode_outcome_in_layout, scan_outcome_in_layout};
pub use step_continue::{encode_step_continue_outcome, scan_step_continue_outcome};

/// Validated borrowed metadata and response, without transaction authority.
///
/// The view borrows only its encoded donor. Materialization freshly admits its
/// complete logical owner size; scanning does not debit a transaction budget.
pub struct ScannedRetainedOutcome<'input> {
    tenant: &'input [u8],
    group: &'input [u8],
    recovery: &'input [u8],
    request: &'input [u8],
    response: &'input [u8],
    intent: ClientIntent,
    original_position: LogIdentity,
    original_time: TimestampMillis,
    expires_at: TimestampMillis,
    response_version: u16,
    status: RetainedStatus,
    limits: AdmissionLimits,
    owned_bytes: usize,
    response_count: Option<usize>,
}

impl ScannedRetainedOutcome<'_> {
    pub(crate) const fn governed_claim_required(&self) -> bool {
        matches!(self.intent.version(), 19 | 20)
    }
    pub(crate) const fn workflow_release_required(&self) -> bool {
        self.intent.version() == 18
            || matches!(
                self.status,
                RetainedStatus::IndexedRejected(
                    wf_domain::workflow::indexed::IndexRejection::ReleaseBacklogFull
                )
            )
    }
    pub(crate) const fn credit_catalog_required(&self) -> bool {
        use wf_domain::governance::identified::IdentifiedCreditRejection as R;
        matches!(self.intent.version(), 13 | 17)
            || matches!(
                self.status,
                RetainedStatus::GovernanceRejected(
                    R::CatalogBacklogFull | R::CatalogSequenceExhausted
                )
            )
    }

    pub(crate) const fn intent_version(&self) -> u16 {
        self.intent.version()
    }

    /// Reads 512 plus every variable byte returned by materialization.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned_bytes
    }

    /// Reads the structural vector count, independent of configured transaction caps.
    #[must_use]
    pub const fn response_count(&self) -> Option<usize> {
        self.response_count
    }

    /// Consumes parsed fields after admitting their complete logical owner size.
    ///
    /// # Errors
    /// Rejects a zero/short owner cap, failed field allocation or typed restoration.
    pub fn into_owned(
        self,
        maximum_owned_bytes: usize,
    ) -> Result<RetainedOutcome, StateValueError> {
        if maximum_owned_bytes == 0 {
            return Err(StateValueError::InvalidValue);
        }
        if self.owned_bytes > maximum_owned_bytes {
            return Err(StateValueError::Limit {
                bytes: self.owned_bytes,
                maximum: maximum_owned_bytes,
            });
        }
        let scope = RequestScope::new(
            TenantId::new(copy(self.tenant)?).map_err(|_| StateValueError::InvalidValue)?,
            GroupId::new(copy(self.group)?).map_err(|_| StateValueError::InvalidValue)?,
            RecoveryId::new(copy(self.recovery)?).map_err(|_| StateValueError::InvalidValue)?,
        );
        RetainedOutcome::new(
            RetainedMetadata {
                scope,
                request: RequestId::new(copy(self.request)?)
                    .map_err(|_| StateValueError::InvalidValue)?,
                intent: self.intent,
                original_position: self.original_position,
                original_time: self.original_time,
                expires_at: self.expires_at,
                status: self.status,
            },
            self.response_version,
            copy(self.response)?,
            self.limits,
        )
        .map_err(|_| StateValueError::InvalidValue)
    }
}

/// Encodes original metadata with its explicitly supported canonical response.
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
    let status = if outcome.intent().version() == 1 && outcome.response_version() == 3 {
        // Wire-only syntax/ceiling admission. The actual guard, point reader and
        // recovery repeat the narrower captured response profile independently.
        let ceiling = AdmissionLimits::new(
            wf_domain::workflow::admission::MAX_INLINE_BYTES,
            maximum_bytes,
            1000,
        )
        .map_err(|_| StateValueError::InvalidValue)?;
        claim_retained_summary(
            outcome.intent(),
            3,
            outcome.response(),
            outcome.original_time(),
            ceiling,
        )
        .map_err(claim_error)?
        .0
    } else if outcome.intent().version() == 7 && outcome.response_version() == 4 {
        atomic_start::wire_status(outcome, maximum_bytes)?
    } else if matches!(outcome.intent().version(), 8 | 9) {
        // The profiled encoder and real reads repeat the captured admission.
        let ceiling = AdmissionLimits::new(
            wf_domain::workflow::admission::MAX_INLINE_BYTES,
            maximum_bytes,
            wf_domain::workflow::admission::MAX_BATCH_ITEMS,
        )
        .map_err(|_| StateValueError::InvalidValue)?;
        policy::workflow_value_status(outcome, ceiling)?
    } else {
        let parse = if outcome.intent().version() == 1 {
            public_mutation_retained_status
        } else {
            record_batch_retained_status
        };
        parse(
            outcome.intent(),
            outcome.response_version(),
            outcome.response(),
        )?
    };
    if status != outcome.status() {
        return Err(StateValueError::InvalidValue);
    }
    encode_fields(outcome, maximum_bytes)
}

fn encode_fields(
    outcome: &RetainedOutcome,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
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

/// Scans a complete FWSO1 frame bound to an expected original scope and request.
///
/// All frame, scope, profile, response and expiry checks precede materialization.
///
/// # Errors
/// Rejects unsupported/malformed responses, foreign scope, wire caps or invalid values.
pub fn scan_outcome<'input>(
    bytes: &'input [u8],
    expected_scope: &RequestScope,
    expected_request: &RequestId,
    limits: AdmissionLimits,
    maximum_wire_bytes: usize,
) -> Result<ScannedRetainedOutcome<'input>, StateValueError> {
    scan_with_policy(
        bytes,
        expected_scope,
        expected_request,
        limits,
        maximum_wire_bytes,
        None,
    )
}

fn scan_with_policy<'input>(
    bytes: &'input [u8],
    expected_scope: &RequestScope,
    expected_request: &RequestId,
    limits: AdmissionLimits,
    maximum_wire_bytes: usize,
    profile: Option<(PolicySchemaLimits, &mut PolicySchemaCharge, usize)>,
) -> Result<ScannedRetainedOutcome<'input>, StateValueError> {
    with_frame(
        bytes,
        maximum_wire_bytes,
        |fields,
         intent_version,
         digest,
         original_position,
         original_time,
         expires_at,
         response_version| {
            let [tenant, group, recovery, request, response] = fields;
            let extended_response = matches!((intent_version, response_version), (1, 3) | (7, 4))
                || matches!(intent_version, 8 | 9)
                || (matches!(intent_version, 2 | 4) && profile.is_some());
            if tenant != expected_scope.tenant().as_bytes()
                || group != expected_scope.group().as_bytes()
                || recovery != expected_scope.recovery().as_bytes()
                || request != expected_request.as_bytes()
                || expires_at <= original_time
                || (response.len() > limits.decision_bytes() && !extended_response)
            {
                return Err(StateValueError::InvalidValue);
            }
            let intent = policy::restore_intent(intent_version, digest, profile.is_some())?;
            let (status, response_count) = policy::response_summary(
                intent,
                response_version,
                response,
                (original_position, original_time),
                limits,
                profile,
            )?;
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
                limits,
                owned_bytes,
                response_count,
            })
        },
    )
}

fn with_frame<'input, T>(
    bytes: &'input [u8],
    maximum_wire_bytes: usize,
    inspect: impl FnOnce(
        [&'input [u8]; 5],
        u16,
        [u8; 32],
        LogIdentity,
        TimestampMillis,
        TimestampMillis,
        u16,
    ) -> Result<T, StateValueError>,
) -> Result<T, StateValueError> {
    let mut offset = start(bytes, *b"FWSO", maximum_wire_bytes)?;
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
    inspect(
        [tenant, group, recovery, request, response],
        intent_version,
        digest,
        LogIdentity::new(term, node, AppliedPosition::new(index)),
        TimestampMillis::new(original_time),
        TimestampMillis::new(expires_at),
        response_version,
    )
}

fn claim_error(error: CodecError) -> StateValueError {
    match error {
        CodecError::ByteLimit | CodecError::ItemLimit | CodecError::ValueLimit => {
            StateValueError::ProfileLimit
        }
        other => other.into(),
    }
}

/// Validates a complete frame, then consumes the same scan into an owned outcome.
///
/// This legacy entry preserves its wire-only cap contract.
/// # Errors
/// Rejects unsupported/corrupt values, explicit caps, admission or allocation.
pub fn decode_outcome(
    bytes: &[u8],
    expected_scope: &RequestScope,
    expected_request: &RequestId,
    limits: AdmissionLimits,
    maximum_bytes: usize,
) -> Result<RetainedOutcome, StateValueError> {
    scan_outcome(
        bytes,
        expected_scope,
        expected_request,
        limits,
        maximum_bytes,
    )?
    .into_owned(usize::MAX)
}
