//! A run of after-images holding the same payload, logged with it once
//! (perf note P1), as Elixir logs a batch's shared payload once per
//! command: `RUN`, the payload, the count, then each after-image's
//! partition and its image with the payload field written as
//! [`image::SHARED_PAYLOAD`]. It reads back as a group of after-images, so
//! replay and replicas apply what the shard did.

use wf_application::commands::queue::Partition;

use super::super::image;
use super::{Logged, Malformed, RUN, Reader, bin};

/// The round's changes as log records, each run of after-images holding
/// one payload as one record of at most `max_bytes` (a single change's
/// record may be larger, as before).
pub(crate) fn records(logged: &[Logged], max_bytes: usize) -> Option<Vec<Vec<u8>>> {
    let mut records = Vec::with_capacity(logged.len());
    let mut rest = logged;
    while let Some((first, tail)) = rest.split_first() {
        let length = run(rest, max_bytes);
        if length > 1 {
            let (shared, after) = rest.split_at_checked(length)?;
            records.push(encode(shared)?);
            rest = after;
        } else {
            records.push(first.encode()?);
            rest = tail;
        }
    }
    Some(records)
}

/// A change's payload when it is an after-image holding one.
fn payload(change: &Logged) -> Option<&[u8]> {
    match change {
        Logged::Put { image, .. } => image::payload_split(image).map(|(_, payload, _)| payload),
        _ => None,
    }
}

/// How many changes from the first hold its payload, their record within
/// `max_bytes`.
fn run(changes: &[Logged], max_bytes: usize) -> usize {
    let Some(shared) = changes.first().and_then(payload) else {
        return 1;
    };
    let mut bytes = shared.len().saturating_add(9);
    let mut length = 0_usize;
    for change in changes {
        let Logged::Put { partition, image } = change else {
            break;
        };
        if payload(change) != Some(shared) {
            break;
        }
        let size = image
            .len()
            .saturating_sub(shared.len())
            .saturating_add(partition.name().len())
            .saturating_add(8);
        if length > 0 && bytes.saturating_add(size) > max_bytes {
            break;
        }
        bytes = bytes.saturating_add(size);
        length = length.saturating_add(1);
    }
    length.max(1)
}

fn encode(run: &[Logged]) -> Option<Vec<u8>> {
    let shared = run.first().and_then(payload)?;
    let mut out = vec![RUN];
    bin(&mut out, shared)?;
    out.extend_from_slice(&u32::try_from(run.len()).ok()?.to_be_bytes());
    for change in run {
        let Logged::Put { partition, image } = change else {
            return None;
        };
        let (head, _, tail) = image::payload_split(image)?;
        bin(&mut out, &partition.name())?;
        let length = head.len().saturating_add(1).saturating_add(tail.len());
        out.extend_from_slice(&u32::try_from(length).ok()?.to_be_bytes());
        out.extend_from_slice(head);
        out.push(image::SHARED_PAYLOAD);
        out.extend_from_slice(tail);
    }
    Some(out)
}

/// A run's after-images, payload put back.
pub(super) fn decode(reader: &mut Reader<'_>) -> Result<Logged, Malformed> {
    let shared = reader.bin()?;
    let count = u32::from_be_bytes(reader.array()?);
    let mut changes = Vec::new();
    for _ in 0..count {
        let partition = Partition::parse(reader.bin()?);
        let image = image::with_payload(reader.bin()?, shared).ok_or(Malformed)?;
        changes.push(Logged::Put { partition, image });
    }
    if changes.is_empty() {
        return Err(Malformed);
    }
    Ok(Logged::Group(changes))
}

#[cfg(test)]
mod tests;
