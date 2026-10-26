//! Shared Fresh reference staging, with explicit complete or observed authority.

use std::collections::BTreeMap;

use wf_domain::claim::ScopedRecordId;
use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::workflow::batch::{ApplyError, BatchItem};
use wf_domain::workflow::indexed::{
    EntryOrder, IndependentIndexedDecision, IndexedBatchItem, IndexedDecision, IndexedGroupView,
    TouchedIndexedGroupView, decide_indexed_batch, decide_public_mutation_touched_indexed_batch,
    decide_touched_independent_indexed_batch, decide_touched_indexed_batch,
};
use wf_types::identity::GroupId;
use wf_types::version::TimestampMillis;

use crate::blob::value_effects_bounds;
use crate::consensus::{PreparedBatchItem, PreparedOperation};
use crate::log::{CodecError, CodecLimits, DecodedRequest};

use super::{
    RecordBatchApplyError, admit, decoder_bound, effects_apply_error, product, reserve, sum,
};

type ValueRow<'a> = ([&'a [u8]; 2], usize);

#[derive(Clone, Copy)]
pub(super) enum FreshAuthority<'query, 'current> {
    Full {
        view: IndexedGroupView<'current>,
        retry_policies: &'query BTreeMap<&'query [u8], &'query InstalledRetryPolicy>,
    },
    Touched(TouchedIndexedGroupView<'query, 'current>),
}

impl<'current> FreshAuthority<'_, 'current> {
    pub(super) fn group(&self) -> &GroupId {
        match self {
            Self::Full { view, .. } => view.batch.group,
            Self::Touched(view) => view.group,
        }
    }

    fn decide(
        self,
        items: &[IndexedBatchItem<'_>],
        order: EntryOrder,
        encoded: &[u8],
        limits: CodecLimits,
    ) -> Result<IndexedDecision<'current>, ApplyError> {
        match self {
            Self::Full { view, .. } => {
                decide_indexed_batch(view, items, order, encoded, limits.admission)
            }
            Self::Touched(view) => {
                decide_touched_indexed_batch(view, items, order, encoded, limits.admission)
            }
        }
    }

    fn retry_policy(&self, prepared: &PreparedBatchItem) -> Option<&InstalledRetryPolicy> {
        match self {
            Self::Full {
                view,
                retry_policies,
            } if matches!(
                prepared.command.operation,
                wf_domain::workflow::Operation::RetryWithPolicy(_)
            ) =>
            {
                let key = ScopedRecordId {
                    partition_key: prepared.partition.as_bytes(),
                    id: prepared.command.id.as_bytes(),
                };
                view.batch
                    .records
                    .get(&key)
                    .and_then(|record| retry_policies.get(record.workflow_type()))
                    .copied()
            }
            Self::Full { .. } | Self::Touched(_) => None,
        }
    }
}

pub(super) fn decide<'current>(
    decoded: &DecodedRequest<'_>,
    authority: FreshAuthority<'_, 'current>,
    order: EntryOrder,
    now: TimestampMillis,
    limits: CodecLimits,
    workspace: usize,
    additional_live_bytes: usize,
) -> Result<IndexedDecision<'current>, RecordBatchApplyError> {
    let prepared = inputs(decoded, now, limits, workspace, additional_live_bytes)?;
    let values = stage_values(decoded, prepared.len())?;
    let mut items = stage_items(decoded, prepared, &values)?;
    for (item, prepared) in items.iter_mut().zip(prepared) {
        item.item.retry_policy = authority.retry_policy(prepared);
    }
    Ok(authority.decide(&items, order, decoded.encoded, limits)?)
}

pub(in crate::log) fn decide_independent<'current>(
    decoded: &DecodedRequest<'_>,
    view: TouchedIndexedGroupView<'_, 'current>,
    order: EntryOrder,
    now: TimestampMillis,
    workspace: usize,
    additional_live_bytes: usize,
) -> Result<IndependentIndexedDecision<'current>, RecordBatchApplyError> {
    let prepared = inputs(
        decoded,
        now,
        decoded.limits,
        workspace,
        additional_live_bytes,
    )?;
    let values = stage_values(decoded, prepared.len())?;
    let items = stage_items(decoded, prepared, &values)?;
    let decide = if matches!(
        decoded.request.operation,
        PreparedOperation::PublicMutation { .. }
    ) {
        decide_public_mutation_touched_indexed_batch
    } else {
        decide_touched_independent_indexed_batch
    };
    Ok(decide(
        view,
        &items,
        order,
        decoded.encoded,
        decoded.limits.admission,
    )?)
}

fn inputs<'a>(
    decoded: &'a DecodedRequest<'_>,
    now: TimestampMillis,
    limits: CodecLimits,
    workspace: usize,
    additional_live_bytes: usize,
) -> Result<&'a [PreparedBatchItem], RecordBatchApplyError> {
    let prepared = decoded
        .request
        .operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?;
    let now = if let PreparedOperation::PublicMutation { evidence, .. } = &decoded.request.operation
    {
        evidence
            .requested_now_ms
            .map_or(decoded.request.stamp.hlc().physical(), TimestampMillis::new)
    } else {
        now
    };
    if prepared.iter().any(|item| item.command.now != now) {
        return Err(ApplyError::InvalidAuthority.into());
    }
    let bound = sum(&[
        decoder_bound(decoded.encoded.len(), prepared.len(), limits)?,
        product(prepared.len(), 192)?,
        256,
        limits.admission.decision_bytes(),
    ])?;
    let bound = sum(&[bound, additional_live_bytes])?;
    admit(bound, workspace)?;
    admit(
        sum(&[bound, std::mem::size_of::<DecodedRequest<'static>>()])?,
        workspace,
    )?;
    Ok(prepared)
}

pub(in crate::log) fn value_effects_budget(
    decoded: &DecodedRequest<'_>,
    workspace_limit: usize,
    additional_live_bytes: usize,
) -> Result<(usize, usize, usize), RecordBatchApplyError> {
    let items = decoded
        .request
        .operation
        .mutation_items()
        .ok_or(CodecError::UnsupportedTag)?;
    let (_, work) = value_effects_bounds(decoded.encoded.len(), items.len())
        .map_err(|error| effects_apply_error(&(error, None)))?;
    let live = sum(&[
        decoder_bound(decoded.encoded.len(), items.len(), decoded.limits)?,
        product(items.len(), 192)?,
        256,
        decoded.limits.admission.decision_bytes(),
        additional_live_bytes,
        std::mem::size_of::<DecodedRequest<'static>>(),
    ])?;
    Ok((live, workspace_limit, work))
}

fn stage_values<'a>(
    decoded: &'a DecodedRequest<'_>,
    count: usize,
) -> Result<Vec<ValueRow<'a>>, CodecError> {
    let mut values = reserve(count)?;
    for index in 0..count {
        let mut row: ValueRow<'_> = ([&[]; 2], 0);
        for value in decoded
            .value_bytes(index)
            .ok_or(CodecError::Malformed)?
            .into_iter()
            .flatten()
        {
            row.0[row.1] = value;
            row.1 += 1;
        }
        values.push(row);
    }
    Ok(values)
}

fn stage_items<'a>(
    decoded: &'a DecodedRequest<'_>,
    prepared: &'a [PreparedBatchItem],
    values: &'a [ValueRow<'a>],
) -> Result<Vec<IndexedBatchItem<'a>>, CodecError> {
    let mut items = reserve(prepared.len())?;
    for (index, (prepared, row)) in prepared.iter().zip(values).enumerate() {
        items.push(IndexedBatchItem {
            item: BatchItem {
                partition: &prepared.partition,
                command: &prepared.command,
                state_key: decoded.state_key(index).ok_or(CodecError::Malformed)?,
                encoded_values: &row.0[..row.1],
                retry_policy: None,
            },
            create_priority: prepared.create_priority,
        });
    }
    Ok(items)
}
