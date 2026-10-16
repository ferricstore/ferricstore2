//! Ordered independent transitions over borrowed current and earlier owned writes.

use std::collections::BTreeMap;

use crate::claim::ScopedRecordId;
use crate::workflow::admission::{
    AdmissionError, AdmissionLimits, add, admit_command, record_bytes,
};
use crate::workflow::{DomainError, Operation, WorkflowRecord, reduce};

use super::{ApplyError, BatchItem, BatchRejection, GroupView, ScopedPlan};

/// Owned ordered successes/errors; later successes see preceding successful writes.
#[derive(Debug, Eq, PartialEq)]
pub struct IndependentTransitionPlan {
    results: Vec<Result<ScopedPlan, DomainError>>,
}

impl IndependentTransitionPlan {
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
pub enum IndependentTransitionDecision {
    /// Complete ordered independent results, including individual domain errors.
    Accepted(IndependentTransitionPlan),
    /// No provisional result or write is exposed.
    Rejected(BatchRejection),
}

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
    if let Some(error) = preflight(view, items, encoded_entry, limits)? {
        return Ok(IndependentTransitionDecision::Rejected(error));
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
            return Ok(IndependentTransitionDecision::Rejected(
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
    Ok(IndependentTransitionDecision::Accepted(
        IndependentTransitionPlan { results },
    ))
}

fn preflight(
    view: GroupView<'_>,
    items: &[BatchItem<'_>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
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
        .any(|item| !matches!(item.command.operation, Operation::Transition(_)))
    {
        return Ok(Some(BatchRejection::Domain(
            DomainError::UnsupportedOperation,
        )));
    }
    Ok(charge(0, items.len(), limits)
        .err()
        .map(BatchRejection::Admission))
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
