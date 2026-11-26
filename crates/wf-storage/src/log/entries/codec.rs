//! The entry log's records, each one group log record: tag `E` an entry
//! (its id, then its bytes), `T` a truncation (every entry after the index
//! is dropped; no index drops all), `V` a vote (opaque bytes), `P` a purge
//! (entries up to the id are gone), `W` the mark that the log's member
//! votes (ADR-019).

use super::EntryId;

const ENTRY: u8 = b'E';
const TRUNCATE: u8 = b'T';
const VOTE: u8 = b'V';
const PURGE: u8 = b'P';
const VOTER: u8 = b'W';

/// One decoded record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Record {
    Entry(EntryId, Vec<u8>),
    Truncate(Option<u64>),
    Vote(Vec<u8>),
    Purge(EntryId),
    Voter,
}

fn put_id(out: &mut Vec<u8>, id: EntryId) {
    out.extend_from_slice(&id.term.to_be_bytes());
    out.extend_from_slice(&id.node.to_be_bytes());
    out.extend_from_slice(&id.index.to_be_bytes());
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    let end = at.checked_add(8)?;
    Some(u64::from_be_bytes(bytes.get(at..end)?.try_into().ok()?))
}

fn id_at(bytes: &[u8], at: usize) -> Option<EntryId> {
    Some(EntryId {
        term: u64_at(bytes, at)?,
        node: u64_at(bytes, at.checked_add(8)?)?,
        index: u64_at(bytes, at.checked_add(16)?)?,
    })
}

pub(super) fn entry(id: EntryId, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len().saturating_add(25));
    out.push(ENTRY);
    put_id(&mut out, id);
    out.extend_from_slice(bytes);
    out
}

pub(super) fn truncate(after: Option<u64>) -> Vec<u8> {
    let mut out = vec![TRUNCATE, u8::from(after.is_some())];
    out.extend_from_slice(&after.unwrap_or(0).to_be_bytes());
    out
}

pub(super) fn vote(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len().saturating_add(1));
    out.push(VOTE);
    out.extend_from_slice(bytes);
    out
}

pub(super) fn voter() -> Vec<u8> {
    vec![VOTER]
}

pub(super) fn purge(id: EntryId) -> Vec<u8> {
    let mut out = vec![PURGE];
    put_id(&mut out, id);
    out
}

/// Reads one record; `None` for bytes no version of this log wrote.
pub(super) fn decode(record: &[u8]) -> Option<Record> {
    let (&tag, rest) = record.split_first()?;
    match tag {
        ENTRY => Some(Record::Entry(id_at(rest, 0)?, rest.get(24..)?.to_vec())),
        TRUNCATE => match (rest.first()?, u64_at(rest, 1)?, rest.len()) {
            (0, _, 9) => Some(Record::Truncate(None)),
            (1, index, 9) => Some(Record::Truncate(Some(index))),
            _ => None,
        },
        VOTE => Some(Record::Vote(rest.to_vec())),
        PURGE if rest.len() == 24 => Some(Record::Purge(id_at(rest, 0)?)),
        VOTER if rest.is_empty() => Some(Record::Voter),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{EntryId, Record, decode, entry, purge, truncate, vote, voter};

    #[test]
    fn every_record_reads_back() {
        let id = EntryId {
            term: 3,
            node: 2,
            index: 7,
        };
        for (bytes, record) in [
            (entry(id, b"x"), Record::Entry(id, b"x".to_vec())),
            (entry(id, b""), Record::Entry(id, Vec::new())),
            (truncate(None), Record::Truncate(None)),
            (truncate(Some(5)), Record::Truncate(Some(5))),
            (vote(b"v"), Record::Vote(b"v".to_vec())),
            (purge(id), Record::Purge(id)),
            (voter(), Record::Voter),
        ] {
            assert_eq!(decode(&bytes), Some(record));
        }
        assert_eq!(decode(b"E"), None);
        assert_eq!(decode(b"Z"), None);
        assert_eq!(decode(b"Wx"), None);
    }
}
