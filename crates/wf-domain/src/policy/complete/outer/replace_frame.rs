//! Borrowed original Replace admission and paid mathematical header restoration.

use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::CommittedTime;

use crate::workflow::admission::AdmissionLimits;
use crate::workflow::request::{ClientIntent, RequestScope};

use super::super::admission::charge_schema;
use super::super::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};
use super::CompletePolicyRequestError;

mod decode;
mod scan;

pub(super) use scan::check_replace_frame_lists;

/// Immutable structure from one original frame; carries no account or paid credit.
pub struct ScannedCompletePolicyReplaceFrame<'a> {
    encoded: &'a [u8],
    original_intent: &'a [u8],
    tenant: &'a [u8],
    group: &'a [u8],
    recovery: &'a [u8],
    request: &'a [u8],
    stamp: CommittedTime,
    intent_digest: [u8; 32],
}

impl<'a> ScannedCompletePolicyReplaceFrame<'a> {
    /// Borrows the exact original request identity bytes for recomputation.
    #[must_use]
    pub const fn original_intent(&self) -> &'a [u8] {
        self.original_intent
    }

    fn restore_header(
        self,
        charge: &mut PolicySchemaCharge,
    ) -> Result<CompletePolicyReplaceFrameHeader<'a>, CompletePolicyRequestError> {
        let ScannedCompletePolicyReplaceFrame {
            encoded,
            original_intent,
            tenant,
            group,
            recovery,
            request,
            stamp,
            intent_digest,
        } = self;
        charge_schema(charge, 128, 0)?;
        charge_schema(charge, 128, 0)?;
        let tenant = TenantId::new(copy_replace_header_field(tenant, charge)?)
            .or(Err(CompletePolicyEncodingError::Malformed))?;
        let group = GroupId::new(copy_replace_header_field(group, charge)?)
            .or(Err(CompletePolicyEncodingError::Malformed))?;
        let recovery = RecoveryId::new(copy_replace_header_field(recovery, charge)?)
            .or(Err(CompletePolicyEncodingError::Malformed))?;
        let scope = RequestScope::new(tenant, group, recovery);
        let request = RequestId::new(copy_replace_header_field(request, charge)?)
            .or(Err(CompletePolicyEncodingError::Malformed))?;
        Ok(CompletePolicyReplaceFrameHeader {
            encoded,
            original_intent,
            scope,
            request,
            stamp,
            intent: ClientIntent::complete_policy_replace(intent_digest),
        })
    }
}

/// Owned identities and borrowed original bytes, without prepared-frame authority.
pub struct CompletePolicyReplaceFrameHeader<'a> {
    encoded: &'a [u8],
    original_intent: &'a [u8],
    scope: RequestScope,
    request: RequestId,
    stamp: CommittedTime,
    intent: ClientIntent,
}

impl<'a> CompletePolicyReplaceFrameHeader<'a> {
    /// Borrows the same complete original frame.
    #[must_use]
    pub const fn encoded(&self) -> &'a [u8] {
        self.encoded
    }

    /// Borrows the original request, without materializing Data.
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
pub fn scan_complete_policy_replace_frame<'a>(
    encoded: &'a [u8],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<ScannedCompletePolicyReplaceFrame<'a>, CompletePolicyRequestError> {
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
pub fn restore_complete_policy_replace_frame_header<'a>(
    scanned: ScannedCompletePolicyReplaceFrame<'a>,
    recomputed: [u8; 32],
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyReplaceFrameHeader<'a>, CompletePolicyRequestError> {
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
    prepay_replace_frame_tail(&header, charge)?;
    Ok(header)
}

fn copy_replace_header_field(
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

fn prepay_replace_frame_tail(
    header: &CompletePolicyReplaceFrameHeader<'_>,
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

/// Consumes a mathematical header and freshly reconstructs original guard/Data.
///
/// Existing namespace and request owners move into the returned input. This
/// operation confers no ports hash, retained-request or installation authority.
///
/// # Errors
/// Rechecks profile/frame caps, then preserves original read/copy/work refusals.
pub fn decode_complete_policy_replace_frame_input(
    header: CompletePolicyReplaceFrameHeader<'_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<super::CompletePolicyReplaceInput, CompletePolicyRequestError> {
    decode::decode_input(header, limits, admission, charge)
}
