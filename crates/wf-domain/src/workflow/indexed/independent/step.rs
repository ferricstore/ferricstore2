//! Per-item admission and index rules before accepting a virtual predecessor.

use crate::claim::ClaimRecord;
use crate::policy::definition::StateMode;
use crate::workflow::batch::{
    ApplyError, BatchRejection, IndependentProfile, IndependentStep, admit_retry_policy,
    charge_independent,
};
use crate::workflow::{Operation, WorkflowRecord, reduce_profile};

use super::super::decision::{next_ordering, validate};
use super::super::{IndexRejection, IndexedBatchItem};
use super::IndependentIndexState;

impl<'current> IndependentIndexState<'_, 'current> {
    pub(super) fn evaluate(
        &mut self,
        item: IndexedBatchItem<'_>,
        current: Option<&WorkflowRecord>,
        previous: Option<usize>,
        bytes: &mut usize,
    ) -> Result<IndependentStep<IndexRejection>, ApplyError> {
        let batch = self.authority.batch();
        let retry_policy = batch.retry_policy(item.item, current)?;
        if matches!(item.item.command.operation, Operation::RetryWithPolicy(_))
            && let (Some(record), Some(policy)) = (current, retry_policy)
        {
            admit_retry_policy(record, policy, self.limits, bytes)?;
        }
        if previous.is_none()
            && let Some(error) = validate(self.authority, item, self.limits, bytes)?
        {
            return Ok(self.refusal(error));
        }
        if let Err(error) = charge_independent(*bytes, self.count, self.limits, self.profile) {
            return Ok(Err(BatchRejection::Admission(error)));
        }
        let (before, before_mode) = self.before(item, current, previous)?;
        let plan = match reduce_profile(
            current,
            item.item.command,
            retry_policy,
            matches!(self.profile, IndependentProfile::Public),
        ) {
            Ok(plan) => plan,
            Err(error) => {
                return Ok(self.refusal(IndexRejection::Batch(BatchRejection::Domain(error))));
            }
        };
        let (after, after_mode, consumed) = match next_ordering(
            self.authority,
            item,
            before,
            before_mode,
            &plan,
            self.order,
            self.consumed,
        )? {
            Ok(values) => values,
            Err(error) => return Ok(self.refusal(error)),
        };
        self.ordering
            .push(plan.write().map(|_| (after, after_mode)));
        self.consumed = consumed;
        Ok(Ok(Ok(plan)))
    }

    fn before<'record>(
        &self,
        item: IndexedBatchItem<'record>,
        current: Option<&'record WorkflowRecord>,
        previous: Option<usize>,
    ) -> Result<(Option<ClaimRecord<'record>>, StateMode), ApplyError>
    where
        'current: 'record,
    {
        if let Some(index) = previous {
            let (metadata, mode) = self
                .ordering
                .get(index)
                .copied()
                .flatten()
                .ok_or(ApplyError::InvalidAuthority)?;
            let record = current.ok_or(ApplyError::InvalidAuthority)?;
            return Ok((
                Some(ClaimRecord {
                    record,
                    partition_key: item.item.partition.as_bytes(),
                    priority: metadata.priority,
                    state_enter_seq: metadata.state_enter_seq,
                }),
                mode,
            ));
        }
        let before = self.authority.current(item.item)?;
        let mode = before.map_or(Ok(StateMode::Parallel), |entry| {
            self.authority.mode(entry.record)
        })?;
        Ok((before, mode))
    }

    fn refusal(&mut self, error: IndexRejection) -> IndependentStep<IndexRejection> {
        if let IndexRejection::Batch(BatchRejection::Admission(error)) = error {
            return Err(BatchRejection::Admission(error));
        }
        self.ordering.push(None);
        Ok(Err(error))
    }
}
