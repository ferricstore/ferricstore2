//! Borrowed mathematical admission and paid header restoration for tag-five frames.

use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::CommittedTime;

use crate::workflow::admission::AdmissionLimits;
use crate::workflow::request::{ClientIntent, RequestScope};

use super::super::admission::charge_schema;
use super::super::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};
use super::{CompleteCorePolicyPatchRequestInput, CompletePolicyRequestError};

mod decode;
mod scan;

/// Immutable structure from one original frame; carries no account or paid credit.
pub struct ScannedCorePatchFrame<'a> {
    encoded: &'a [u8],
    original_intent: &'a [u8],
    tenant: &'a [u8],
    group: &'a [u8],
    recovery: &'a [u8],
    request: &'a [u8],
    stamp: CommittedTime,
    intent_digest: [u8; 32],
    state_changes: usize,
}

impl<'a> ScannedCorePatchFrame<'a> {
    /// Computes the prospective owning-command bound from reached structure.
    /// # Errors
    /// Refuses checked arithmetic overflow; conveys no reusable paid credit.
    pub fn decode_bound(&self) -> Result<usize, CompletePolicyRequestError> {
        self.encoded
            .len()
            .checked_mul(4)
            .and_then(|n| {
                self.state_changes
                    .checked_mul(2048)
                    .and_then(|m| n.checked_add(m))
            })
            .and_then(|n| n.checked_add(4096))
            .ok_or_else(|| PolicySchemaError::LengthOverflow.into())
    }

    /// Borrows the exact original partial-command identity bytes for recomputation.
    #[must_use]
    pub const fn original_intent(&self) -> &'a [u8] {
        self.original_intent
    }

    fn restore_header(
        self,
        charge: &mut PolicySchemaCharge,
    ) -> Result<CompleteCorePolicyPatchFrameHeader<'a>, CompletePolicyRequestError> {
        let ScannedCorePatchFrame {
            encoded,
            original_intent,
            tenant,
            group,
            recovery,
            request,
            stamp,
            intent_digest,
            state_changes: _,
        } = self;
        charge_schema(charge, 128, 0)?;
        charge_schema(charge, 128, 0)?;
        let tenant = TenantId::new(copy_header_field(tenant, charge)?)
            .or(Err(CompletePolicyEncodingError::Malformed))?;
        let group = GroupId::new(copy_header_field(group, charge)?)
            .or(Err(CompletePolicyEncodingError::Malformed))?;
        let recovery = RecoveryId::new(copy_header_field(recovery, charge)?)
            .or(Err(CompletePolicyEncodingError::Malformed))?;
        let scope = RequestScope::new(tenant, group, recovery);
        let request = RequestId::new(copy_header_field(request, charge)?)
            .or(Err(CompletePolicyEncodingError::Malformed))?;
        Ok(CompleteCorePolicyPatchFrameHeader {
            encoded,
            original_intent,
            scope,
            request,
            stamp,
            intent: ClientIntent::complete_core_policy_patch(intent_digest),
        })
    }
}

/// Owned identities and borrowed original bytes, without prepared-frame authority.
pub struct CompleteCorePolicyPatchFrameHeader<'a> {
    encoded: &'a [u8],
    original_intent: &'a [u8],
    scope: RequestScope,
    request: RequestId,
    stamp: CommittedTime,
    intent: ClientIntent,
}

impl<'a> CompleteCorePolicyPatchFrameHeader<'a> {
    /// Borrows the same complete original frame.
    #[must_use]
    pub const fn encoded(&self) -> &'a [u8] {
        self.encoded
    }

    /// Borrows the original partial command, without materializing a Patch.
    #[must_use]
    pub const fn original_intent(&self) -> &'a [u8] {
        self.original_intent
    }

    /// Borrows independently owned validated namespace identities.
    #[must_use]
    pub const fn scope(&self) -> &RequestScope {
        &self.scope
    }

    /// Borrows the original caller request identity.
    #[must_use]
    pub const fn request(&self) -> &RequestId {
        &self.request
    }

    /// Reads the structurally validated committed stamp.
    #[must_use]
    pub const fn stamp(&self) -> CommittedTime {
        self.stamp
    }

    /// Reads the supplied intent after its paid recomputed-digest comparison.
    #[must_use]
    pub const fn intent(&self) -> ClientIntent {
        self.intent
    }
}

/// Scans one complete original frame through the received schema account.
///
/// The result is mathematical structure, with no hash computation or paid credit.
/// Structural scope admission precedes intent recomputation and all owned headers.
///
/// # Errors
/// Preserves profile, frame, scope, wire, work and checked-length refusals.
pub fn scan_complete_core_policy_patch_frame<'a>(
    encoded: &'a [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<ScannedCorePatchFrame<'a>, CompletePolicyRequestError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch.into());
    }
    if encoded.len() > admission.decision_bytes() {
        return Err(CompletePolicyEncodingError::ByteLimit.into());
    }
    let (scanned, inner_tenant) = scan::scan_frame(encoded, limits, charge)?;
    let comparison = scanned
        .tenant
        .len()
        .checked_add(inner_tenant.len())
        .ok_or(PolicySchemaError::LengthOverflow)?;
    let comparison = comparison
        .checked_add(128)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    let comparison = u64::try_from(comparison).or(Err(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, comparison, 0)?;
    if scanned.tenant != inner_tenant {
        return Err(CompletePolicyEncodingError::Malformed.into());
    }
    let bytes =
        u64::try_from(scanned.original_intent.len()).or(Err(PolicySchemaError::LengthOverflow))?;
    let hash = bytes
        .checked_add(128)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, hash, 0)?;
    Ok(scanned)
}

/// Consumes one same-frame view after fresh comparison and header/tail admissions.
///
/// No grammar scan is repeated. This header cannot mint a ports Verified value.
/// Public calls do not establish account identity; the closed raw ports entry
/// threads its actual account through every scan, comparison, copy and tail.
///
/// # Errors
/// Rechecks profile/frame caps, then preserves digest, copy and work refusals.
pub fn restore_complete_core_policy_patch_frame_header<'a>(
    scanned: ScannedCorePatchFrame<'a>,
    recomputed: [u8; 32],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchFrameHeader<'a>, CompletePolicyRequestError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch.into());
    }
    if scanned.encoded.len() > admission.decision_bytes() {
        return Err(CompletePolicyEncodingError::ByteLimit.into());
    }
    charge_schema(charge, 192, 0)?;
    if recomputed != scanned.intent_digest {
        return Err(CompletePolicyRequestError::IntentMismatch);
    }
    let header = scanned.restore_header(charge)?;
    prepare_tail(&header, charge)?;
    Ok(header)
}

fn copy_header_field(
    bytes: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, CompletePolicyRequestError> {
    let bytes_len = u64::try_from(bytes.len()).or(Err(PolicySchemaError::LengthOverflow))?;
    let work = bytes_len
        .checked_add(128)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, work, 0)?;
    let mut copied = Vec::new();
    copied
        .try_reserve_exact(bytes.len())
        .or(Err(CompletePolicyEncodingError::AllocationFailure))?;
    copied.extend_from_slice(bytes);
    Ok(copied)
}

fn prepare_tail(
    header: &CompleteCorePolicyPatchFrameHeader<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), CompletePolicyRequestError> {
    charge_schema(charge, 128, 0)?;
    let isolation = header
        .scope
        .tenant()
        .as_bytes()
        .len()
        .checked_add(header.scope.group().as_bytes().len())
        .ok_or(CompletePolicyRequestError::ScopeLimit)?;
    let isolation = isolation
        .checked_add(header.scope.recovery().as_bytes().len())
        .ok_or(CompletePolicyRequestError::ScopeLimit)?;
    let isolation = isolation
        .checked_add(18)
        .ok_or(CompletePolicyRequestError::ScopeLimit)?;
    if isolation > 8_192 {
        return Err(CompletePolicyRequestError::ScopeLimit);
    }
    let bytes = u64::try_from(isolation).or(Err(PolicySchemaError::LengthOverflow))?;
    // Distinct scope measure, reserve, and emission occurrences.
    for base in [512, 128, 512] {
        let work = bytes
            .checked_add(base)
            .ok_or(PolicySchemaError::LengthOverflow)?;
        charge_schema(charge, work, 0)?;
    }
    let frame_len =
        u64::try_from(header.encoded.len()).or(Err(PolicySchemaError::LengthOverflow))?;
    let frame = frame_len
        .checked_add(128)
        .ok_or(PolicySchemaError::LengthOverflow)?;
    charge_schema(charge, frame, 0)?;
    Ok(())
}

/// Consumes same-frame mathematical header content into the original raw command.
///
/// Immutable header correspondence is retained while every reconstruction read,
/// owning node and selected leaf reserve/copy freshly pays the received account.
/// No retained request, current value, default or install authority is supplied.
///
/// # Errors
/// Rechecks profile/frame caps, then preserves wire, work and leaf-reserve refusals.
pub fn decode_complete_core_policy_patch_frame_input(
    header: CompleteCorePolicyPatchFrameHeader<'_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompleteCorePolicyPatchRequestInput, CompletePolicyRequestError> {
    decode::decode_input(header, limits, admission, charge)
}

/// Prices a real additional borrowed grammar walk before original Patch ownership.
/// # Errors
/// Preserves frame, profile, cumulative work and checked-size refusals.
pub fn complete_core_policy_patch_frame_decode_bound(
    encoded: &[u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<usize, CompletePolicyRequestError> {
    scan_complete_core_policy_patch_frame(encoded, limits, admission, charge)?.decode_bound()
}
