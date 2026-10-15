//! Original whole-flow transport hashing and freshly paid borrowed verification.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyRequestError, OriginalFlowPolicyReplaceFrameHeader,
    OriginalFlowPolicyReplaceInput, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    prepare_original_flow_policy_replace_frame_bytes,
    restore_original_flow_policy_replace_frame_header, scan_original_flow_policy_replace_frame,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, PreparedDigest};
use wf_domain::workflow::request::gate::CommittedHeader;
use wf_types::version::LogIdentity;

use super::super::{PreparedBytes, keys};
use super::intent::map_codec_error;

mod apply;

pub use apply::{OriginalFlowPolicyReplaceRequestPlan, decide_original_flow_policy_replace_frame};

/// Verified original bytes and independently restored mathematical header owners.
///
/// This result grants no retained-request, ordered-apply or installation authority.
pub struct VerifiedOriginalFlowPolicyReplaceFrame<'frame> {
    header: OriginalFlowPolicyReplaceFrameHeader<'frame>,
    digest: PreparedDigest,
    isolation: Vec<u8>,
}

impl<'frame> VerifiedOriginalFlowPolicyReplaceFrame<'frame> {
    /// Borrows the complete immutable frame without copying it.
    #[must_use]
    pub fn encoded(&self) -> &'frame [u8] {
        self.header.encoded()
    }

    /// Borrows canonical intent, excluding captured source-order sidecar bytes.
    #[must_use]
    pub fn original_intent(&self) -> &'frame [u8] {
        self.header.original_intent()
    }

    /// Reads the independently recomputed identity of the complete frame.
    #[must_use]
    pub const fn digest(&self) -> PreparedDigest {
        self.digest
    }

    /// Borrows the independently owned canonical isolation bytes.
    #[must_use]
    pub fn isolation_scope(&self) -> &[u8] {
        &self.isolation
    }

    /// Joins separately supplied log identity to this freshly verified frame.
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

/// Prepares original represented input and hashes its canonical intent and frame.
///
/// The domain prospectively admits both hashes and the digest overwrite on this
/// same account. Ports performs them directly in the one final frame owner.
///
/// # Errors
/// Preserves profile, scope, represented-class, bounds, work and allocation errors.
pub fn prepare_original_flow_policy_replace_frame(
    input: OriginalFlowPolicyReplaceInput<'_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<PreparedBytes, CompletePolicyRequestError> {
    let (mut bytes, digest_offset, intent_range) =
        prepare_original_flow_policy_replace_frame_bytes(input, limits, admission, charge)?;
    let intent = bytes
        .get(intent_range)
        .ok_or(CompletePolicyEncodingError::EncodingMismatch)?;
    let intent_digest: [u8; 32] = Sha256::digest(intent).into();
    let digest_end = digest_offset
        .checked_add(intent_digest.len())
        .ok_or(PolicySchemaError::LengthOverflow)?;
    bytes
        .get_mut(digest_offset..digest_end)
        .ok_or(CompletePolicyEncodingError::EncodingMismatch)?
        .copy_from_slice(&intent_digest);
    let digest = PreparedDigest::new(Sha256::digest(&bytes).into());
    Ok(PreparedBytes { bytes, digest })
}

/// Verifies one exact original frame without constructing an adapter tree.
///
/// The scan and restoration prospectively pay each reached hash and owner on the
/// supplied continuous account. Current policies and the request gate are unread.
///
/// # Errors
/// Preserves structural, profile, work, intent and fallible owner refusals.
pub fn verify_original_flow_policy_replace_frame<'frame>(
    encoded: &'frame [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<VerifiedOriginalFlowPolicyReplaceFrame<'frame>, CompletePolicyRequestError> {
    let scanned = scan_original_flow_policy_replace_frame(encoded, limits, admission, charge)?;
    let recomputed = Sha256::digest(scanned.original_intent()).into();
    let header = restore_original_flow_policy_replace_frame_header(
        scanned, recomputed, limits, admission, charge,
    )?;
    let isolation = keys::scope([
        header.scope().tenant().as_bytes(),
        header.scope().group().as_bytes(),
        header.scope().recovery().as_bytes(),
    ])
    .map_err(map_codec_error)?;
    let digest = PreparedDigest::new(Sha256::digest(header.encoded()).into());
    Ok(VerifiedOriginalFlowPolicyReplaceFrame {
        header,
        digest,
        isolation,
    })
}
