//! Private owners and public borrowed access for Complete policy callers.

use crate::workflow::admission::AdmissionLimits;
use crate::workflow::request::RequestScope;
use wf_types::identity::RequestId;
use wf_types::version::CommittedTime;

use super::super::{CompleteCorePolicyPatchInput, CompletePolicyData, PolicySchemaLimits};

/// Original caller owners before bounded admission.
///
/// Construction and destruction are outside the logical schema work account.
pub struct CompletePolicyReplaceInput {
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) stamp: CommittedTime,
    pub(super) expected_generation: Option<u64>,
    pub(super) data: CompletePolicyData,
}

impl CompletePolicyReplaceInput {
    /// Takes the supplied owners without validation, cloning, or allocating a generation.
    #[must_use]
    pub const fn new(
        scope: RequestScope,
        request: RequestId,
        stamp: CommittedTime,
        expected_generation: Option<u64>,
        data: CompletePolicyData,
    ) -> Self {
        Self {
            scope,
            request,
            stamp,
            expected_generation,
            data,
        }
    }

    /// Borrows the original namespace owners.
    #[must_use]
    pub const fn scope(&self) -> &RequestScope {
        &self.scope
    }

    /// Borrows the supplied idempotency request identity.
    #[must_use]
    pub const fn request(&self) -> &RequestId {
        &self.request
    }

    /// Reads the supplied committed time without allocating log identity.
    #[must_use]
    pub const fn stamp(&self) -> CommittedTime {
        self.stamp
    }

    /// Reads the caller's optional generation guard, including a supplied zero.
    #[must_use]
    pub const fn expected_generation(&self) -> Option<u64> {
        self.expected_generation
    }

    /// Borrows the original complete normalized content.
    #[must_use]
    pub const fn data(&self) -> &CompletePolicyData {
        &self.data
    }

    /// Moves every original owner without admission or reusable paid credit.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        RequestScope,
        RequestId,
        CommittedTime,
        Option<u64>,
        CompletePolicyData,
    ) {
        (
            self.scope,
            self.request,
            self.stamp,
            self.expected_generation,
            self.data,
        )
    }
}

/// Owned caller with immutable profiles selected by bounded outer admission.
pub struct CompletePolicyReplaceCaller {
    pub(super) input: CompletePolicyReplaceInput,
    pub(super) schema_limits: PolicySchemaLimits,
    pub(super) admission_limits: AdmissionLimits,
}

impl CompletePolicyReplaceCaller {
    /// Borrows the original caller owners and supplied request values.
    #[must_use]
    pub const fn input(&self) -> &CompletePolicyReplaceInput {
        &self.input
    }

    /// Reads the selected immutable schema profile.
    #[must_use]
    pub const fn schema_limits(&self) -> PolicySchemaLimits {
        self.schema_limits
    }

    /// Reads the selected immutable outer admission profile.
    #[must_use]
    pub const fn admission_limits(&self) -> AdmissionLimits {
        self.admission_limits
    }
}

/// Original normalized partial command and supplied caller owners.
///
/// Construction and destruction are outside logical schema work.
pub struct CompleteCorePolicyPatchRequestInput {
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) stamp: CommittedTime,
    pub(super) command: CompleteCorePolicyPatchInput,
}

impl CompleteCorePolicyPatchRequestInput {
    /// Moves the original command and supplied header without admission.
    #[must_use]
    pub const fn new(
        scope: RequestScope,
        request: RequestId,
        stamp: CommittedTime,
        command: CompleteCorePolicyPatchInput,
    ) -> Self {
        Self {
            scope,
            request,
            stamp,
            command,
        }
    }

    /// Borrows the original namespace owners.
    #[must_use]
    pub const fn scope(&self) -> &RequestScope {
        &self.scope
    }

    /// Borrows the supplied idempotency request identity.
    #[must_use]
    pub const fn request(&self) -> &RequestId {
        &self.request
    }

    /// Reads the supplied committed time.
    #[must_use]
    pub const fn stamp(&self) -> CommittedTime {
        self.stamp
    }

    /// Borrows the original normalized partial command.
    #[must_use]
    pub const fn command(&self) -> &CompleteCorePolicyPatchInput {
        &self.command
    }

    /// Moves all raw owners without admission, credit or installation authority.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        RequestScope,
        RequestId,
        CommittedTime,
        CompleteCorePolicyPatchInput,
    ) {
        (self.scope, self.request, self.stamp, self.command)
    }
}

/// Original partial-command caller with immutable selected profiles.
///
/// Retains the original partial command for later current-policy remerge.
pub struct CompleteCorePolicyPatchCaller {
    pub(super) input: CompleteCorePolicyPatchRequestInput,
    pub(super) schema_limits: PolicySchemaLimits,
    pub(super) admission_limits: AdmissionLimits,
}

impl CompleteCorePolicyPatchCaller {
    /// Borrows the original caller owners and supplied request values.
    #[must_use]
    pub const fn input(&self) -> &CompleteCorePolicyPatchRequestInput {
        &self.input
    }

    /// Reads the selected immutable schema profile.
    #[must_use]
    pub const fn schema_limits(&self) -> PolicySchemaLimits {
        self.schema_limits
    }

    /// Reads the selected immutable outer admission profile.
    #[must_use]
    pub const fn admission_limits(&self) -> AdmissionLimits {
        self.admission_limits
    }
}
