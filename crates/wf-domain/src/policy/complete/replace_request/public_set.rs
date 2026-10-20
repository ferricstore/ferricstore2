//! Strict current, source merge, one business walk and late original emission.

use super::flow::OriginalFlowPolicyInstall;
use super::original::{bind_original_current_type, read_original_current_bytes};
use super::{
    CompletePolicyReplaceApplyError as ExistingError,
    OriginalCompletePolicyReplaceApplyError as RawError, OriginalCompletePolicyReplaceApplyView,
};
use crate::policy::complete::flow_input::value::{
    PolicyInputCursor as Cursor, PolicyInputKind as Kind,
};
use crate::policy::complete::original_entry::{
    decode_retained, indexed::OriginalPolicyCurrentIndex,
};
use crate::policy::complete::public_set::{
    OriginalPolicySetMapOrders,
    merge::{PolicyMergeArena, peak_at},
};
use crate::policy::complete::{
    CompletePolicyEncodingError as Error, FlowPolicyInputRejection, OriginalPolicyReadError,
    OriginalPolicySetFrameHeader, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    admission::charge_schema,
};
use crate::policy::installation::PolicyError;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::request::{ClientIntent, PolicyRejection, RetainedOutcome};
use wf_types::version::{LogIdentity, TimestampMillis};

mod point;
mod retention;
use point::OriginalPolicySetAuthority;
pub use point::{OriginalPolicySetFreshView, prepare_original_policy_set_point_fresh};

/// Fresh decision plus the exact predecessor and only actually reached context.
pub type OriginalPolicySetPreparation<'current> = (
    OriginalPolicySetFresh,
    Option<&'current [u8]>,
    Option<u32>,
    Option<(u64, u64)>,
);

/// Coherent source inputs and borrowed captured order facts, without attestation.
#[derive(Clone, Copy)]
pub struct OriginalPolicySetApplyView<'view, 'raw, 'context, 'capture> {
    base: OriginalCompletePolicyReplaceApplyView<'view, 'raw, 'context>,
    orders: &'capture OriginalPolicySetMapOrders<'capture>,
}

impl<'view, 'raw, 'context, 'capture> OriginalPolicySetApplyView<'view, 'raw, 'context, 'capture> {
    /// Combines immutable supplied inputs; construction grants no current authority.
    #[must_use]
    pub const fn new(
        base: OriginalCompletePolicyReplaceApplyView<'view, 'raw, 'context>,
        orders: &'capture OriginalPolicySetMapOrders<'capture>,
    ) -> Self {
        Self { base, orders }
    }
    /// Reads the supplied coherent view without certifying its provenance.
    #[must_use]
    pub const fn base(&self) -> OriginalCompletePolicyReplaceApplyView<'view, 'raw, 'context> {
        self.base
    }
    /// Borrows represented order facts without claiming their VM origin.
    #[must_use]
    pub const fn orders(&self) -> &'capture OriginalPolicySetMapOrders<'capture> {
        self.orders
    }
}

/// An independently owned installation and a separately owned retained response.
pub type OriginalPolicySetFresh = (Option<OriginalFlowPolicyInstall>, RetainedOutcome);

enum PolicySetRejected {
    Policy(PolicyRejection),
    Input(FlowPolicyInputRejection),
}

/// Prepares Fresh work after a separately verified frame and one request gate.
///
/// Both replace modes and reply selector remain bound by registry4. The returned
/// original envelope is not the final public response-state projection.
/// # Errors
/// Preserves operational/current origins and retains only reached caller errors.
pub fn prepare_original_policy_set_fresh(
    view: OriginalPolicySetApplyView<'_, '_, '_, '_>,
    header: OriginalPolicySetFrameHeader<'_>,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalPolicySetFresh, RawError> {
    point::prepare_fresh(&view, header, committed, limits, admission, charge, false)
        .map(|prepared| prepared.0)
}

/// Retains exact original inputs without granting physical installation authority.
/// # Errors
/// Preserves the original ordered preparation errors and same-account work.
pub fn prepare_original_policy_set_fresh_with_query<'raw>(
    view: OriginalPolicySetApplyView<'_, 'raw, '_, '_>,
    header: OriginalPolicySetFrameHeader<'_>,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalPolicySetPreparation<'raw>, RawError> {
    point::prepare_fresh(&view, header, committed, limits, admission, charge, true)
}

fn prepare<'raw>(
    view: &impl OriginalPolicySetAuthority<'raw>,
    input: (
        &[u8],
        crate::policy::complete::outer::public_set_frame::PolicySetFrameCursor<'_>,
        Option<u64>,
        bool,
    ),
    bounds: (usize, usize, bool),
    stage: (usize, PolicySchemaLimits),
    charge: &mut PolicySchemaCharge,
    original: &mut Option<&'raw [u8]>,
    used: &mut (Option<u32>, Option<(u64, u64)>),
) -> Result<Result<OriginalFlowPolicyInstall, PolicySetRejected>, RawError> {
    let (base, limits) = stage;
    let (key, patch, expected, replace) = input;
    pay(charge, 128)?;
    if patch.kind() != Kind::Map {
        return Err(schema(PolicySchemaError::InvalidAuthority));
    }
    pay(charge, plus(128, key.len())?)?;
    let requested = key
        .strip_prefix(b"f:{f}:policy:")
        .filter(|s| !s.is_empty())
        .ok_or_else(|| schema(PolicySchemaError::InvalidAuthority))?;
    let bytes = view.current(requested, charge)?;
    *original = bytes;
    if bounds.2
        && let Some(bytes) = bytes.filter(|b| b.starts_with(b"FWPV"))
    {
        return point::prepare_v2(
            view,
            bytes,
            (key, patch, expected, replace),
            (bounds.0, bounds.1, base),
            limits,
            charge,
            used,
        );
    }
    let current_live = plus(base, bytes.map_or(0, <[u8]>::len))?;
    let current = if let Some(bytes) = bytes {
        peak_at(current_live, charge).map_err(current_error)?;
        Some(
            if bounds.2 {
                crate::policy::complete::original_entry::indexed::decode_portable_retained(
                    bytes,
                    current_live,
                    limits,
                    charge,
                )
            } else {
                decode_retained(bytes, &view.safety(), 263_168, current_live, limits, charge)
            }
            .map_err(current_error)?
            .ok_or(RawError::Current(OriginalPolicyReadError::Corrupt))?,
        )
    } else {
        None
    };
    let stored = current.as_ref().map_or(0, |c| c.entry().generation());
    pay(charge, 128)?;
    if expected.is_some_and(|value| value != stored) {
        return Ok(Err(policy(PolicyError::StaleGeneration)?));
    }
    if let Some(current) = &current {
        bind_original_current_type(current.entry(), requested, charge)?;
    }
    if replace {
        drop(current);
        return point::finish(
            view,
            (key, stored),
            Cursor::Frame4(patch),
            (plus(bounds.0, 32)?, bounds.1, base),
            limits,
            charge,
            used,
        );
    }
    merge_current(
        view,
        current,
        (key, stored, patch),
        (bounds.0, bounds.1, base, bounds.2),
        limits,
        charge,
        used,
    )
}

fn merge_current<'raw>(
    view: &impl OriginalPolicySetAuthority<'raw>,
    mut current: Option<OriginalPolicyCurrentIndex<'_, '_>>,
    input: (
        &[u8],
        u64,
        crate::policy::complete::outer::public_set_frame::PolicySetFrameCursor<'_>,
    ),
    stage: (usize, usize, usize, bool),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    used: &mut (Option<u32>, Option<(u64, u64)>),
) -> Result<Result<OriginalFlowPolicyInstall, PolicySetRejected>, RawError> {
    let (key, stored, patch) = input;
    let (bounds, base) = ((stage.0, stage.1), stage.2);
    let bytes = current.as_ref().map(|c| c.entry().encoded());
    let current_live = plus(base, bytes.map_or(0, <[u8]>::len))?;
    let orders = if stage.3 { None } else { view.orders() };
    if let Some(orders) = orders {
        orders.bind_current_bytes(bytes, charge).map_err(encoding)?;
    }
    let count = current
        .as_ref()
        .map_or(0, OriginalPolicyCurrentIndex::count);
    let total = plus(plus(count, bounds.0)?, 32)?;
    let depth = bounds.1.max(
        current
            .as_ref()
            .map_or(0, OriginalPolicyCurrentIndex::depth_bound),
    );
    let mut live = plus(
        current_live,
        current
            .as_ref()
            .map_or(Ok(0), OriginalPolicyCurrentIndex::owner_bound)
            .map_err(encoding)?,
    )?;
    if let Some(orders) = orders {
        live = plus(live, orders.admit_current(count, charge).map_err(encoding)?)?;
        live = plus(live, orders.admit_merged(total, charge).map_err(encoding)?)?;
    }
    peak_at(live, charge).map_err(encoding)?;
    if let Some(current) = &mut current {
        current.build_catalog(live, charge).map_err(current_error)?;
        live = plus(live, current.catalog_bound().map_err(encoding)?)?;
    }
    let mut arena = match orders {
        Some(orders) => PolicyMergeArena::new(total, depth, orders, live, charge),
        None => PolicyMergeArena::new_native(total, depth, live, charge),
    }
    .map_err(encoding)?;
    arena
        .merge(
            current.as_ref().map(OriginalPolicyCurrentIndex::cursor),
            patch,
            charge,
        )
        .map_err(encoding)?;
    live = plus(
        live,
        PolicyMergeArena::bound(total, depth).map_err(encoding)?,
    )?;
    point::finish(
        view,
        (key, stored),
        Cursor::Merge(arena.cursor()),
        (total, depth, live),
        limits,
        charge,
        used,
    )
}

fn policy(error: PolicyError) -> Result<PolicySetRejected, RawError> {
    PolicyRejection::new(error)
        .map(PolicySetRejected::Policy)
        .map_err(|e| RawError::Replace(ExistingError::Outcome(e)))
}
fn current_error(error: Error) -> RawError {
    RawError::Current(OriginalPolicyReadError::Encoding(error))
}
fn encoding(error: Error) -> RawError {
    RawError::Replace(ExistingError::Request(error.into()))
}
fn schema(error: PolicySchemaError) -> RawError {
    encoding(Error::Schema(error))
}
fn operational(error: ApplyError) -> RawError {
    RawError::Replace(ExistingError::Apply(error))
}
fn plus(a: usize, b: usize) -> Result<usize, RawError> {
    a.checked_add(b)
        .ok_or_else(|| schema(PolicySchemaError::LengthOverflow))
}
fn pay(charge: &mut PolicySchemaCharge, work: usize) -> Result<(), RawError> {
    charge_schema(
        charge,
        u64::try_from(work).map_err(|_| schema(PolicySchemaError::LengthOverflow))?,
        0,
    )
    .map_err(schema)
}
fn copy(bytes: &[u8], charge: &mut PolicySchemaCharge) -> Result<Vec<u8>, RawError> {
    pay(charge, plus(128, bytes.len())?)?;
    let mut out = Vec::new();
    out.try_reserve_exact(bytes.len())
        .map_err(|_| encoding(Error::AllocationFailure))?;
    out.extend_from_slice(bytes);
    Ok(out)
}
