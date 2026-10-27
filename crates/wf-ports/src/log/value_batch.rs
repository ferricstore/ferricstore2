//! Exact value framing with one owned operation and prospective logical bounds.

use sha2::{Digest, Sha256};
use wf_domain::workflow::request::ClientIntent;

use super::{
    CodecError, CodecLimits, DecodedRequest, PreparedBytes, common, decode, digest, keys,
    reader::Reader, scan, writer::Writer,
};
use crate::consensus::{
    PreparedRequest, PreparedWorkflowValueBatch, WorkflowValueDefaults, WorkflowValueItemInput,
    WorkflowValuePolicyWitness,
};

mod apply;
mod budget;
mod frame;
mod intent;
mod metadata;
mod operation;
pub(crate) mod response;

pub use apply::{WorkflowValueDecision, decide_fresh_workflow_value_batch_touched};
pub use response::workflow_value_retained_summary;

use budget::{BASE_WORK, BYTE_WORK, ITEM_WORK, KEY_WORK, POLICY_WORK, ValueBatchCharge};

/// Encode an explicit intent9 command whose supplied values are stored bytes.
///
/// This codec grants no durable publication or successful typed-decode promise.
/// # Errors
/// Rejects invalid structure, caller association and operational admission.
pub fn encode_workflow_stored_value_batch(
    batch: &PreparedWorkflowValueBatch,
    limits: CodecLimits,
) -> Result<PreparedBytes, CodecError> {
    encode_profile(batch, limits, 9)
}

/// Decode complete stored-byte intent9 into one owned operation and metadata.
/// # Errors
/// Rejects crossed profiles, corruption, malformed fields and operational limits.
pub fn decode_workflow_stored_value_batch(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<PreparedWorkflowValueBatch, CodecError> {
    decode_profile(encoded, limits, 9)
}

/// Prepare bounded intent9 ownership and report its continuous codec work.
/// # Errors
/// Rejects crossed profiles, corruption and explicit owned/work ceilings.
pub fn prepare_workflow_stored_value_batch(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
    maximum_work: usize,
) -> Result<(DecodedRequest<'_>, usize), CodecError> {
    prepare_profile(encoded, limits, maximum_owned, maximum_work, 9)
}

/// Borrowed value metadata beside one decoded operation; no apply authority.
#[derive(Clone, Copy)]
pub struct WorkflowValueMetadata<'a> {
    /// Contained reducer profile.
    pub profile: u8,
    /// Explicit presence and value of the captured-reference marker.
    pub policy_references_captured: Option<bool>,
    /// Recorded lifecycle configuration and its content identity.
    pub defaults: WorkflowValueDefaults,
    /// Exact backing witnesses borrowed from the decoded owner.
    pub policies: &'a [WorkflowValuePolicyWitness],
    /// Ordered source presence and target inputs borrowed from that owner.
    pub inputs: &'a [WorkflowValueItemInput],
}

pub(super) struct ValueBatchMetadata {
    profile: u8,
    marker: Option<bool>,
    defaults: WorkflowValueDefaults,
    policies: Vec<WorkflowValuePolicyWitness>,
    inputs: Vec<WorkflowValueItemInput>,
}

/// Encodes one exact logical-binary value command without storage authority.
///
/// # Errors
/// Rejects malformed profiles, lengths, defaults, caller evidence or admission.
pub fn encode_workflow_value_batch(
    batch: &PreparedWorkflowValueBatch,
    limits: CodecLimits,
) -> Result<PreparedBytes, CodecError> {
    encode_profile(batch, limits, 8)
}

fn encode_profile(
    batch: &PreparedWorkflowValueBatch,
    limits: CodecLimits,
    version: u16,
) -> Result<PreparedBytes, CodecError> {
    let items = batch
        .operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?;
    if items.is_empty()
        || items.len() > limits.admission().batch_items()
        || batch.policies.is_empty()
        || batch.policies.len() > limits.admission().batch_items()
    {
        return Err(CodecError::ItemLimit);
    }
    let (_, maximum_work) = budget::limits(limits, false)?;
    let mut charge = ValueBatchCharge::new(maximum_work);
    charge.debit(BASE_WORK)?;
    for (count, multiplier) in [
        (limits.admission().decision_bytes(), BYTE_WORK),
        (items.len(), ITEM_WORK),
        (batch.policies.len(), POLICY_WORK),
    ] {
        charge.debit(
            count
                .checked_mul(multiplier)
                .ok_or(CodecError::LengthOverflow)?,
        )?;
    }
    let maximum = limits.admission().decision_bytes();
    let mut measure = Writer::measure(maximum);
    frame::write(batch, None, &mut measure, limits, version)?;
    let body_length = measure.len();
    let length = body_length
        .checked_add(32)
        .ok_or(CodecError::LengthOverflow)?;
    if length > maximum {
        return Err(CodecError::ByteLimit);
    }
    metadata::check(batch, limits)?;
    let intent = intent::derive(batch, limits, version)?;
    let mut writer = Writer::allocate(length, maximum)?;
    frame::write(batch, Some(intent), &mut writer, limits, version)?;
    let mut bytes = writer.finish()?;
    if bytes.len() != body_length || bytes.capacity() < length {
        return Err(CodecError::LengthOverflow);
    }
    let footer: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&footer);
    Ok(PreparedBytes {
        digest: digest(&bytes),
        bytes,
    })
}

/// Decodes one complete owned command, retaining no input lifetime.
///
/// # Errors
/// Rejects corruption, incoherent fields and the closed logical codec bounds.
pub fn decode_workflow_value_batch(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<PreparedWorkflowValueBatch, CodecError> {
    decode_profile(encoded, limits, 8)
}

fn decode_profile(
    encoded: &[u8],
    limits: CodecLimits,
    version: u16,
) -> Result<PreparedWorkflowValueBatch, CodecError> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let (maximum_owned, maximum_work) = budget::limits(limits, false)?;
    read_owned(encoded, limits, maximum_owned, maximum_work, version).map(|(batch, _, _)| batch)
}

/// Prepares one bounded generic owner and reports the same work account's spend.
///
/// # Errors
/// Rejects malformed fields, corruption or explicit owned/work admission.
pub fn prepare_workflow_value_batch(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
    maximum_work: usize,
) -> Result<(DecodedRequest<'_>, usize), CodecError> {
    prepare_profile(encoded, limits, maximum_owned, maximum_work, 8)
}

fn prepare_profile(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
    maximum_work: usize,
    version: u16,
) -> Result<(DecodedRequest<'_>, usize), CodecError> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let mut charge = ValueBatchCharge::new(maximum_work);
    charge.debit(BASE_WORK)?;
    let decoded = prepare_charged(encoded, limits, maximum_owned, &mut charge, version)?;
    Ok((decoded, charge.spent))
}

fn read_owned(
    encoded: &[u8],
    limits: CodecLimits,
    maximum_owned: usize,
    maximum_work: usize,
    version: u16,
) -> Result<(PreparedWorkflowValueBatch, ClientIntent, usize), CodecError> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let mut charge = ValueBatchCharge::new(maximum_work);
    charge.debit(BASE_WORK)?;
    let (items, policies, _, bytes, expected) =
        frame::preflight(encoded, limits, &mut charge, version)?;
    budget::admit(
        budget::owned(bytes, items, policies, 0, false)?,
        maximum_owned,
    )?;
    let batch = frame::materialize(encoded, limits, version)?;
    metadata::check(&batch, limits)?;
    if intent::derive(&batch, limits, version)? != expected {
        return Err(CodecError::Malformed);
    }
    Ok((batch, expected, charge.spent))
}

pub(super) fn read_decoded(
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<DecodedRequest<'_>, CodecError> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let mut charge = ValueBatchCharge::new(BASE_WORK);
    charge.debit(BASE_WORK)?;
    let version = {
        let mut reader = Reader::new(encoded, limits);
        common::stored_header(&mut reader)?.intent.version()
    };
    if !matches!(version, 8 | 9) {
        scan::legacy_preflight(encoded, limits)?;
        return decode::read(encoded, limits);
    }
    let (maximum_owned, maximum_work) = budget::limits(limits, true)?;
    if charge.spent > maximum_work {
        return Err(CodecError::WorkLimit);
    }
    charge.maximum = maximum_work;
    prepare_charged(encoded, limits, maximum_owned, &mut charge, version)
}

pub(super) fn validate(encoded: &[u8], limits: CodecLimits) -> Result<(), CodecError> {
    if encoded.len() > limits.admission().decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let mut charge = ValueBatchCharge::new(BASE_WORK);
    charge.debit(BASE_WORK)?;
    let version = {
        let mut reader = Reader::new(encoded, limits);
        common::stored_header(&mut reader)?.intent.version()
    };
    if !matches!(version, 8 | 9) {
        return scan::legacy_preflight(encoded, limits);
    }
    let (_, maximum_work) = budget::limits(limits, false)?;
    if charge.spent > maximum_work {
        return Err(CodecError::WorkLimit);
    }
    charge.maximum = maximum_work;
    frame::preflight(encoded, limits, &mut charge, version).map(|_| ())
}

fn prepare_charged<'input>(
    encoded: &'input [u8],
    limits: CodecLimits,
    maximum_owned: usize,
    charge: &mut ValueBatchCharge,
    version: u16,
) -> Result<DecodedRequest<'input>, CodecError> {
    let (items, policies, key_bytes, bytes, expected) =
        frame::preflight(encoded, limits, charge, version)?;
    charge.debit(
        key_bytes
            .checked_mul(KEY_WORK)
            .ok_or(CodecError::LengthOverflow)?,
    )?;
    budget::admit(
        budget::owned(bytes, items, policies, key_bytes, true)?,
        maximum_owned,
    )?;
    let batch = frame::materialize(encoded, limits, version)?;
    metadata::check(&batch, limits)?;
    if intent::derive(&batch, limits, version)? != expected {
        return Err(CodecError::Malformed);
    }
    let scope = [
        batch.scope.tenant().as_bytes(),
        batch.scope.group().as_bytes(),
        batch.scope.recovery().as_bytes(),
    ];
    let isolation = keys::scope(scope)?;
    let mut state_keys = budget::reserve(items, limits.admission().batch_items())?;
    for item in batch
        .operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?
    {
        state_keys.push(keys::state(
            &isolation,
            item.partition.as_bytes(),
            item.command.id.as_bytes(),
        )?);
    }
    let PreparedWorkflowValueBatch {
        scope,
        request,
        stamp,
        profile,
        policy_references_captured,
        defaults,
        policies,
        operation,
        inputs,
    } = batch;
    Ok(DecodedRequest {
        request: PreparedRequest {
            scope,
            request,
            intent: expected,
            stamp,
            operation,
        },
        encoded,
        digest: digest(encoded),
        isolation,
        state_keys,
        claim_filters: None,
        limits,
        record_batch_intent: Ok(expected),
        value_metadata: Some(ValueBatchMetadata {
            profile,
            marker: policy_references_captured,
            defaults,
            policies,
            inputs,
        }),
    })
}

impl DecodedRequest<'_> {
    /// Borrows the same decoded metadata owner without granting apply authority.
    #[must_use]
    pub fn workflow_value_metadata(&self) -> Option<WorkflowValueMetadata<'_>> {
        self.value_metadata
            .as_ref()
            .map(|value| WorkflowValueMetadata {
                profile: value.profile,
                policy_references_captured: value.marker,
                defaults: value.defaults,
                policies: &value.policies,
                inputs: &value.inputs,
            })
    }
}
