//! Borrowed postimage comparison after complete hard grammar validation.
use super::{CodecError as E, NormalizedClaimDue, TimestampMillis};
use crate::log::{CodecLimits, reader::Reader};
use wf_domain::{
    claim::{caller::ClaimPartitionMode, coordination::ClaimPhase},
    workflow::admission::AdmissionLimits,
};

pub(super) fn verify(
    body: &[u8],
    time: TimestampMillis,
    call: &NormalizedClaimDue<'_>,
    phase: ClaimPhase,
    admission: AdmissionLimits,
) -> Result<u64, E> {
    let mut input = Reader::new(body, CodecLimits::new(admission));
    input.take(17)?;
    let count = input.u32()?;
    let deadline = time
        .get()
        .checked_add(call.lease_ms())
        .ok_or(E::Malformed)?;
    for _ in 0..count {
        let partition = input.raw(65535)?;
        input.raw(65535)?;
        let kind = input.raw(admission.inline_bytes())?;
        let state = input.raw(admission.inline_bytes())?;
        let worker = input.raw(admission.inline_bytes())?;
        input.raw(admission.inline_bytes())?;
        input.take(16)?;
        let end = input.u64()?;
        input.take(24)?;
        let priority = input.byte()?;
        input.take(16)?;
        for _ in 0..3 {
            input.optional(|input| input.raw(admission.inline_bytes()))?;
        }
        let (mode, keys) = call.partitions();
        let selected = match mode {
            ClaimPartitionMode::Any => true,
            ClaimPartitionMode::Only => keys.contains(&partition),
            ClaimPartitionMode::Auto | ClaimPartitionMode::Global => return Err(E::UnsupportedTag),
        };
        let (any, names) = call.states();
        let state_matches = phase == ClaimPhase::Running || (any || names.contains(&state));
        if kind != call.workflow_type()
            || worker != call.worker()
            || end != deadline
            || !selected
            || !state_matches
            || (phase == ClaimPhase::Normal && state == b"running")
            || call.priority().is_some_and(|p| p != u64::from(priority))
        {
            return Err(E::Malformed);
        }
    }
    input.finish()?;
    Ok(u64::from(count))
}
