//! Fresh-only canonical indexed batch composition after retained request inspection.

use super::DispatchPlan;
use super::dispatch::after_gate;
use super::gate::{CommittedHeader, GateView, inspect_request};
use crate::workflow::admission::{AdmissionError, AdmissionLimits};
use crate::workflow::batch::{ApplyError, BatchRejection};
use crate::workflow::indexed::{
    EntryOrder, IndexRejection, IndexedBatchItem, IndexedDecision, IndexedGroupView,
    decide_indexed_batch,
};

/// Coherent request authority and current canonical indexed group authority.
#[derive(Clone, Copy, Debug)]
pub struct IndexedRequestView<'a> {
    /// Original retained/control namespace authority.
    pub gate: GateView<'a>,
    /// Coherent group metadata, records, modes and correctness indexes.
    pub group: IndexedGroupView<'a>,
}
/// Explicit private committed header, bounded items and sequence ordinal.
#[derive(Clone, Copy, Debug)]
pub struct IndexedEnvelope<'a> {
    /// Required trusted stamped original request header.
    pub header: CommittedHeader<'a>,
    /// Immutable same-group atomic items in meaningful input order.
    pub items: &'a [IndexedBatchItem<'a>],
    /// Next available sequence offset after preceding subcommands in this entry.
    pub first_ordinal: u32,
}
/// Scoped replay or complete fresh canonical record/index semantic decision.
pub type IndexedRequestPlan<'a> = DispatchPlan<'a, IndexedDecision<'a>>;

/// Dispatches an indexed batch only for a new admissible scoped request.
///
/// # Errors
/// Operational authority/time/identity/resource failure produces no control plan.
pub fn decide_indexed_request<'a>(
    view: IndexedRequestView<'a>,
    envelope: IndexedEnvelope<'a>,
    limits: AdmissionLimits,
) -> Result<IndexedRequestPlan<'a>, ApplyError> {
    if view.group.batch.group != view.gate.scope.group() {
        return Err(ApplyError::InvalidAuthority);
    }
    let gate = inspect_request(view.gate, envelope.header, limits)?;
    after_gate(gate, |now| {
        if envelope.items.is_empty() || envelope.items.len() > limits.batch_items() {
            return Ok(IndexedDecision::Rejected(IndexRejection::Batch(
                BatchRejection::Admission(AdmissionError::BatchItems),
            )));
        }
        if envelope
            .items
            .iter()
            .any(|item| item.item.command.now != now)
        {
            return Err(ApplyError::InvalidAuthority);
        }
        decide_indexed_batch(
            view.group,
            envelope.items,
            EntryOrder {
                identity: envelope.header.identity.log(),
                first_ordinal: envelope.first_ordinal,
            },
            envelope.header.encoded_entry,
            limits,
        )
    })
}
