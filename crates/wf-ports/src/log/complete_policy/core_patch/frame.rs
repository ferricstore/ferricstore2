//! Closed original core-Patch frame preparation and paid borrowed verification.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchCaller, CompleteCorePolicyPatchFrameHeader,
    CompleteCorePolicyPatchRequestInput, CompletePolicyEncodingError, CompletePolicyRequestError,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, ScannedCorePatchFrame,
    prepare_complete_core_policy_patch_frame_caller,
    restore_complete_core_policy_patch_frame_header, scan_complete_core_policy_patch_frame,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, PreparedDigest};
use wf_domain::workflow::request::gate::CommittedHeader;
use wf_types::version::LogIdentity;

use super::super::super::{CodecError, PreparedBytes, keys, writer::Writer};
use super::{hash_core_patch_intent, map_codec_error, write_core_patch_intent};

mod apply;
mod decode;

pub use apply::{CompleteCorePolicyPatchRequestPlan, decide_complete_core_policy_patch_frame};
pub use decode::{DecodedCompleteCorePolicyPatchFrame, decode_complete_core_policy_patch_frame};

/// Verified original bytes and their restored private request header.
///
/// The header and hashes are mathematical preparation, not ordered authority.
pub struct VerifiedCompleteCorePolicyPatchFrame<'a> {
    header: CompleteCorePolicyPatchFrameHeader<'a>,
    digest: PreparedDigest,
    isolation: Vec<u8>,
}

impl<'a> VerifiedCompleteCorePolicyPatchFrame<'a> {
    /// Borrows the complete immutable input without a copied frame.
    #[must_use]
    pub fn encoded(&self) -> &'a [u8] {
        self.header.encoded()
    }

    /// Borrows the original partial command without materializing a policy.
    #[must_use]
    pub fn original_intent(&self) -> &'a [u8] {
        self.header.original_intent()
    }

    /// Reads the recomputed identity of the complete frame.
    #[must_use]
    pub const fn digest(&self) -> PreparedDigest {
        self.digest
    }

    /// Borrows the canonical isolation bytes owned by this result.
    #[must_use]
    pub fn isolation_scope(&self) -> &[u8] {
        &self.isolation
    }

    /// Joins a separately supplied log identity to verified frame identity.
    #[must_use]
    pub fn header(&self, log: LogIdentity) -> CommittedHeader<'_> {
        CommittedHeader {
            format_version: 1,
            scope: self.header.scope(),
            request: self.header.request(),
            intent: self.header.intent(),
            identity: EntryIdentity::new(log, self.digest),
            stamp: self.header.stamp(),
            encoded_entry: self.header.encoded(),
            encoded_isolation_scope: &self.isolation,
        }
    }
}

/// Verifies original tag-five bytes on the caller's continuous schema account.
///
/// No partial policy is expanded and no current state or request gate is read.
///
/// # Errors
/// Preserves structural, profile, work, intent and fallible scope-output refusals.
pub fn verify_complete_core_policy_patch_frame<'a>(
    encoded: &'a [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<VerifiedCompleteCorePolicyPatchFrame<'a>, CompletePolicyRequestError> {
    verify_frame(encoded, limits, admission, charge)
}

fn verify_frame<'a>(
    encoded: &'a [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<VerifiedCompleteCorePolicyPatchFrame<'a>, CompletePolicyRequestError> {
    let scanned = scan_complete_core_policy_patch_frame(encoded, limits, admission, charge)?;
    let recomputed = hash_original_intent(&scanned);
    let header = restore_complete_core_policy_patch_frame_header(
        scanned, recomputed, limits, admission, charge,
    )?;
    let isolation = emit_scope(&header)?;
    let digest = hash_frame(header.encoded());
    Ok(VerifiedCompleteCorePolicyPatchFrame {
        header,
        digest,
        isolation,
    })
}

fn hash_original_intent(scanned: &ScannedCorePatchFrame<'_>) -> [u8; 32] {
    Sha256::digest(scanned.original_intent()).into()
}

fn emit_scope(
    header: &CompleteCorePolicyPatchFrameHeader<'_>,
) -> Result<Vec<u8>, CompletePolicyRequestError> {
    keys::scope([
        header.scope().tenant().as_bytes(),
        header.scope().group().as_bytes(),
        header.scope().recovery().as_bytes(),
    ])
    .map_err(|error| match error {
        CodecError::ScopeLimit => CompletePolicyRequestError::ScopeLimit,
        CodecError::LengthOverflow => PolicySchemaError::LengthOverflow.into(),
        CodecError::Allocation => CompletePolicyEncodingError::AllocationFailure.into(),
        CodecError::ByteLimit | CodecError::ValueLimit => {
            CompletePolicyEncodingError::ByteLimit.into()
        }
        _ => CompletePolicyEncodingError::Malformed.into(),
    })
}

fn hash_frame(encoded: &[u8]) -> PreparedDigest {
    PreparedDigest::new(Sha256::digest(encoded).into())
}

/// Prepares exact original core-Patch frame bytes under one continuous account.
///
/// Freshly admits raw owned input; no merged policy value replaces partial identity.
///
/// # Errors
/// Preserves profile, work, scope, intent/frame caps and fallible output refusal.
pub fn prepare_complete_core_policy_patch_frame(
    input: CompleteCorePolicyPatchRequestInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<PreparedBytes, CompletePolicyRequestError> {
    let (caller, intent_bytes, frame_bytes) =
        prepare_complete_core_policy_patch_frame_caller(input, limits, admission, charge)?;
    let intent_digest = hash_core_patch_intent(&caller)?;
    let bytes = emit_core_patch_frame(&caller, &intent_digest, intent_bytes, frame_bytes)?;
    let digest = hash_frame(&bytes);
    Ok(PreparedBytes { bytes, digest })
}

fn emit_core_patch_frame(
    caller: &CompleteCorePolicyPatchCaller,
    intent_digest: &[u8; 32],
    intent_bytes: usize,
    frame_bytes: usize,
) -> Result<Vec<u8>, CompletePolicyRequestError> {
    let mut writer = Writer::allocate(frame_bytes, frame_bytes).map_err(map_codec_error)?;
    write_core_patch_frame_header(caller, intent_digest, intent_bytes, &mut writer)
        .map_err(map_codec_error)?;
    write_core_patch_intent(caller, &mut writer)?;
    if writer.len() != frame_bytes {
        return Err(CompletePolicyEncodingError::EncodingMismatch.into());
    }
    writer.finish().map_err(map_codec_error)
}

fn write_core_patch_frame_header(
    caller: &CompleteCorePolicyPatchCaller,
    intent_digest: &[u8; 32],
    intent_bytes: usize,
    writer: &mut Writer<'_>,
) -> Result<(), CodecError> {
    let input = caller.input();
    writer.write(b"FWCP")?;
    writer.u16(1)?;
    writer.byte(5)?;
    writer.raw(input.scope().tenant().as_bytes(), 8_192)?;
    writer.raw(input.scope().group().as_bytes(), 256)?;
    writer.raw(input.scope().recovery().as_bytes(), 256)?;
    writer.raw(input.request().as_bytes(), 256)?;
    writer.u16(2)?;
    writer.write(intent_digest)?;
    writer.u64(input.stamp().hlc().physical().get())?;
    writer.u16(input.stamp().hlc().logical())?;
    writer.u64(input.stamp().wall().get())?;
    writer.u32(u32::try_from(intent_bytes).or(Err(CodecError::LengthOverflow))?)?;
    Ok(())
}
