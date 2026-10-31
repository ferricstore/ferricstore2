//! Fixed-depth named containers; leaves are borrowed and never sorted or copied.
use super::{
    CodecError as E, IntentSink, NormalizedClaimDue, fields::boolean, header, map, scalar,
};
use wf_domain::claim::caller::{ClaimPartitionMode, ClaimValuesMode};

pub(super) fn states(caller: &NormalizedClaimDue<'_>, out: &mut IntentSink<'_>) -> Result<(), E> {
    let (any, names) = caller.states();
    map(2, out, |out| {
        boolean("any", any, out)?;
        scalar(5, b"names", out)?;
        list(names, out)
    })
}

pub(super) fn partitions(
    caller: &NormalizedClaimDue<'_>,
    out: &mut IntentSink<'_>,
) -> Result<(), E> {
    let (mode, names) = caller.partitions();
    let mode = match mode {
        ClaimPartitionMode::Auto => b"auto".as_slice(),
        ClaimPartitionMode::Any => b"any",
        ClaimPartitionMode::Global => b"global",
        ClaimPartitionMode::Only => b"only",
    };
    map(2, out, |out| {
        scalar(5, b"keys", out)?;
        list(names, out)?;
        scalar(5, b"mode", out)?;
        scalar(5, mode, out)
    })
}

pub(super) fn values(caller: &NormalizedClaimDue<'_>, out: &mut IntentSink<'_>) -> Result<(), E> {
    let (mode, names) = caller.values();
    let mode = match mode {
        ClaimValuesMode::Omitted => b"omitted".as_slice(),
        ClaimValuesMode::All => b"all",
        ClaimValuesMode::Only => b"only",
    };
    map(2, out, |out| {
        scalar(5, b"mode", out)?;
        scalar(5, mode, out)?;
        scalar(5, b"names", out)?;
        list(names, out)
    })
}

fn list(names: &[&[u8]], out: &mut IntentSink<'_>) -> Result<(), E> {
    let size = names
        .iter()
        .try_fold(8_usize, |n, bytes| {
            n.checked_add(9).and_then(|n| n.checked_add(bytes.len()))
        })
        .ok_or(E::LengthOverflow)?;
    header(7, size, out)?;
    out(&u64::try_from(names.len())
        .map_err(|_| E::LengthOverflow)?
        .to_be_bytes())?;
    for name in names {
        scalar(6, name, out)?;
    }
    Ok(())
}
