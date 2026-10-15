//! Strict current, source merge, one business walk and late original emission.

use super::flow::OriginalFlowPolicyInstall;
use super::original::{bind_original_current_type, read_original_current_bytes};
use super::{
    CompletePolicyReplaceApplyError as ExistingError,
    OriginalCompletePolicyReplaceApplyError as RawError, OriginalCompletePolicyReplaceApplyView,
};
use crate::policy::complete::flow_input::{
    emit_original_flow_policy_envelope, prepare_joined_flow_policy_layout,
    value::{PolicyInputCursor as Cursor, PolicyInputKind as Kind},
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
    RawFlowPolicyValue as Raw, admission::charge_schema,
};
use crate::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use crate::retention::MAX_HISTORY_EVENTS;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::request::{ClientIntent, PolicyRejection, RetainedOutcome};
use wf_types::version::{LogIdentity, TimestampMillis};

mod retention;

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
    if charge.profile() != limits {
        return Err(schema(PolicySchemaError::ProfileMismatch));
    }
    let (bounds, backing) = (header.patch_bounds(), header.patch_backing());
    let mut live = plus(header.encoded().len(), backing)?;
    for bytes in [
        header.scope().tenant().as_bytes(),
        header.scope().group().as_bytes(),
        header.scope().recovery().as_bytes(),
        header.request().as_bytes(),
    ] {
        live = plus(live, plus(128, bytes.len())?)?;
    }
    charge_schema(
        charge,
        128,
        u64::try_from(live).map_err(|_| schema(PolicySchemaError::LengthOverflow))?,
    )
    .map_err(schema)?;
    let (scope, request, key, patch, expected, replace, _) = header.into_parts();
    let prepared = prepare(
        &view,
        (key, patch, expected, replace),
        bounds,
        live,
        limits,
        charge,
    )?;
    retention::retain(
        scope,
        request,
        committed,
        prepared,
        view.base.outcome_retention_ms,
        admission,
        charge,
    )
}

fn prepare(
    view: &OriginalPolicySetApplyView<'_, '_, '_, '_>,
    input: (
        &[u8],
        crate::policy::complete::outer::public_set_frame::PolicySetFrameCursor<'_>,
        Option<u64>,
        bool,
    ),
    bounds: (usize, usize),
    base: usize,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<OriginalFlowPolicyInstall, PolicySetRejected>, RawError> {
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
    let bytes = read_original_current_bytes(&view.base, requested, charge)?;
    let current_live = plus(base, bytes.map_or(0, <[u8]>::len))?;
    let current = if let Some(bytes) = bytes {
        peak_at(current_live, charge).map_err(current_error)?;
        Some(
            decode_retained(
                bytes,
                &view.base.safety,
                263_168,
                current_live,
                limits,
                charge,
            )
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
        return finish(
            &view.base,
            (key, stored),
            Cursor::Frame4(patch),
            (plus(bounds.0, 32)?, bounds.1, base),
            limits,
            charge,
        );
    }
    merge_current(
        view,
        current,
        (key, stored, patch),
        bounds,
        base,
        limits,
        charge,
    )
}

fn merge_current(
    view: &OriginalPolicySetApplyView<'_, '_, '_, '_>,
    mut current: Option<OriginalPolicyCurrentIndex<'_, '_>>,
    input: (
        &[u8],
        u64,
        crate::policy::complete::outer::public_set_frame::PolicySetFrameCursor<'_>,
    ),
    bounds: (usize, usize),
    base: usize,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<OriginalFlowPolicyInstall, PolicySetRejected>, RawError> {
    let (key, stored, patch) = input;
    let bytes = current.as_ref().map(|c| c.entry().encoded());
    let current_live = plus(base, bytes.map_or(0, <[u8]>::len))?;
    view.orders
        .bind_current_bytes(bytes, charge)
        .map_err(encoding)?;
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
    live = plus(
        live,
        view.orders.admit_current(count, charge).map_err(encoding)?,
    )?;
    live = plus(
        live,
        view.orders.admit_merged(total, charge).map_err(encoding)?,
    )?;
    peak_at(live, charge).map_err(encoding)?;
    if let Some(current) = &mut current {
        current.build_catalog(live, charge).map_err(current_error)?;
        live = plus(live, current.catalog_bound().map_err(encoding)?)?;
    }
    let mut arena =
        PolicyMergeArena::new(total, depth, view.orders, live, charge).map_err(encoding)?;
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
    finish(
        &view.base,
        (key, stored),
        Cursor::Merge(arena.cursor()),
        (total, depth, live),
        limits,
        charge,
    )
}

fn finish(
    view: &OriginalCompletePolicyReplaceApplyView<'_, '_, '_>,
    key: (&[u8], u64),
    attrs: Cursor<'_>,
    stage: (usize, usize, usize),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<OriginalFlowPolicyInstall, PolicySetRejected>, RawError> {
    let (key, stored) = key;
    let requested = key
        .get(13..)
        .ok_or_else(|| schema(PolicySchemaError::InvalidAuthority))?;
    pay(charge, 128)?;
    let maximum = view.history_max_events_limit.map_err(operational)?;
    if maximum == 0 || maximum > MAX_HISTORY_EVENTS {
        return Err(operational(ApplyError::InvalidAuthority));
    }
    let normalized = match prepare_joined_flow_policy_layout(
        (Cursor::Flow(Raw::Binary(requested)), attrs, maximum),
        (stage.0, stage.1),
        stage.2,
        limits,
        charge,
    )
    .map_err(encoding)?
    {
        Ok(layout) => layout,
        Err(reason) => return Ok(Err(PolicySetRejected::Input(reason))),
    };
    let mut high = stored;
    for map in [view.active_migrations, view.completed_migrations] {
        pay(charge, plus(128, requested.len())?)?;
        let value = map
            .get(requested)
            .copied()
            .unwrap_or(Ok(0))
            .map_err(operational)?;
        if value > MAX_POLICY_GENERATION {
            return Err(operational(ApplyError::InvalidAuthority));
        }
        high = high.max(value);
    }
    pay(charge, 128)?;
    let Some(generation) = high.checked_add(1).filter(|&n| n <= MAX_POLICY_GENERATION) else {
        return Ok(Err(policy(PolicyError::GenerationExhausted)?));
    };
    let g = if generation <= 255 {
        2
    } else if i32::try_from(generation).is_ok() {
        5
    } else {
        3 + generation.bit_width().div_ceil(8) as usize
    };
    let length = plus(normalized.1, plus(18, g)?)?;
    let scratch = stage
        .0
        .checked_mul(672)
        .and_then(|n| n.checked_add(stage.1.checked_mul(64)?))
        .and_then(|n| n.checked_add(2752))
        .ok_or_else(|| schema(PolicySchemaError::LengthOverflow))?;
    let live = plus(plus(stage.2, scratch)?, length)?;
    peak_at(live, charge).map_err(encoding)?;
    let encoded =
        emit_original_flow_policy_envelope(&normalized, generation, charge).map_err(encoding)?;
    peak_at(plus(live, key.len())?, charge).map_err(encoding)?;
    let key = copy(key, charge)?;
    pay(charge, 128)?;
    Ok(Ok(OriginalFlowPolicyInstall::new_proposed(
        key, stored, generation, encoded,
    )))
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
