//! The holdings section of a shard image (audit gap G10), after the stubs:
//! a `u64` count, then per holding Flow its partition, its id, a `u64`
//! reference count and each reference it holds to a value it does not own.

use wf_application::commands::queue::Partition;

use super::super::command::{Malformed, Reader, bin};

/// One holding Flow: its partition, id and held references.
pub(crate) struct Holding {
    pub(crate) partition: Partition,
    pub(crate) id: Vec<u8>,
    pub(crate) refs: Vec<Vec<u8>>,
}

/// A holding to image: the Flow's partition and id, and its references.
pub(crate) type HoldingEntry<'a> = ((&'a Partition, &'a [u8]), &'a [Vec<u8>]);

/// Writes the holdings section.
pub(crate) fn encode(out: &mut Vec<u8>, holdings: &[HoldingEntry<'_>]) -> Option<()> {
    out.extend_from_slice(&u64::try_from(holdings.len()).ok()?.to_be_bytes());
    for ((partition, id), refs) in holdings {
        bin(out, &partition.name())?;
        bin(out, id)?;
        out.extend_from_slice(&u64::try_from(refs.len()).ok()?.to_be_bytes());
        for reference in *refs {
            bin(out, reference)?;
        }
    }
    Some(())
}

/// Reads the holdings section.
pub(super) fn decode(reader: &mut Reader<'_>) -> Result<Vec<Holding>, Malformed> {
    let count = reader.u64()?;
    let mut holdings = Vec::new();
    for _ in 0..count {
        let partition = Partition::parse(reader.bin()?);
        let id = reader.bin()?.to_vec();
        let mut refs = Vec::new();
        for _ in 0..reader.u64()? {
            refs.push(reader.bin()?.to_vec());
        }
        holdings.push(Holding {
            partition,
            id,
            refs,
        });
    }
    Ok(holdings)
}
