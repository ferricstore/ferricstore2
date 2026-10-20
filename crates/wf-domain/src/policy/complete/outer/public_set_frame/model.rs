//! Supplied preparation, borrowed scans and independently owned namespace headers.

use super::{
    CompletePolicyRequestError, Error, PolicySchemaCharge, PolicySchemaLimits,
    PolicySetFrameCursor, add, check_scope, copy, peak, visit,
};
use crate::policy::complete::{OriginalPolicySetMapOrders, OriginalPolicySetRequest};
use crate::workflow::request::{ClientIntent, RequestScope};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::CommittedTime;

pub(in crate::policy::complete) type PolicySetFrameHeaderParts<'frame> = (
    RequestScope,
    RequestId,
    &'frame [u8],
    PolicySetFrameCursor<'frame>,
    Option<u64>,
    bool,
    Option<&'frame [u8]>,
);

/// Supplied public preparation and server capture facts, without origin authority.
pub struct OriginalPolicySetFrameInput<'request, 'input, 'capture> {
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) stamp: CommittedTime,
    pub(super) prepared: &'request OriginalPolicySetRequest<'input>,
    pub(super) orders: &'capture OriginalPolicySetMapOrders<'capture>,
}
impl<'request, 'input, 'capture> OriginalPolicySetFrameInput<'request, 'input, 'capture> {
    /// Moves namespace owners and borrows one already-prepared original request.
    #[must_use]
    pub const fn new(
        scope: RequestScope,
        request: RequestId,
        stamp: CommittedTime,
        prepared: &'request OriginalPolicySetRequest<'input>,
        orders: &'capture OriginalPolicySetMapOrders<'capture>,
    ) -> Self {
        Self {
            scope,
            request,
            stamp,
            prepared,
            orders,
        }
    }
    /// Reads the supplied capture identity, without certifying its origin.
    #[must_use]
    pub const fn patch_identity(&self) -> [u8; 32] {
        self.orders.patch_identity()
    }
}

/// Same-frame mathematical structure; no hash, paid credit or installation proof.
pub struct ScannedOriginalPolicySetFrame<'frame> {
    pub(super) encoded: &'frame [u8],
    pub(super) original_intent: &'frame [u8],
    pub(super) tenant: &'frame [u8],
    pub(super) group: &'frame [u8],
    pub(super) recovery: &'frame [u8],
    pub(super) request: &'frame [u8],
    pub(super) stamp: CommittedTime,
    pub(super) digest: [u8; 32],
    pub(super) policy_key: &'frame [u8],
    pub(super) patch: PolicySetFrameCursor<'frame>,
    pub(super) expected: Option<u64>,
    pub(super) replace: bool,
    pub(super) response_state: Option<&'frame [u8]>,
    pub(super) patch_bounds: (usize, usize),
}
impl<'frame> ScannedOriginalPolicySetFrame<'frame> {
    /// Borrows the exact original canonical intent range for fresh hashing.
    #[must_use]
    pub const fn original_intent(&self) -> &'frame [u8] {
        self.original_intent
    }
    /// Borrows the exact canonical patch range for fresh capture comparison.
    #[must_use]
    pub const fn canonical_patch(&self) -> &'frame [u8] {
        self.patch.node
    }
    pub(super) fn restore(
        self,
        digest: [u8; 32],
        backing: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<OriginalPolicySetFrameHeader<'frame>, CompletePolicyRequestError> {
        visit(charge, 128)?;
        visit(charge, 128)?;
        let mut live = add(self.encoded.len(), backing)?;
        live = add(live, add(128, self.tenant.len())?)?;
        peak(charge, live)?;
        let tenant = TenantId::new(copy(self.tenant, charge)?).map_err(|_| Error::Malformed)?;
        live = add(live, add(128, self.group.len())?)?;
        peak(charge, live)?;
        let group = GroupId::new(copy(self.group, charge)?).map_err(|_| Error::Malformed)?;
        live = add(live, add(128, self.recovery.len())?)?;
        peak(charge, live)?;
        let recovery =
            RecoveryId::new(copy(self.recovery, charge)?).map_err(|_| Error::Malformed)?;
        live = add(live, add(128, self.request.len())?)?;
        peak(charge, live)?;
        let request = RequestId::new(copy(self.request, charge)?).map_err(|_| Error::Malformed)?;
        let scope = RequestScope::new(tenant, group, recovery);
        visit(charge, 128)?;
        let length = check_scope(self.tenant, self.group, self.recovery)?;
        visit(charge, add(512, length)?)?;
        peak(charge, add(live, length)?)?;
        visit(charge, add(128, length)?)?;
        visit(charge, add(512, length)?)?;
        visit(charge, add(128, self.encoded.len())?)?;
        Ok(OriginalPolicySetFrameHeader {
            encoded: self.encoded,
            original_intent: self.original_intent,
            scope,
            request,
            stamp: self.stamp,
            intent: ClientIntent::original_policy_set(digest),
            policy_key: self.policy_key,
            patch: self.patch,
            expected: self.expected,
            replace: self.replace,
            response_state: self.response_state,
            patch_bounds: self.patch_bounds,
            patch_backing: backing,
        })
    }
}

/// Independent namespace owners and a borrowed canonical original public command.
pub struct OriginalPolicySetFrameHeader<'frame> {
    encoded: &'frame [u8],
    original_intent: &'frame [u8],
    scope: RequestScope,
    request: RequestId,
    stamp: CommittedTime,
    intent: ClientIntent,
    policy_key: &'frame [u8],
    patch: PolicySetFrameCursor<'frame>,
    expected: Option<u64>,
    replace: bool,
    response_state: Option<&'frame [u8]>,
    patch_bounds: (usize, usize),
    patch_backing: usize,
}
impl<'frame> OriginalPolicySetFrameHeader<'frame> {
    /// Borrows the complete checked frame.
    #[must_use]
    pub const fn encoded(&self) -> &'frame [u8] {
        self.encoded
    }
    /// Borrows canonical client intent, excluding source-order sidecar.
    #[must_use]
    pub const fn original_intent(&self) -> &'frame [u8] {
        self.original_intent
    }
    /// Borrows independently owned namespace identities.
    #[must_use]
    pub const fn scope(&self) -> &RequestScope {
        &self.scope
    }
    /// Borrows the independently owned request identity.
    #[must_use]
    pub const fn request(&self) -> &RequestId {
        &self.request
    }
    /// Reads the supplied committed stamp.
    #[must_use]
    pub const fn stamp(&self) -> CommittedTime {
        self.stamp
    }
    /// Reads the registry-four canonical intent.
    #[must_use]
    pub const fn intent(&self) -> ClientIntent {
        self.intent
    }
    /// Reads the original public replacement mode.
    #[must_use]
    pub const fn replace(&self) -> bool {
        self.replace
    }
    /// Borrows the original reply-state selector, preserving None versus empty.
    #[must_use]
    pub const fn response_state(&self) -> Option<&'frame [u8]> {
        self.response_state
    }
    pub(in crate::policy::complete) const fn patch_bounds(&self) -> (usize, usize) {
        self.patch_bounds
    }
    /// Borrows the exact original policy key from the committed frame.
    #[must_use]
    pub const fn policy_key(&self) -> &'frame [u8] {
        self.policy_key
    }

    /// Copies the original requested type on the same freshly checked account.
    /// # Errors
    /// Preserves profile, invalid key authority, work and allocation refusals.
    pub fn copy_workflow_type(
        &self,
        limits: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Vec<u8>, CompletePolicyRequestError> {
        super::profile(limits, charge)?;
        let requested = self
            .policy_key
            .strip_prefix(b"f:{f}:policy:")
            .filter(|s| !s.is_empty())
            .filter(|_| self.policy_key.len() <= 65_535)
            .ok_or_else(super::authority)?;
        let mut live = add(self.encoded.len(), self.patch_backing)?;
        for bytes in [
            self.scope.tenant().as_bytes(),
            self.scope.group().as_bytes(),
            self.scope.recovery().as_bytes(),
            self.request.as_bytes(),
        ] {
            live = add(live, add(128, bytes.len())?)?;
        }
        peak(charge, add(live, add(128, requested.len())?)?)?;
        Ok(copy(requested, charge)?)
    }

    pub(in crate::policy::complete) const fn patch_backing(&self) -> usize {
        self.patch_backing
    }
    pub(in crate::policy::complete) fn into_parts(self) -> PolicySetFrameHeaderParts<'frame> {
        (
            self.scope,
            self.request,
            self.policy_key,
            self.patch,
            self.expected,
            self.replace,
            self.response_state,
        )
    }
}

pub(super) fn write_header(
    input: &OriginalPolicySetFrameInput<'_, '_, '_>,
    ilen: usize,
    slen: usize,
    sink: &mut super::codec::PolicySetFrameSink<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, Error> {
    visit(charge, 128)?;
    sink.write(b"FWCP", charge)?;
    sink.write(&1_u16.to_be_bytes(), charge)?;
    sink.write(&[9], charge)?;
    for bytes in [
        input.scope.tenant().as_bytes(),
        input.scope.group().as_bytes(),
        input.scope.recovery().as_bytes(),
        input.request.as_bytes(),
    ] {
        sink.raw(bytes, charge)?;
    }
    sink.write(&4_u16.to_be_bytes(), charge)?;
    let digest = sink.position();
    sink.write(&[0; 32], charge)?;
    sink.write(&input.stamp.hlc().physical().get().to_be_bytes(), charge)?;
    sink.write(&input.stamp.hlc().logical().to_be_bytes(), charge)?;
    sink.write(&input.stamp.wall().get().to_be_bytes(), charge)?;
    sink.u32(ilen, charge)?;
    sink.u32(slen, charge)?;
    Ok(digest)
}
