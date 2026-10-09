//! Explicit selection scope, committed time and separate work budgets.

use wf_types::bounds::MAX_RECORD_ID_BYTES;
use wf_types::version::TimestampMillis;

use super::{ClaimRecord, SelectionError};
use crate::workflow::MAX_EXACT_MILLIS;

pub(super) const MAX_CLAIMS: usize = 1_000;
pub(super) const MAX_SCANS: usize = 16_384;
pub(super) const MAX_HEADS: usize = 4_096;
const MAX_FILTER_FOOTPRINT: usize = 64;

/// Authoritative partition selection, with no implicit client routing trust.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PartitionFilter<'a> {
    /// Every partition represented by the coherent group view.
    Any,
    /// Only the nonempty unique partition components listed here.
    Only(&'a [&'a [u8]]),
}

/// Logical handler state selection, independent of list order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateFilter<'a> {
    /// Every active logical state in the authoritative view.
    Any,
    /// Only the nonempty unique states listed here.
    Only(&'a [&'a [u8]]),
}

/// Committed selection request; the decision consults no clock or external service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionRequest<'a> {
    /// Expected authoritative group.
    pub group: &'a [u8],
    /// Required workflow type.
    pub workflow_type: &'a [u8],
    /// Requested partition scope.
    pub partitions: PartitionFilter<'a>,
    /// Included logical states.
    pub states: StateFilter<'a>,
    /// Excluded logical states, applied after inclusion.
    pub exclude_states: &'a [&'a [u8]],
    /// Optional source priority band; omission drains 2, then 1, then 0.
    pub priority: Option<u8>,
    /// Explicit committed milliseconds, never a wall-clock read.
    pub now: TimestampMillis,
    /// Maximum selected records, 1 through 1000.
    pub limit: usize,
    /// Maximum merged rows inspected, 1 through 16384.
    pub scan_budget: usize,
    /// Maximum lane-head lookups, 1 through 4096, separate from follower-free scans.
    pub head_budget: usize,
    /// Explicitly allow selection of expired running records as reclaim actions.
    ///
    /// This is narrower than automatic source reclaim-ratio/timeout scheduling.
    pub reclaim_expired: bool,
}

impl<'a> SelectionRequest<'a> {
    /// Creates source default limit 1, all states/partitions, and bounded work budgets.
    #[must_use]
    pub const fn new(group: &'a [u8], workflow_type: &'a [u8], now: TimestampMillis) -> Self {
        Self {
            group,
            workflow_type,
            partitions: PartitionFilter::Any,
            states: StateFilter::Any,
            exclude_states: &[],
            priority: None,
            now,
            limit: 1,
            scan_budget: MAX_SCANS,
            head_budget: MAX_HEADS,
            reclaim_expired: false,
        }
    }

    pub(super) fn validate(self) -> Result<(), SelectionError> {
        if !valid_component(self.group) || !valid_component(self.workflow_type) {
            return Err(SelectionError::InvalidScope);
        }
        let partitions = match self.partitions {
            PartitionFilter::Any => 1,
            PartitionFilter::Only(partitions) => validate_components(partitions, false)?,
        };
        let states = match self.states {
            StateFilter::Any => 1,
            StateFilter::Only(states) => validate_components(states, false)?,
        };
        validate_components(self.exclude_states, true)?;
        if partitions * states > MAX_FILTER_FOOTPRINT {
            return Err(SelectionError::InvalidScope);
        }
        if self.priority.is_some_and(|priority| priority > 2) {
            return Err(SelectionError::InvalidPriority);
        }
        if self.now.get() > MAX_EXACT_MILLIS
            || !(1..=MAX_CLAIMS).contains(&self.limit)
            || !(1..=MAX_SCANS).contains(&self.scan_budget)
            || !(1..=MAX_HEADS).contains(&self.head_budget)
        {
            return Err(SelectionError::InvalidBudget);
        }
        Ok(())
    }

    pub(super) fn matches(self, entry: ClaimRecord<'_>) -> bool {
        entry.record.workflow_type() == self.workflow_type
            && match self.partitions {
                PartitionFilter::Any => true,
                PartitionFilter::Only(partitions) => partitions.contains(&entry.partition_key),
            }
            && match self.states {
                StateFilter::Any => true,
                StateFilter::Only(states) => states.contains(&entry.logical_state()),
            }
            && !self.exclude_states.contains(&entry.logical_state())
            && self
                .priority
                .is_none_or(|priority| priority == entry.priority)
    }
}

pub(super) fn valid_component(component: &[u8]) -> bool {
    !component.is_empty() && component.len() <= MAX_RECORD_ID_BYTES
}

fn validate_components(components: &[&[u8]], allow_empty: bool) -> Result<usize, SelectionError> {
    if (!allow_empty && components.is_empty()) || components.len() > MAX_FILTER_FOOTPRINT {
        return Err(SelectionError::InvalidScope);
    }
    for (index, component) in components.iter().enumerate() {
        if !valid_component(component) || components[..index].contains(component) {
            return Err(SelectionError::InvalidScope);
        }
    }
    Ok(components.len())
}
