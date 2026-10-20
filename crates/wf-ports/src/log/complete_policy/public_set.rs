//! Both-mode public set transport hashing and one freshly paid verification.

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyRequestError, OriginalPolicySetFrameHeader,
    OriginalPolicySetFrameInput, OriginalPolicySetMapOrders, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, prepare_original_policy_set_frame_bytes,
    restore_original_policy_set_frame_header, scan_original_policy_set_frame,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, PreparedDigest};
use wf_domain::workflow::request::gate::CommittedHeader;
use wf_types::version::LogIdentity;

use super::super::{PreparedBytes, keys};
use super::intent::map_codec_error;

mod apply;
pub(super) mod response;

pub use apply::{
    FreshOriginalPolicySetFrame, OriginalPolicySetPointInspection, OriginalPolicySetRequestPlan,
    decide_original_policy_set_frame, decide_original_policy_set_frame_bounded,
    decide_original_policy_set_point_fresh_bounded,
    inspect_original_policy_set_point_frame_bounded,
    prepare_original_policy_set_point_frame_bounded,
};
pub use response::{original_policy_set_response_matches, original_policy_set_response_status};

/// Verified original bytes and independently restored mathematical header owners.
///
/// This result grants no retained-request, ordered-apply or installation authority.
pub struct VerifiedOriginalPolicySetFrame<'frame> {
    header: OriginalPolicySetFrameHeader<'frame>,
    digest: PreparedDigest,
    isolation: Vec<u8>,
}

impl<'frame> VerifiedOriginalPolicySetFrame<'frame> {
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

    /// Borrows the original policy key without copying or granting read authority.
    #[must_use]
    pub fn policy_key(&self) -> &[u8] {
        self.header.policy_key()
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

/// Prepares the public patch, mode, reply selector and canonical frame identity.
///
/// The domain prepays Patch SHA, intent SHA, overwrite, whole SHA and final
/// comparison on this account. Ports performs that exact order on the final owner.
///
/// # Errors
/// Preserves profile, scope, represented-class, bounds, work and allocation errors.
pub fn prepare_original_policy_set_frame(
    input: OriginalPolicySetFrameInput<'_, '_, '_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<PreparedBytes, CompletePolicyRequestError> {
    let expected_patch = input.patch_identity();
    let (mut bytes, digest_offset, intent_range, patch_range) =
        prepare_original_policy_set_frame_bytes(input, limits, admission, charge)?;
    let patch = bytes
        .get(patch_range)
        .ok_or(CompletePolicyEncodingError::EncodingMismatch)?;
    let patch_digest: [u8; 32] = Sha256::digest(patch).into();
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
    if patch_digest != expected_patch {
        return Err(PolicySchemaError::InvalidAuthority.into());
    }
    Ok(PreparedBytes { bytes, digest })
}

/// Verifies one exact original frame without constructing an adapter tree.
///
/// The scan and restoration prospectively pay each reached hash and owner on the
/// supplied continuous account. Current policies and the request gate are unread.
///
/// # Errors
/// Preserves structural, profile, work, intent and fallible owner refusals.
pub fn verify_original_policy_set_frame<'frame>(
    encoded: &'frame [u8],
    orders: &OriginalPolicySetMapOrders<'_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<VerifiedOriginalPolicySetFrame<'frame>, CompletePolicyRequestError> {
    let scanned = scan_original_policy_set_frame(encoded, limits, admission, charge)?;
    let recomputed = Sha256::digest(scanned.original_intent()).into();
    let patch = Sha256::digest(scanned.canonical_patch()).into();
    let header = restore_original_policy_set_frame_header(
        scanned, recomputed, patch, orders, limits, admission, charge,
    )?;
    let isolation = keys::scope([
        header.scope().tenant().as_bytes(),
        header.scope().group().as_bytes(),
        header.scope().recovery().as_bytes(),
    ])
    .map_err(map_codec_error)?;
    let digest = PreparedDigest::new(Sha256::digest(header.encoded()).into());
    Ok(VerifiedOriginalPolicySetFrame {
        header,
        digest,
        isolation,
    })
}
