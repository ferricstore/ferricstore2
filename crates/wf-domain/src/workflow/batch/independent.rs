//! Ordered independent mutations over borrowed current and earlier owned writes.

use std::collections::BTreeMap;

use crate::claim::ScopedRecordId;
use crate::workflow::admission::{
    AdmissionError, AdmissionLimits, add, admit_command, record_bytes,
};
use crate::workflow::{DomainError, Operation, TransitionPlan, WorkflowRecord, reduce};

use super::{ApplyError, BatchAuthority, BatchItem, BatchRejection, GroupView, ScopedPlan};

#[derive(Clone, Copy)]
pub(in crate::workflow) enum IndependentProfile {
    Transition,
    Terminal,
    Indexed,
}

pub(in crate::workflow) type IndependentStep<E> = Result<Result<TransitionPlan, E>, BatchRejection>;

pub(in crate::workflow) struct IndependentResults<'input, E> {
    pub(in crate::workflow) results: Vec<Result<ScopedPlan, E>>,
    pub(in crate::workflow) latest: BTreeMap<ScopedRecordId<'input>, usize>,
}

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
    decide(
        view,
        items,
        encoded_entry,
        limits,
        IndependentProfile::Transition,
    )
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
    decide(
        view,
        items,
        encoded_entry,
        limits,
        IndependentProfile::Terminal,
    )
}

fn decide(
    view: GroupView<'_>,
    items: &[BatchItem<'_>],
    encoded_entry: &[u8],
    limits: AdmissionLimits,
    profile: IndependentProfile,
) -> Result<IndependentBatchDecision, ApplyError> {
    let view = BatchAuthority::Full(view);
    if let Some(error) = preflight(view, items.iter().copied(), encoded_entry, limits, profile)? {
        return Ok(IndependentBatchDecision::Rejected(error));
    }
    match run(view, items, limits, profile, |_, item, current, _, _| {
        Ok(Ok(reduce(current, item.command)))
    })? {
        Ok(fold) => Ok(IndependentBatchDecision::Accepted(IndependentBatchPlan {
            results: fold.results,
        })),
        Err(error) => Ok(IndependentBatchDecision::Rejected(error)),
    }
}

pub(in crate::workflow) fn run<'input, E>(
    view: BatchAuthority<'_, '_>,
    items: &[BatchItem<'input>],
    limits: AdmissionLimits,
    profile: IndependentProfile,
    mut evaluate: impl FnMut(
        usize,
        BatchItem<'input>,
        Option<&WorkflowRecord>,
        Option<usize>,
        &mut usize,
    ) -> Result<IndependentStep<E>, ApplyError>,
) -> Result<Result<IndependentResults<'input, E>, BatchRejection>, ApplyError> {
    if let Err(error) = charge(0, items.len(), limits, profile) {
        return Ok(Err(BatchRejection::Admission(error)));
    }
    let mut fold = IndependentResults {
        results: Vec::new(),
        latest: BTreeMap::new(),
    };
    fold.results
        .try_reserve_exact(items.len())
        .map_err(|_| ApplyError::AllocationFailed)?;
    let mut variable_bytes = 0;
    for (index, item) in items.iter().enumerate() {
        let id = ScopedRecordId {
            partition_key: item.partition.as_bytes(),
            id: item.command.id.as_bytes(),
        };
        let previous = fold.latest.get(&id).copied();
        let current = current_record(view, id, previous, &fold.results)?;
        if current.is_some_and(|record| record.id() != &item.command.id) {
            return Err(ApplyError::InvalidAuthority);
        }
        if let Some(error) = admit_step(
            *item,
            current,
            &mut variable_bytes,
            items.len(),
            limits,
            profile,
        )? {
            return Ok(Err(BatchRejection::Admission(error)));
        }
        let result = match evaluate(index, *item, current, previous, &mut variable_bytes)? {
            Ok(result) => result,
            Err(error) => return Ok(Err(error)),
        }
        .map(|plan| ScopedPlan {
            partition: item.partition.clone(),
            plan,
        });
        let changed = result
            .as_ref()
            .is_ok_and(|plan| plan.plan().write().is_some());
        fold.results.push(result);
        if changed {
            fold.latest.insert(id, index);
        }
    }
    Ok(Ok(fold))
}

pub(in crate::workflow) fn preflight<'input>(
    view: BatchAuthority<'_, '_>,
    mut items: impl ExactSizeIterator<Item = BatchItem<'input>> + Clone,
    encoded_entry: &[u8],
    limits: AdmissionLimits,
    profile: IndependentProfile,
) -> Result<Option<BatchRejection>, ApplyError> {
    let count = items.len();
    if count > limits.batch_items() {
        return Ok(Some(BatchRejection::Admission(AdmissionError::BatchItems)));
    }
    if let Err(error) = limits.check_total(encoded_entry.len()) {
        return Ok(Some(BatchRejection::Admission(error)));
    }
    for item in items.clone() {
        let group = view
            .placement(item.partition)
            .ok_or(ApplyError::InvalidAuthority)?;
        if group != view.group() {
            return Ok(Some(BatchRejection::CrossGroup));
        }
    }
    if items.any(|item| !allowed_operation(&item.command.operation, profile)) {
        return Ok(Some(BatchRejection::Domain(
            DomainError::UnsupportedOperation,
        )));
    }
    Ok(charge(0, count, limits, profile)
        .err()
        .map(BatchRejection::Admission))
}

fn allowed_operation(operation: &Operation, profile: IndependentProfile) -> bool {
    match profile {
        IndependentProfile::Terminal => matches!(
            operation,
            Operation::Complete(_)
                | Operation::Fail(_)
                | Operation::Retry(_)
                | Operation::Cancel(_)
        ),
        IndependentProfile::Transition => matches!(operation, Operation::Transition(_)),
        IndependentProfile::Indexed => matches!(
            operation,
            Operation::Create(_)
                | Operation::Transition(_)
                | Operation::Complete(_)
                | Operation::Fail(_)
                | Operation::Retry(_)
                | Operation::RetryWithPolicy(_)
                | Operation::ExtendLease(_)
                | Operation::Cancel(_)
        ),
    }
}

fn current_record<'row, E>(
    view: BatchAuthority<'_, 'row>,
    id: ScopedRecordId<'_>,
    previous: Option<usize>,
    results: &'row [Result<ScopedPlan, E>],
) -> Result<Option<&'row WorkflowRecord>, ApplyError> {
    match previous {
        Some(index) => results
            .get(index)
            .and_then(|result| result.as_ref().ok())
            .and_then(|plan| plan.plan().write())
            .map(|write| Some(&write.record))
            .ok_or(ApplyError::InvalidAuthority),
        None => view.record(id),
    }
}

fn admit_step(
    item: BatchItem<'_>,
    current: Option<&WorkflowRecord>,
    variable_bytes: &mut usize,
    count: usize,
    limits: AdmissionLimits,
    profile: IndependentProfile,
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
        charge(*variable_bytes, count, limits, profile)
    })();
    Ok(attempt.err())
}

pub(in crate::workflow) fn charge(
    variable_bytes: usize,
    count: usize,
    limits: AdmissionLimits,
    profile: IndependentProfile,
) -> Result<(), AdmissionError> {
    let mut bytes = limits.charge(variable_bytes, count)?;
    add(
        &mut bytes,
        count
            .checked_mul(if matches!(profile, IndependentProfile::Indexed) {
                1_664
            } else {
                128
            })
            .ok_or(AdmissionError::LengthOverflow)?,
    )?;
    limits.check_total(bytes)
}
