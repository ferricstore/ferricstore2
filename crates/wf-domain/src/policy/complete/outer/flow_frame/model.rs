//! Supplied inputs, owner-free scans and independently owned request headers.

use crate::policy::complete::RawFlowPolicyValue;
use crate::workflow::request::{ClientIntent, RequestScope};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::CommittedTime;

use super::{
    CompletePolicyRequestError, Error, FlowPolicyFrameCursor, PolicySchemaCharge, add, check_scope,
    copy, visit,
};

/// Original lower allocator command; construction grants no admission or authority.
pub struct OriginalFlowPolicyReplaceInput<'input> {
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) stamp: CommittedTime,
    pub(super) policy_key: &'input [u8],
    pub(super) expected: RawFlowPolicyValue<'input>,
    pub(super) patch: RawFlowPolicyValue<'input>,
}

impl<'input> OriginalFlowPolicyReplaceInput<'input> {
    /// Moves caller identities and borrows the original key and represented terms.
    #[must_use]
    pub const fn new(
        scope: RequestScope,
        request: RequestId,
        stamp: CommittedTime,
        policy_key: &'input [u8],
        expected: RawFlowPolicyValue<'input>,
        patch: RawFlowPolicyValue<'input>,
    ) -> Self {
        Self {
            scope,
            request,
            stamp,
            policy_key,
            expected,
            patch,
        }
    }
}

/// Mathematical same-frame structure without hash or installation authority.
pub struct ScannedOriginalFlowPolicyReplaceFrame<'frame> {
    pub(super) encoded: &'frame [u8],
    pub(super) original_intent: &'frame [u8],
    pub(super) tenant: &'frame [u8],
    pub(super) group: &'frame [u8],
    pub(super) recovery: &'frame [u8],
    pub(super) request: &'frame [u8],
    pub(super) stamp: CommittedTime,
    pub(super) digest: [u8; 32],
    pub(super) policy_key: &'frame [u8],
    pub(super) expected: FlowPolicyFrameCursor<'frame>,
    pub(super) patch: FlowPolicyFrameCursor<'frame>,
}

impl<'frame> ScannedOriginalFlowPolicyReplaceFrame<'frame> {
    /// Borrows exactly the canonical intent range that requires fresh hashing.
    #[must_use]
    pub const fn original_intent(&self) -> &'frame [u8] {
        self.original_intent
    }
    pub(super) fn restore(
        self,
        recomputed: [u8; 32],
        charge: &mut PolicySchemaCharge,
    ) -> Result<OriginalFlowPolicyReplaceFrameHeader<'frame>, CompletePolicyRequestError> {
        visit(charge, 128)?;
        visit(charge, 128)?;
        let tenant = TenantId::new(copy(self.tenant, charge)?).map_err(|_| Error::Malformed)?;
        let group = GroupId::new(copy(self.group, charge)?).map_err(|_| Error::Malformed)?;
        let recovery =
            RecoveryId::new(copy(self.recovery, charge)?).map_err(|_| Error::Malformed)?;
        let request = RequestId::new(copy(self.request, charge)?).map_err(|_| Error::Malformed)?;
        let scope = RequestScope::new(tenant, group, recovery);
        visit(charge, 128)?;
        let length = check_scope(self.tenant, self.group, self.recovery)?;
        for base in [512, 128, 512] {
            visit(charge, add(base, length)?)?;
        }
        visit(charge, add(128, self.encoded.len())?)?;
        Ok(OriginalFlowPolicyReplaceFrameHeader {
            encoded: self.encoded,
            original_intent: self.original_intent,
            scope,
            request,
            stamp: self.stamp,
            intent: ClientIntent::original_flow_policy_replace(recomputed),
            policy_key: self.policy_key,
            expected: self.expected,
            patch: self.patch,
        })
    }
}

/// Owned namespace and borrowed canonical command, with no ports origin authority.
pub struct OriginalFlowPolicyReplaceFrameHeader<'frame> {
    pub(super) encoded: &'frame [u8],
    pub(super) original_intent: &'frame [u8],
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) stamp: CommittedTime,
    pub(super) intent: ClientIntent,
    pub(super) policy_key: &'frame [u8],
    pub(super) expected: FlowPolicyFrameCursor<'frame>,
    pub(super) patch: FlowPolicyFrameCursor<'frame>,
}

impl<'frame> OriginalFlowPolicyReplaceFrameHeader<'frame> {
    /// Borrows the complete original transport frame.
    #[must_use]
    pub const fn encoded(&self) -> &'frame [u8] {
        self.encoded
    }
    /// Borrows canonical mathematical intent, excluding source-order sidecar.
    #[must_use]
    pub const fn original_intent(&self) -> &'frame [u8] {
        self.original_intent
    }
    /// Borrows independent namespace owners.
    #[must_use]
    pub const fn scope(&self) -> &RequestScope {
        &self.scope
    }
    /// Borrows the idempotency identity.
    #[must_use]
    pub const fn request(&self) -> &RequestId {
        &self.request
    }
    /// Reads the supplied committed stamp.
    #[must_use]
    pub const fn stamp(&self) -> CommittedTime {
        self.stamp
    }
    /// Reads the independently compared registry-three intent.
    #[must_use]
    pub const fn intent(&self) -> ClientIntent {
        self.intent
    }
    pub(in crate::policy::complete) fn into_parts(
        self,
    ) -> (
        RequestScope,
        RequestId,
        &'frame [u8],
        FlowPolicyFrameCursor<'frame>,
        FlowPolicyFrameCursor<'frame>,
    ) {
        (
            self.scope,
            self.request,
            self.policy_key,
            self.expected,
            self.patch,
        )
    }
}
