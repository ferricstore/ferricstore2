//! The kept-on-disk section of a snapshot image (ADR-016 phase D), after
//! the holdings: a `u64` count, then per kept version on disk the Flow's
//! partition and id, the version's kind, name and version, and where its
//! bytes are (file, offset, `u32` length). The Flow's own record carries
//! only the versions in memory ([`super::Kept::Resident`]).

use wf_application::commands::flow::record::RetainedKey;
use wf_application::commands::queue::Partition;
use wf_storage::cold::Location;

use super::super::command::{Malformed, Reader, bin};

/// A kept version on disk as an image restores it.
pub(crate) struct Stored {
    pub(crate) partition: Partition,
    pub(crate) id: Vec<u8>,
    pub(crate) kept: RetainedKey,
    pub(crate) location: Location,
}

/// A kept version on disk to image: the Flow's partition and id, the
/// version and where it is.
pub(crate) type StoredEntry<'a> = ((&'a Partition, &'a [u8]), &'a RetainedKey, Location);

/// Writes the section.
pub(crate) fn encode(out: &mut Vec<u8>, entries: &[StoredEntry<'_>]) -> Option<()> {
    out.extend_from_slice(&u64::try_from(entries.len()).ok()?.to_be_bytes());
    for ((partition, id), (kind, name, version), location) in entries {
        bin(out, &partition.name())?;
        bin(out, id)?;
        out.push(*kind);
        bin(out, name)?;
        out.extend_from_slice(&version.to_be_bytes());
        out.extend_from_slice(&location.file.to_be_bytes());
        out.extend_from_slice(&location.offset.to_be_bytes());
        out.extend_from_slice(&location.length.to_be_bytes());
    }
    Some(())
}

/// Reads the section.
pub(super) fn decode(reader: &mut Reader<'_>) -> Result<Vec<Stored>, Malformed> {
    let count = reader.u64()?;
    let mut entries = Vec::new();
    for _ in 0..count {
        let partition = Partition::parse(reader.bin()?);
        let id = reader.bin()?.to_vec();
        let kind = reader.u8()?;
        let name = reader.bin()?.to_vec();
        let version = reader.u64()?;
        let location = Location {
            file: reader.u64()?,
            offset: reader.u64()?,
            length: u32::from_be_bytes(reader.array()?),
        };
        entries.push(Stored {
            partition,
            id,
            kept: (kind, name, version),
            location,
        });
    }
    Ok(entries)
}
