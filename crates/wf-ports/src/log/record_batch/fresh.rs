//! Shared Fresh reference staging, with explicit complete or observed authority.

use std::collections::BTreeMap;

use wf_domain::claim::ScopedRecordId;
use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::workflow::batch::{ApplyError, BatchItem};
use wf_domain::workflow::indexed::{
    EntryOrder, IndexedBatchItem, IndexedDecision, IndexedGroupView, TouchedIndexedGroupView,
    decide_indexed_batch, decide_touched_indexed_batch,
};
use wf_types::identity::GroupId;
use wf_types::version::TimestampMillis;

use crate::consensus::{PreparedBatchItem, PreparedOperation};
use crate::log::{CodecError, CodecLimits, DecodedRequest};

use super::{RecordBatchApplyError, admit, decoder_bound, product, reserve, sum};

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
    let PreparedOperation::Batch(prepared) = &decoded.request.operation else {
        return Err(CodecError::UnsupportedTag.into());
    };
    if prepared.iter().any(|item| item.command.now != now) {
        return Err(ApplyError::InvalidAuthority.into());
    }
    let count = prepared.len();
    let bound = sum(&[
        decoder_bound(decoded.encoded.len(), count, limits)?,
        product(count, 192)?,
        256,
        limits.admission.decision_bytes(),
    ])?;
    admit(sum(&[bound, additional_live_bytes])?, workspace)?;
    let mut values: Vec<ValueRow<'_>> = reserve(count)?;
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
    let mut items = reserve(count)?;
    for (index, (prepared, row)) in prepared.iter().zip(&values).enumerate() {
        items.push(IndexedBatchItem {
            item: BatchItem {
                partition: &prepared.partition,
                command: &prepared.command,
                state_key: decoded.state_key(index).ok_or(CodecError::Malformed)?,
                encoded_values: &row.0[..row.1],
                retry_policy: authority.retry_policy(prepared),
            },
            create_priority: prepared.create_priority,
        });
    }
    Ok(authority.decide(&items, order, decoded.encoded, limits)?)
}
