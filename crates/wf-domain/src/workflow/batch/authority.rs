//! Closed borrowed current-record and retry sources, preserving reached reads.

use wf_types::identity::{GroupId, PartitionId};

use crate::claim::{ClaimRecord, ScopedRecordId};
use crate::policy::InstalledRetryPolicy;
use crate::workflow::indexed::TouchedIndexedGroupView;
use crate::workflow::{Operation, WorkflowRecord};

use super::{ApplyError, BatchItem, GroupView};

#[derive(Clone, Copy)]
pub(in crate::workflow) enum BatchAuthority<'query, 'current> {
    Full(GroupView<'current>),
    Touched(TouchedIndexedGroupView<'query, 'current>),
}

impl<'current> BatchAuthority<'_, 'current> {
    pub(in crate::workflow) fn group(&self) -> &GroupId {
        match self {
            Self::Full(view) => view.group,
            Self::Touched(view) => view.group,
        }
    }

    pub(in crate::workflow) fn placement(&self, partition: &PartitionId) -> Option<&GroupId> {
        match self {
            Self::Full(view) => view.placements.get(partition),
            Self::Touched(view) => view.placements.get(partition),
        }
    }

    pub(in crate::workflow) fn record(
        &self,
        id: ScopedRecordId<'_>,
    ) -> Result<Option<&'current WorkflowRecord>, ApplyError> {
        match self {
            Self::Full(view) => Ok(view.records.get(&id).copied()),
            Self::Touched(view) => Ok(observed_current(*view, id)?.map(|entry| entry.record)),
        }
    }

    pub(in crate::workflow) fn retry_policy<'policy>(
        &'policy self,
        item: BatchItem<'policy>,
        record: Option<&WorkflowRecord>,
    ) -> Result<Option<&'policy InstalledRetryPolicy>, ApplyError> {
        match self {
            Self::Full(_) => Ok(item.retry_policy),
            Self::Touched(view) => {
                let Some(record) = record else {
                    return Ok(None);
                };
                if !matches!(item.command.operation, Operation::RetryWithPolicy(_)) {
                    return Ok(None);
                }
                view.retry_policies
                    .get(record.workflow_type())
                    .copied()
                    .ok_or(ApplyError::InvalidAuthority)
            }
        }
    }
}

pub(in crate::workflow) fn observed_current<'current>(
    view: TouchedIndexedGroupView<'_, 'current>,
    id: ScopedRecordId<'_>,
) -> Result<Option<ClaimRecord<'current>>, ApplyError> {
    let current = view
        .current
        .get(&id)
        .copied()
        .ok_or(ApplyError::InvalidAuthority)?;
    if current.is_some_and(|entry| ScopedRecordId::for_record(&entry) != id) {
        return Err(ApplyError::InvalidAuthority);
    }
    Ok(current)
}
