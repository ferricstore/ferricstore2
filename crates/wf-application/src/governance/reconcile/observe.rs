//! One original-partition batch at a time; no complete record/history owners accumulate.
use super::{ReleaseReconcileLimits as L, model::ReleaseStatus as S};
use wf_domain::{
    governance::release::{ReleaseDisposition as D, ReleaseIntentRef, classify_release},
    workflow::request::RequestScope,
};
use wf_ports::state::{WorkflowReleasePage, WorkflowReleaseReadPort, WorkflowReleaseRecord};

pub(super) fn records<P: WorkflowReleaseReadPort>(
    port: &mut P,
    scope: &RequestScope,
    page: &WorkflowReleasePage,
    intents: &[ReleaseIntentRef<'_>],
    decisions: &mut [S; 256],
    limits: L,
) -> u16 {
    let mut order: [usize; 256] = std::array::from_fn(|i| i);
    let mut length = 0;
    for (at, entry) in page.entries.iter().enumerate() {
        if entry.completed {
            decisions[at] = S::Completed;
        } else {
            order[length] = at;
            length += 1;
        }
    }
    order[..length].sort_unstable_by(|a, b| {
        intents[*a]
            .partition
            .cmp(&intents[*b].partition)
            .then(a.cmp(b))
    });
    let mut begin = 0;
    let mut batches = 0;
    while begin < length {
        let partition = intents[order[begin]].partition;
        let end = begin
            + order[begin..length]
                .iter()
                .take_while(|i| intents[**i].partition == partition)
                .count();
        let mut ids = [&[][..]; 256];
        for (slot, index) in ids.iter_mut().zip(&order[begin..end]) {
            *slot = intents[*index].flow_id;
        }
        batches += 1;
        if let Ok(records) = port.workflow_release_records_for_release(
            scope,
            partition,
            &ids[..end - begin],
            limits.read,
        ) && records.len() == end - begin
            && valid_records(&records, limits)
        {
            for (record, index) in records.iter().zip(&order[begin..end]) {
                decisions[*index] =
                    match classify_release(intents[*index].reservation_id, record.observation()) {
                        D::Retain => S::Retained,
                        D::Release => S::Release,
                        D::ReadError => S::Error,
                    };
            }
        }
        begin = end;
    }
    batches
}
fn valid_records(records: &[WorkflowReleaseRecord], limits: L) -> bool {
    records
        .iter()
        .try_fold(128_usize, |n, r| {
            let dynamic = match r {
                WorkflowReleaseRecord::Record {
                    state,
                    reservation_id,
                } => {
                    if state.as_ref().is_some_and(|v| v.len() > 65535)
                        || reservation_id
                            .as_ref()
                            .is_some_and(|v| v.is_empty() || v.len() > 256)
                    {
                        return None;
                    }
                    state
                        .as_ref()
                        .map_or(0, Vec::len)
                        .checked_add(reservation_id.as_ref().map_or(0, Vec::len))?
                }
                _ => 0,
            };
            n.checked_add(size_of::<WorkflowReleaseRecord>())?
                .checked_add(dynamic)
        })
        .is_some_and(|n| n <= limits.read.maximum_bytes)
}
