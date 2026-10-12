//! Raw original Replace decoding with moved headers and presence-preserving Data.

use wf_domain::policy::complete::{
    CompletePolicyReplaceInput, CompletePolicyRequestError, PolicySchemaCharge, PolicySchemaLimits,
    decode_complete_policy_replace_frame_input,
};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::{EntryIdentity, PreparedDigest};
use wf_domain::workflow::request::ClientIntent;
use wf_domain::workflow::request::gate::CommittedHeader;
use wf_types::version::LogIdentity;

use super::{VerifiedCompletePolicyReplaceFrame, verify_complete_policy_replace_frame};

/// Original complete Data owners and borrowed canonical bytes.
///
/// This mathematical result grants no Fresh, current-policy or installation authority.
pub struct DecodedCompletePolicyReplaceFrame<'a> {
    input: CompletePolicyReplaceInput,
    encoded: &'a [u8],
    original_intent: &'a [u8],
    intent: ClientIntent,
    digest: PreparedDigest,
    isolation: Vec<u8>,
}

impl<'a> DecodedCompletePolicyReplaceFrame<'a> {
    /// Borrows reconstructed original Data and moved header identities.
    #[must_use]
    pub const fn input(&self) -> &CompletePolicyReplaceInput {
        &self.input
    }

    /// Borrows the same immutable complete frame.
    #[must_use]
    pub const fn encoded(&self) -> &'a [u8] {
        self.encoded
    }

    /// Borrows the exact original generation-free identity region.
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

    /// Joins supplied log identity to mathematical decoded content.
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

    /// Moves the original input; later consumers must freshly pay their work.
    #[must_use]
    pub fn into_input(self) -> CompletePolicyReplaceInput {
        self.input
    }
}

/// Decodes raw tag-six bytes through one continuous schema account.
///
/// Complete borrowed verification precedes original guard and Data reconstruction.
/// This codec inspects no retained requests, current policy or context.
///
/// # Errors
/// Preserves verification, profile, wire, work and fallible leaf-reserve refusals.
pub fn decode_complete_policy_replace_frame<'a>(
    encoded: &'a [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<DecodedCompletePolicyReplaceFrame<'a>, CompletePolicyRequestError> {
    let verified = verify_complete_policy_replace_frame(encoded, limits, admission, charge)?;
    consume_verified_frame(verified, limits, admission, charge)
}

pub(super) fn consume_verified_frame<'a>(
    verified: VerifiedCompletePolicyReplaceFrame<'a>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<DecodedCompletePolicyReplaceFrame<'a>, CompletePolicyRequestError> {
    let VerifiedCompletePolicyReplaceFrame {
        header,
        digest,
        isolation,
    } = verified;
    let encoded = header.encoded();
    let original_intent = header.original_intent();
    let intent = header.intent();
    let input = decode_complete_policy_replace_frame_input(header, limits, admission, charge)?;
    Ok(DecodedCompletePolicyReplaceFrame {
        input,
        encoded,
        original_intent,
        intent,
        digest,
        isolation,
    })
}
