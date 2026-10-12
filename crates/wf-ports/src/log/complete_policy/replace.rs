//! Original Complete Replace preparation and freshly paid borrowed verification.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyReplaceCaller, CompletePolicyReplaceFrameHeader,
    CompletePolicyReplaceInput, CompletePolicyRequestError, PolicySchemaCharge, PolicySchemaLimits,
    prepare_complete_policy_replace_frame_caller, restore_complete_policy_replace_frame_header,
    scan_complete_policy_replace_frame,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, PreparedDigest};
use wf_domain::workflow::request::gate::CommittedHeader;
use wf_types::version::LogIdentity;

use super::super::{CodecError, PreparedBytes, keys, writer::Writer};
use super::intent::{hash_intent, map_codec_error, write_replace_intent};

mod apply;
mod decode;

pub use apply::{CompletePolicyReplaceRequestPlan, decide_complete_policy_replace_frame};

pub use decode::{DecodedCompletePolicyReplaceFrame, decode_complete_policy_replace_frame};

/// Verified original bytes and their restored private request header.
///
/// The header and hashes are mathematical preparation, not ordered authority.
pub struct VerifiedCompletePolicyReplaceFrame<'a> {
    header: CompletePolicyReplaceFrameHeader<'a>,
    digest: PreparedDigest,
    isolation: Vec<u8>,
}

impl<'a> VerifiedCompletePolicyReplaceFrame<'a> {
    /// Borrows the complete immutable input without a copied frame.
    #[must_use]
    pub fn encoded(&self) -> &'a [u8] {
        self.header.encoded()
    }

    /// Borrows the original request without materializing a policy.
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

/// Verifies original tag-six bytes on the caller's continuous schema account.
///
/// No complete policy is expanded and no current state or request gate is read.
///
/// # Errors
/// Preserves structural, profile, work, intent and fallible scope-output refusals.
pub fn verify_complete_policy_replace_frame<'a>(
    encoded: &'a [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<VerifiedCompletePolicyReplaceFrame<'a>, CompletePolicyRequestError> {
    verify_replace_frame(encoded, limits, admission, charge)
}

fn verify_replace_frame<'a>(
    encoded: &'a [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<VerifiedCompletePolicyReplaceFrame<'a>, CompletePolicyRequestError> {
    let scanned = scan_complete_policy_replace_frame(encoded, limits, admission, charge)?;
    let recomputed = Sha256::digest(scanned.original_intent()).into();
    let header = restore_complete_policy_replace_frame_header(
        scanned, recomputed, limits, admission, charge,
    )?;
    let isolation = keys::scope([
        header.scope().tenant().as_bytes(),
        header.scope().group().as_bytes(),
        header.scope().recovery().as_bytes(),
    ])
    .map_err(map_codec_error)?;
    let digest = PreparedDigest::new(Sha256::digest(header.encoded()).into());
    Ok(VerifiedCompletePolicyReplaceFrame {
        header,
        digest,
        isolation,
    })
}

/// Prepares exact original Complete Replace frame bytes under one continuous account.
///
/// Freshly admits raw owned input; no merged policy value replaces original identity.
///
/// # Errors
/// Preserves profile, work, scope, intent/frame caps and fallible output refusal.
pub fn prepare_complete_policy_replace_frame(
    input: CompletePolicyReplaceInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<PreparedBytes, CompletePolicyRequestError> {
    let (caller, intent_bytes, frame_bytes) =
        prepare_complete_policy_replace_frame_caller(input, limits, admission, charge)?;
    let intent_digest = hash_intent(&caller)?;
    let bytes = emit_replace_frame(&caller, &intent_digest, intent_bytes, frame_bytes)?;
    let digest = PreparedDigest::new(Sha256::digest(&bytes).into());
    Ok(PreparedBytes { bytes, digest })
}

fn emit_replace_frame(
    caller: &CompletePolicyReplaceCaller,
    intent_digest: &[u8; 32],
    intent_bytes: usize,
    frame_bytes: usize,
) -> Result<Vec<u8>, CompletePolicyRequestError> {
    let mut writer = Writer::allocate(frame_bytes, frame_bytes).map_err(map_codec_error)?;
    write_replace_frame_header(caller, intent_digest, intent_bytes, &mut writer)
        .map_err(map_codec_error)?;
    write_replace_intent(caller, &mut writer)?;
    if writer.len() != frame_bytes {
        return Err(CompletePolicyEncodingError::EncodingMismatch.into());
    }
    writer.finish().map_err(map_codec_error)
}

fn write_replace_frame_header(
    caller: &CompletePolicyReplaceCaller,
    intent_digest: &[u8; 32],
    intent_bytes: usize,
    writer: &mut Writer<'_>,
) -> Result<(), CodecError> {
    let input = caller.input();
    writer.write(b"FWCP")?;
    writer.u16(1)?;
    writer.byte(6)?;
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
