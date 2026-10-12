//! Shared fresh-only mapping; only private pure reducers supply the callback.

use wf_types::version::TimestampMillis;

use super::RetainedOutcome;
use super::gate::{GateDecision, RequestGate};
use crate::workflow::batch::ApplyError;
use crate::workflow::control::ControlPlan;

/// Scoped operation decision, with exact original replay independent of operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DispatchDecision<'a, T> {
    /// First request's accepted/deterministic-rejected semantic operation result.
    Fresh(T),
    /// Exact original immutable bytes/status/time/position/retention owner.
    Replay(&'a RetainedOutcome),
    /// Different intent reused the request ID; original is preserved.
    IntentConflict,
    /// Retention expired; do not reexecute this retained identity.
    Expired,
    /// Exact last log replay; no control or operation writes.
    LogReplay,
}

/// Proposed control and semantic result, requiring outer atomic persistence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatchPlan<'a, T> {
    control: ControlPlan,
    decision: DispatchDecision<'a, T>,
}
impl<'a, T> DispatchPlan<'a, T> {
    /// Reads current processing control independently of original retained result.
    #[must_use]
    pub const fn control(&self) -> ControlPlan {
        self.control
    }
    /// Borrows fresh semantic result or scoped original replay decision.
    #[must_use]
    pub const fn decision(&self) -> &DispatchDecision<'a, T> {
        &self.decision
    }
    /// Only fresh accepted/deterministic-rejected semantic results need an outcome.
    #[must_use]
    pub const fn requires_outcome_write(&self) -> bool {
        matches!(self.decision, DispatchDecision::Fresh(_))
    }
    /// Transfers bounded semantic/control values without copying replay bytes.
    #[must_use]
    pub fn into_parts(self) -> (ControlPlan, DispatchDecision<'a, T>) {
        (self.control, self.decision)
    }
}

pub(super) fn after_gate<T>(
    gate: RequestGate<'_>,
    fresh: impl FnOnce(TimestampMillis) -> Result<T, ApplyError>,
) -> Result<DispatchPlan<'_, T>, ApplyError> {
    let control = gate.control();
    let decision = match gate.decision() {
        GateDecision::Fresh => DispatchDecision::Fresh(fresh(control.execution_time())?),
        GateDecision::Replay(original) => DispatchDecision::Replay(original),
        GateDecision::IntentConflict => DispatchDecision::IntentConflict,
        GateDecision::Expired => DispatchDecision::Expired,
        GateDecision::LogReplay => DispatchDecision::LogReplay,
    };
    Ok(DispatchPlan { control, decision })
}
