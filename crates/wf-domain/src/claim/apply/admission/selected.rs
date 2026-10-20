//! Selected actual owners and complete nested plan/read charges.

use super::ClaimPreflightError;
use crate::claim::apply::token::source_token_length;
use crate::claim::apply::{
    ClaimApplyView, ClaimEncoding, ClaimEnvelope, ClaimQuery, ClaimRejection, ClaimWorkLimits,
};
use crate::claim::{
    ClaimRecord, ClaimSelection, DueOrder, LaneKey, LaneOrder, ScopedRecordId, SelectedClaim,
};
use crate::workflow::admission::{AdmissionError, AdmissionLimits, record_bytes};
use crate::workflow::batch::ApplyError;
use crate::workflow::indexed::IndexedAuthority;
use std::collections::BTreeMap;
use wf_types::version::TimestampMillis;

pub(in super::super) fn preflight_selected(
    view: ClaimApplyView<'_>,
    envelope: &ClaimEnvelope<'_>,
    selection: &ClaimSelection<'_>,
    context: (TimestampMillis, usize),
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
    g: &mut u64,
) -> Result<(), ClaimPreflightError> {
    preflight_selected_with(
        (IndexedAuthority::Full(view.group), view.encodings),
        envelope,
        selection,
        context,
        limits,
        work,
        g,
    )
}

pub(in super::super) fn preflight_selected_with(
    view: (
        IndexedAuthority<'_, '_>,
        &BTreeMap<ScopedRecordId<'_>, ClaimEncoding<'_>>,
    ),
    envelope: &ClaimEnvelope<'_>,
    selection: &ClaimSelection<'_>,
    context: (TimestampMillis, usize),
    limits: AdmissionLimits,
    work: ClaimWorkLimits,
    g: &mut u64,
) -> Result<(), ClaimPreflightError> {
    let (now, mut variable) = context;
    for selected in selection.records() {
        let (b, t) = selected_charge(view, *selected, envelope.query, now, limits)?;
        variable = variable
            .checked_add(b)
            .ok_or(ClaimPreflightError::Business(
                ClaimRejection::LengthOverflow,
            ))?;
        let next = u64::try_from(t).ok().and_then(|n| g.checked_add(n)).ok_or(
            ClaimPreflightError::Business(ClaimRejection::LengthOverflow),
        )?;
        if next > work.max_work_bytes() {
            return Err(ClaimPreflightError::Business(ClaimRejection::WorkBytes {
                bytes: next,
                maximum: work.max_work_bytes(),
            }));
        }
        *g = next;
    }
    let p = limits
        .charge(variable, selection.records().len())
        .map_err(|error| {
            ClaimPreflightError::Business(match error {
                AdmissionError::LengthOverflow => ClaimRejection::LengthOverflow,
                _ => ClaimRejection::DecisionBytes,
            })
        })?;
    let total = u64::try_from(p).ok().and_then(|p| g.checked_add(p)).ok_or(
        ClaimPreflightError::Business(ClaimRejection::LengthOverflow),
    )?;
    if total > work.max_work_bytes() {
        return Err(ClaimPreflightError::Business(ClaimRejection::WorkBytes {
            bytes: total,
            maximum: work.max_work_bytes(),
        }));
    }
    Ok(())
}

fn selected_charge(
    view: (
        IndexedAuthority<'_, '_>,
        &BTreeMap<ScopedRecordId<'_>, ClaimEncoding<'_>>,
    ),
    selected: SelectedClaim<'_>,
    query: ClaimQuery<'_>,
    now: TimestampMillis,
    limits: AdmissionLimits,
) -> Result<(usize, usize), ClaimPreflightError> {
    let invalid = ClaimPreflightError::Apply(ApplyError::InvalidAuthority);
    let (view, encodings) = view;
    let id = ScopedRecordId {
        partition_key: selected.partition_key,
        id: selected.record.id().as_bytes(),
    };
    let entry = view.entry(id).map_err(ClaimPreflightError::Apply)?;
    let canonical = view.canonical(id).map_err(ClaimPreflightError::Apply)?;
    let r = record_bytes(entry.record, limits).map_err(|_| invalid)?;
    let rg = record_bytes(canonical, limits).map_err(|_| invalid)?;
    entry
        .record
        .version()
        .checked_add(1)
        .ok_or(ClaimPreflightError::Business(
            ClaimRejection::VersionOverflow,
        ))?;
    let fence = entry
        .record
        .fence()
        .checked_next()
        .ok_or(ClaimPreflightError::Business(ClaimRejection::FenceOverflow))?;
    let token = source_token_length(query.worker, now, fence)?;
    if token > limits.inline_bytes() {
        return Err(ClaimPreflightError::Business(
            ClaimRejection::ValueTooLarge {
                bytes: token,
                maximum: limits.inline_bytes(),
            },
        ));
    }
    let encoding = encodings.get(&id).ok_or(invalid)?;
    let mut b = rg;
    for bytes in [
        id.id.len(),
        selected.record.state().len(),
        encoding.partition.as_bytes().len(),
        encoding.state_key.len(),
        query.worker.len(),
        token,
    ] {
        b = b.checked_add(bytes).ok_or(ClaimPreflightError::Business(
            ClaimRejection::LengthOverflow,
        ))?;
    }
    let (f, t) = selected_index_charge(view, entry, (r, rg), limits)?;
    let b = b.checked_add(f).ok_or(ClaimPreflightError::Business(
        ClaimRejection::LengthOverflow,
    ))?;
    Ok((b, t))
}

fn selected_index_charge<'current>(
    view: IndexedAuthority<'_, 'current>,
    entry: ClaimRecord<'current>,
    bytes: (usize, usize),
    limits: AdmissionLimits,
) -> Result<(usize, usize), ClaimPreflightError> {
    let invalid = ClaimPreflightError::Apply(ApplyError::InvalidAuthority);
    let (r, rg) = bytes;
    let mut f = 0;
    let lane = LaneKey::for_record(&entry).ok_or(invalid)?;
    let mut t =
        r.checked_add(rg)
            .and_then(|n| n.checked_mul(2))
            .ok_or(ClaimPreflightError::Business(
                ClaimRejection::LengthOverflow,
            ))?;
    let member = if view
        .lane_present(lane)
        .map_err(ClaimPreflightError::Apply)?
    {
        let member = view
            .member(lane, LaneOrder::for_record(&entry))
            .map_err(ClaimPreflightError::Apply)?
            .ok_or(invalid)?;
        let (_, head) = view.head(lane).map_err(ClaimPreflightError::Apply)?;
        let hid = ScopedRecordId::for_record(&head);
        let hi = view.entry(hid).map_err(ClaimPreflightError::Apply)?;
        let hg = view.canonical(hid).map_err(ClaimPreflightError::Apply)?;
        let rh = record_bytes(head.record, limits).map_err(|_| invalid)?;
        let rhi = record_bytes(hi.record, limits).map_err(|_| invalid)?;
        let rhg = record_bytes(hg, limits).map_err(|_| invalid)?;
        f = rh.checked_add(rhg).ok_or(ClaimPreflightError::Business(
            ClaimRejection::LengthOverflow,
        ))?;
        t = t
            .checked_add(rh)
            .and_then(|n| n.checked_add(rhi))
            .and_then(|n| n.checked_add(rh))
            .and_then(|n| n.checked_add(rhg))
            .ok_or(ClaimPreflightError::Business(
                ClaimRejection::LengthOverflow,
            ))?;
        member
    } else {
        view.parallel_entry(DueOrder::for_record(&entry).ok_or(invalid)?, entry)
            .map_err(ClaimPreflightError::Apply)?
    };
    let rm = record_bytes(member.record, limits).map_err(|_| invalid)?;
    t = t
        .checked_add(r)
        .and_then(|n| n.checked_add(rm))
        .ok_or(ClaimPreflightError::Business(
            ClaimRejection::LengthOverflow,
        ))?;
    Ok((f, t))
}
