//! Raw original-frame decoding with moved headers and presence-preserving owners.

use wf_domain::policy::complete::{
    CompleteCorePolicyPatchRequestInput, CompletePolicyRequestError, PolicySchemaCharge,
    PolicySchemaLimits, decode_complete_core_policy_patch_frame_input,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, PreparedDigest};
use wf_domain::workflow::request::ClientIntent;
use wf_domain::workflow::request::gate::CommittedHeader;
use wf_types::version::LogIdentity;

use super::{VerifiedCompleteCorePolicyPatchFrame, verify_frame};

/// Original partial-command owners and borrowed canonical bytes.
///
/// This mathematical codec result grants no Fresh, current-policy or install authority.
pub struct DecodedCompleteCorePolicyPatchFrame<'a> {
    input: CompleteCorePolicyPatchRequestInput,
    encoded: &'a [u8],
    original_intent: &'a [u8],
    intent: ClientIntent,
    digest: PreparedDigest,
    isolation: Vec<u8>,
}

impl<'a> DecodedCompleteCorePolicyPatchFrame<'a> {
    /// Borrows the reconstructed original command and moved header identities.
    #[must_use]
    pub const fn input(&self) -> &CompleteCorePolicyPatchRequestInput {
        &self.input
    }

    /// Borrows the same immutable complete frame.
    #[must_use]
    pub const fn encoded(&self) -> &'a [u8] {
        self.encoded
    }

    /// Borrows the exact original partial-command identity region.
    #[must_use]
    pub const fn original_intent(&self) -> &'a [u8] {
        self.original_intent
    }

    /// Reads the independently recomputed complete-frame digest.
    #[must_use]
    pub const fn digest(&self) -> PreparedDigest {
        self.digest
    }

    /// Borrows the moved canonical isolation owner.
    #[must_use]
    pub fn isolation_scope(&self) -> &[u8] {
        &self.isolation
    }

    /// Joins supplied log identity to mathematical decoded frame content.
    #[must_use]
    pub fn header(&self, log: LogIdentity) -> CommittedHeader<'_> {
        CommittedHeader {
            format_version: 1,
            scope: self.input.scope(),
            request: self.input.request(),
            intent: self.intent,
            identity: EntryIdentity::new(log, self.digest),
            stamp: self.input.stamp(),
            encoded_entry: self.encoded,
            encoded_isolation_scope: &self.isolation,
        }
    }

    /// Moves raw reconstructed input; later consumers must freshly pay their work.
    #[must_use]
    pub fn into_input(self) -> CompleteCorePolicyPatchRequestInput {
        self.input
    }
}

/// Decodes raw tag-five bytes through one continuous schema account.
///
/// Complete borrowed verification precedes original partial-command reconstruction.
/// This codec does not inspect retained requests or current policy state.
///
/// # Errors
/// Preserves verification, profile, wire, work and selected leaf-reserve refusals.
pub fn decode_complete_core_policy_patch_frame<'a>(
    encoded: &'a [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<DecodedCompleteCorePolicyPatchFrame<'a>, CompletePolicyRequestError> {
    let verified = verify_frame(encoded, limits, admission, charge)?;
    consume_verified_frame(verified, limits, admission, charge)
}

pub(super) fn consume_verified_frame<'a>(
    verified: VerifiedCompleteCorePolicyPatchFrame<'a>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<DecodedCompleteCorePolicyPatchFrame<'a>, CompletePolicyRequestError> {
    let VerifiedCompleteCorePolicyPatchFrame {
        header,
        digest,
        isolation,
    } = verified;
    let encoded = header.encoded();
    let original_intent = header.original_intent();
    let intent = header.intent();
    let input = decode_complete_core_policy_patch_frame_input(header, limits, admission, charge)?;
    Ok(DecodedCompleteCorePolicyPatchFrame {
        input,
        encoded,
        original_intent,
        intent,
        digest,
        isolation,
    })
}
