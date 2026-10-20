//! Closed pure current/context adapters and the shared lazy Fresh reducer.

use super::{
    OriginalFlowPolicyInstall, OriginalPolicyReadError, OriginalPolicySetApplyView,
    OriginalPolicySetFrameHeader, OriginalPolicySetMapOrders, OriginalPolicySetPreparation,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, PolicySetRejected, RawError,
};
use super::{copy, encoding, operational, pay, peak_at, plus, policy, schema};
use crate::policy::complete::flow_input::{
    emit_original_flow_policy_envelope, prepare_joined_flow_policy_layout,
    value::PolicyInputCursor as Cursor,
};
use crate::policy::complete::{
    OriginalEtfSafety, OriginalPolicyReadValue, RawFlowPolicyValue as Raw,
};
use crate::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use crate::retention::MAX_HISTORY_EVENTS;
use crate::workflow::admission::AdmissionLimits;
use crate::workflow::batch::ApplyError;
use crate::workflow::request::ClientIntent;
use wf_types::version::{LogIdentity, TimestampMillis};

/// Same-guard mathematical current and lazy context inputs, with no request gate.
#[derive(Clone, Copy)]
pub struct OriginalPolicySetFreshView<'raw> {
    /// Actual original current read class, without provenance attestation.
    pub current: OriginalPolicyReadValue<'raw>,
    /// Same-type active migration fact or its operational refusal.
    pub active_migration: Result<u64, ApplyError>,
    /// Same-type completed migration fact or its operational refusal.
    pub completed_migration: Result<u64, ApplyError>,
    /// Captured immutable history ceiling, inspected only when reached.
    pub history_max_events_limit: Result<u32, ApplyError>,
    /// Captured positive retention window, inspected at the original late phase.
    pub outcome_retention_ms: Result<u64, ApplyError>,
}

pub(super) trait OriginalPolicySetAuthority<'raw> {
    fn current(
        &self,
        requested: &[u8],
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<&'raw [u8]>, RawError>;
    fn safety(&self) -> OriginalEtfSafety<'_>;
    fn orders(&self) -> Option<&OriginalPolicySetMapOrders<'_>>;
    fn history(&self) -> Result<u32, ApplyError>;
    fn active(&self, requested: &[u8]) -> Result<u64, ApplyError>;
    fn completed(&self, requested: &[u8]) -> Result<u64, ApplyError>;
    fn window(&self) -> Result<u64, ApplyError>;
}
impl<'raw> OriginalPolicySetAuthority<'raw> for OriginalPolicySetApplyView<'_, 'raw, '_, '_> {
    fn current(
        &self,
        requested: &[u8],
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<&'raw [u8]>, RawError> {
        super::read_original_current_bytes(&self.base, requested, charge)
    }
    fn safety(&self) -> OriginalEtfSafety<'_> {
        self.base.safety
    }
    fn orders(&self) -> Option<&OriginalPolicySetMapOrders<'_>> {
        Some(self.orders)
    }
    fn history(&self) -> Result<u32, ApplyError> {
        self.base.history_max_events_limit
    }
    fn active(&self, requested: &[u8]) -> Result<u64, ApplyError> {
        self.base
            .active_migrations
            .get(requested)
            .copied()
            .unwrap_or(Ok(0))
    }
    fn completed(&self, requested: &[u8]) -> Result<u64, ApplyError> {
        self.base
            .completed_migrations
            .get(requested)
            .copied()
            .unwrap_or(Ok(0))
    }
    fn window(&self) -> Result<u64, ApplyError> {
        self.base.outcome_retention_ms
    }
}
impl<'raw> OriginalPolicySetAuthority<'raw> for OriginalPolicySetFreshView<'raw> {
    fn current(
        &self,
        requested: &[u8],
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<&'raw [u8]>, RawError> {
        pay(charge, plus(128, requested.len())?)?;
        match self.current {
            OriginalPolicyReadValue::Absent => Ok(None),
            OriginalPolicyReadValue::Binary(bytes) => Ok(Some(bytes)),
            OriginalPolicyReadValue::Other => {
                Err(RawError::Current(OriginalPolicyReadError::Corrupt))
            }
            OriginalPolicyReadValue::Unavailable => {
                Err(RawError::Current(OriginalPolicyReadError::ShardUnavailable))
            }
            OriginalPolicyReadValue::ReadFailed => {
                Err(RawError::Current(OriginalPolicyReadError::ReadFailed))
            }
        }
    }
    fn safety(&self) -> OriginalEtfSafety<'_> {
        crate::policy::complete::canonical_original_etf_safety()
    }
    fn orders(&self) -> Option<&OriginalPolicySetMapOrders<'_>> {
        None
    }
    fn history(&self) -> Result<u32, ApplyError> {
        self.history_max_events_limit
    }
    fn active(&self, _: &[u8]) -> Result<u64, ApplyError> {
        self.active_migration
    }
    fn completed(&self, _: &[u8]) -> Result<u64, ApplyError> {
        self.completed_migration
    }
    fn window(&self) -> Result<u64, ApplyError> {
        self.outcome_retention_ms
    }
}

/// Prepares only Fresh work after one separately completed charged point gate.
/// # Errors
/// Preserves original current, source, lazy context, work and retention origins.
pub fn prepare_original_policy_set_point_fresh<'raw>(
    view: OriginalPolicySetFreshView<'raw>,
    header: OriginalPolicySetFrameHeader<'_>,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalPolicySetPreparation<'raw>, RawError> {
    prepare_fresh(&view, header, committed, limits, admission, charge, true)
}

pub(super) fn prepare_fresh<'raw>(
    view: &impl OriginalPolicySetAuthority<'raw>,
    header: OriginalPolicySetFrameHeader<'_>,
    committed: (ClientIntent, LogIdentity, TimestampMillis),
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
    native: bool,
) -> Result<OriginalPolicySetPreparation<'raw>, RawError> {
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
    super::charge_schema(
        charge,
        128,
        u64::try_from(live).map_err(|_| schema(PolicySchemaError::LengthOverflow))?,
    )
    .map_err(schema)?;
    let (scope, request, key, patch, expected, replace, _) = header.into_parts();
    let (mut current, mut used) = (None, (None, None));
    let prepared = super::prepare(
        view,
        (key, patch, expected, replace),
        (bounds.0, bounds.1, native),
        (live, limits),
        charge,
        &mut current,
        &mut used,
    )?;
    let fresh = super::retention::retain(
        scope,
        request,
        committed,
        prepared,
        view.window(),
        admission,
        charge,
    )?;
    Ok((fresh, current, used.0, used.1))
}

pub(super) fn finish<'raw>(
    view: &impl OriginalPolicySetAuthority<'raw>,
    key: (&[u8], u64),
    attrs: Cursor<'_>,
    stage: (usize, usize, usize),
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    used: &mut (Option<u32>, Option<(u64, u64)>),
) -> Result<Result<OriginalFlowPolicyInstall, PolicySetRejected>, RawError> {
    let (key, stored) = key;
    let requested = key
        .get(13..)
        .ok_or_else(|| schema(PolicySchemaError::InvalidAuthority))?;
    pay(charge, 128)?;
    let maximum = view.history().map_err(operational)?;
    if maximum == 0 || maximum > MAX_HISTORY_EVENTS {
        return Err(operational(ApplyError::InvalidAuthority));
    }
    used.0 = Some(maximum);
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
    let mut generations = [0; 2];
    for (index, value) in [view.active(requested), view.completed(requested)]
        .into_iter()
        .enumerate()
    {
        pay(charge, plus(128, requested.len())?)?;
        let value = value.map_err(operational)?;
        if value > MAX_POLICY_GENERATION {
            return Err(operational(ApplyError::InvalidAuthority));
        }
        high = high.max(value);
        generations[index] = value;
    }
    used.1 = Some((generations[0], generations[1]));
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
