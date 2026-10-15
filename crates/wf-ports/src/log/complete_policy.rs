//! Private Complete Replace and core Patch caller intent preparation.

use wf_domain::policy::complete::{
    CompleteCorePolicyPatchCaller, CompleteCorePolicyPatchRequestInput,
    CompletePolicyReplaceCaller, CompletePolicyReplaceInput, CompletePolicyRequestError,
    PolicySchemaCharge, PolicySchemaLimits, prepare_complete_core_policy_patch_caller,
    prepare_complete_policy_replace_caller,
};
use wf_domain::workflow::admission::AdmissionLimits;

mod core_patch;
mod intent;
mod original_entry;
mod replace;

pub use original_entry::{OriginalPolicyReference, prepare_original_complete_policy_reference};

pub use replace::{
    CompletePolicyReplaceRequestPlan, decide_complete_policy_replace_frame,
    decide_original_complete_policy_replace_frame,
};
pub use replace::{
    DecodedCompletePolicyReplaceFrame, VerifiedCompletePolicyReplaceFrame,
    decode_complete_policy_replace_frame, prepare_complete_policy_replace_frame,
    verify_complete_policy_replace_frame,
};

pub use core_patch::{CompleteCorePolicyPatchRequestPlan, decide_complete_core_policy_patch_frame};
pub use core_patch::{
    DecodedCompleteCorePolicyPatchFrame, VerifiedCompleteCorePolicyPatchFrame,
    decode_complete_core_policy_patch_frame, prepare_complete_core_policy_patch_frame,
    verify_complete_core_policy_patch_frame,
};

/// Original admitted caller owners and the independently prepared intent digest.
pub struct CompletePolicyReplaceIntent {
    caller: CompletePolicyReplaceCaller,
    digest: [u8; 32],
}

impl CompletePolicyReplaceIntent {
    /// Borrows the admitted original caller with its immutable profiles.
    #[must_use]
    pub const fn caller(&self) -> &CompletePolicyReplaceCaller {
        &self.caller
    }

    /// Borrows the fixed digest; no public canonical encoder or constructor is supplied.
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

/// Prepares a caller-bound intent under its original continuous schema account.
///
/// # Errors
/// Rejects caller admission, canonical size, or incremental hashing preconditions.
pub fn prepare_complete_policy_replace_intent(
    input: CompletePolicyReplaceInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyReplaceIntent, CompletePolicyRequestError> {
    let caller = prepare_complete_policy_replace_caller(input, limits, admission, charge)?;
    let digest = intent::hash_intent(&caller)?;
    Ok(CompletePolicyReplaceIntent { caller, digest })
}

/// Original partial-command caller and its fixed intent digest.
///
/// Retains the original partial command rather than a merged policy value.
pub struct CompleteCorePolicyPatchIntent {
    caller: CompleteCorePolicyPatchCaller,
    digest: [u8; 32],
}

impl CompleteCorePolicyPatchIntent {
    /// Borrows the original caller and its immutable profiles.
    #[must_use]
    pub const fn caller(&self) -> &CompleteCorePolicyPatchCaller {
        &self.caller
    }

    /// Borrows the fixed digest without granting hashing or encoding authority.
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

/// Prepares original partial-command identity on its continuous schema account.
///
/// Freshly admits the supplied input before privately hashing its original fields.
///
/// # Errors
/// Preserves caller admission, canonical size, and incremental hashing refusals.
pub fn prepare_complete_core_policy_patch_intent(
    input: CompleteCorePolicyPatchRequestInput,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchIntent, CompletePolicyRequestError> {
    let caller = prepare_complete_core_policy_patch_caller(input, limits, admission, charge)?;
    let digest = core_patch::hash_core_patch_intent(&caller)?;
    Ok(CompleteCorePolicyPatchIntent { caller, digest })
}
