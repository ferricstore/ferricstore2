//! Ordered independent mutations over borrowed current and earlier owned writes.

use std::collections::BTreeMap;

use crate::claim::ScopedRecordId;
use crate::workflow::admission::{
    AdmissionError, AdmissionLimits, add, admit_command, record_bytes,
};
use crate::workflow::{DomainError, Operation, WorkflowRecord, reduce};

use super::{ApplyError, BatchItem, BatchRejection, GroupView, ScopedPlan};

/// Owned ordered successes/errors; later successes see preceding successful writes.
#[derive(Debug, Eq, PartialEq)]
pub struct IndependentBatchPlan {
    results: Vec<Result<ScopedPlan, DomainError>>,
}

impl IndependentBatchPlan {
    /// Borrows one result per input in its original order.
    #[must_use]
    pub fn results(&self) -> &[Result<ScopedPlan, DomainError>] {
        &self.results
    }

    /// Moves all results without retaining any input or current-view borrow.
    #[must_use]
    pub fn into_results(self) -> Vec<Result<ScopedPlan, DomainError>> {
        self.results
    }
}

/// Independent business outcomes or a whole input/profile refusal with no plan.
#[derive(Debug, Eq, PartialEq)]
pub enum IndependentBatchDecision {
    /// Complete ordered independent results, including individual domain errors.
    Accepted(IndependentBatchPlan),
    /// No provisional result or write is exposed.
    Rejected(BatchRejection),
}

/// Compatibility name for the transition entry point's shared decision.
pub use IndependentBatchDecision as IndependentTransitionDecision;
/// Compatibility name for the transition entry point's shared owned plan.
pub use IndependentBatchPlan as IndependentTransitionPlan;

/// Decides same-group transitions in input order, retaining independent failures.
///
/// Repeated scoped identities consume the last successful proposed write. The
/// plan owns core/history fields and provides no indexed or durable apply proof.
/// The decoder remains responsible for actual key/value correspondence.
/// # Errors
/// Rejects corrupt/missing authority or a fallible result allocation operationally.
pub fn decide_independent_transition_batch(
    view: GroupView<'_>,
    items: &[BatchItem<'_>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndependentTransitionDecision, ApplyError> {
    decide(view, items, encoded_entry, limits, false)
}

/// Decides same-group complete, fail, caller-resolved retry and cancel commands.
///
/// Results retain input order and earlier successful writes. Policy retries are
/// outside this profile; plain retries retain their supplied committed deadline.
/// This owns core/history plans without performing indexed or durable apply.
/// The decoder remains responsible for actual key/value correspondence.
/// # Errors
/// Rejects corrupt/missing authority or a fallible result allocation operationally.
pub fn decide_independent_terminal_batch(
    view: GroupView<'_>,
    items: &[BatchItem<'_>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
) -> Result<IndependentBatchDecision, ApplyError> {
    decide(view, items, encoded_entry, limits, true)
}

fn decide(
    view: GroupView<'_>,
    items: &[BatchItem<'_>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
    terminal: bool,
) -> Result<IndependentBatchDecision, ApplyError> {
    if let Some(error) = preflight(view, items, encoded_entry, limits, terminal)? {
        return Ok(IndependentBatchDecision::Rejected(error));
    }
    let mut results = Vec::new();
    results
        .try_reserve_exact(items.len())
        .map_err(|_| ApplyError::AllocationFailed)?;
    let mut latest = BTreeMap::new();
    let mut variable_bytes = 0;
    for item in items {
        let id = ScopedRecordId {
            partition_key: item.partition.as_bytes(),
            id: item.command.id.as_bytes(),
        };
        let current = current_record(view, id, &latest, &results)?;
        if current.is_some_and(|record| record.id() != &item.command.id) {
            return Err(ApplyError::InvalidAuthority);
        }
        if let Some(error) = admit_step(*item, current, &mut variable_bytes, items.len(), limits)? {
            return Ok(IndependentBatchDecision::Rejected(
                BatchRejection::Admission(error),
            ));
        }
        let result = reduce(current, item.command).map(|plan| ScopedPlan {
            partition: item.partition.clone(),
            plan,
        });
        let changed = result
            .as_ref()
            .is_ok_and(|plan| plan.plan().write().is_some());
        results.push(result);
        if changed {
            latest.insert(id, results.len() - 1);
        }
    }
    drop(latest);
    Ok(IndependentBatchDecision::Accepted(IndependentBatchPlan {
        results,
    }))
}

fn preflight(
    view: GroupView<'_>,
    items: &[BatchItem<'_>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
    terminal: bool,
) -> Result<Option<BatchRejection>, ApplyError> {
    if items.len() > limits.batch_items() {
        return Ok(Some(BatchRejection::Admission(AdmissionError::BatchItems)));
    }
    if let Err(error) = limits.check_total(encoded_entry.len()) {
        return Ok(Some(BatchRejection::Admission(error)));
    }
    for item in items {
        let group = view
            .placements
            .get(item.partition)
            .ok_or(ApplyError::InvalidAuthority)?;
        if group != view.group {
            return Ok(Some(BatchRejection::CrossGroup));
        }
    }
    if items
        .iter()
        .any(|item| !allowed_operation(&item.command.operation, terminal))
    {
        return Ok(Some(BatchRejection::Domain(
            DomainError::UnsupportedOperation,
        )));
    }
    Ok(charge(0, items.len(), limits)
        .err()
        .map(BatchRejection::Admission))
}

fn allowed_operation(operation: &Operation, terminal: bool) -> bool {
    if terminal {
        matches!(
            operation,
            Operation::Complete(_)
                | Operation::Fail(_)
                | Operation::Retry(_)
                | Operation::Cancel(_)
        )
    } else {
        matches!(operation, Operation::Transition(_))
    }
}

fn current_record<'row>(
    view: GroupView<'row>,
    id: ScopedRecordId<'_>,
    latest: &BTreeMap<ScopedRecordId<'_>, usize>,
    results: &'row [Result<ScopedPlan, DomainError>],
) -> Result<Option<&'row WorkflowRecord>, ApplyError> {
    match latest.get(&id) {
        Some(index) => results
            .get(*index)
            .and_then(|result| result.as_ref().ok())
            .and_then(|plan| plan.plan().write())
            .map(|write| Some(&write.record))
            .ok_or(ApplyError::InvalidAuthority),
        None => Ok(view.records.get(&id).copied()),
    }
}

fn admit_step(
    item: BatchItem<'_>,
    current: Option<&WorkflowRecord>,
    variable_bytes: &mut usize,
    count: usize,
    limits: AdmissionLimits,
) -> Result<Option<AdmissionError>, ApplyError> {
    let command = match admit_command(item.command, item.state_key, item.encoded_values, limits) {
        Ok(bytes) => bytes,
        Err(error) => return Ok(Some(error)),
    };
    let current = current
        .map(|record| record_bytes(record, limits))
        .transpose()
        .map_err(|_| ApplyError::InvalidAuthority)?
        .unwrap_or(0);
    let attempt = (|| {
        add(variable_bytes, command)?;
        add(variable_bytes, item.partition.as_bytes().len())?;
        add(variable_bytes, current)?;
        charge(*variable_bytes, count, limits)
    })();
    Ok(attempt.err())
}

fn charge(
    variable_bytes: usize,
    count: usize,
    limits: AdmissionLimits,
) -> Result<(), AdmissionError> {
    let mut bytes = limits.charge(variable_bytes, count)?;
    add(
        &mut bytes,
        count
            .checked_mul(128)
            .ok_or(AdmissionError::LengthOverflow)?,
    )?;
    limits.check_total(bytes)
}
