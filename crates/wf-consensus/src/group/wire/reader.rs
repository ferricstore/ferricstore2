//! Reading a peer message's fields in order (the encoding `wire`
//! describes).

use std::io;

use openraft::Snapshot;
use openraft::type_config::alias::{LogIdOf, SnapshotOf, VoteOf};
use wf_ports::consensus::log::EntryId;

use super::super::{Config, codec, snapshot};
use super::invalid;

/// Reads a message's fields in order.
pub(super) struct Reader<'a>(pub(super) &'a [u8]);

impl<'a> Reader<'a> {
    pub(super) fn take(&mut self, length: usize) -> io::Result<&'a [u8]> {
        if self.0.len() < length {
            return Err(invalid());
        }
        let (head, rest) = self.0.split_at(length);
        self.0 = rest;
        Ok(head)
    }

    pub(super) fn u8(&mut self) -> io::Result<u8> {
        self.take(1)?.first().copied().ok_or_else(invalid)
    }

    pub(super) fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| invalid())?,
        ))
    }

    pub(super) fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| invalid())?,
        ))
    }

    pub(super) fn vote(&mut self) -> io::Result<VoteOf<Config>> {
        codec::vote(self.take(17)?)
    }

    pub(super) fn entry_id(&mut self) -> io::Result<Option<EntryId>> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(EntryId {
                term: self.u64()?,
                node: self.u64()?,
                index: self.u64()?,
            })),
            _ => Err(invalid()),
        }
    }

    pub(super) fn log_id(&mut self) -> io::Result<Option<LogIdOf<Config>>> {
        Ok(self.entry_id()?.map(codec::log_id))
    }

    pub(super) fn bytes(&mut self) -> io::Result<&'a [u8]> {
        let length = usize::try_from(self.u32()?).map_err(|_| invalid())?;
        self.take(length)
    }

    pub(super) fn snapshot(&mut self) -> io::Result<SnapshotOf<Config, Vec<u8>>> {
        let meta = snapshot::parse_meta(self.bytes()?)?;
        Ok(Snapshot {
            meta,
            snapshot: self.bytes()?.to_vec(),
        })
    }

    pub(super) fn done<T>(&self, value: T) -> io::Result<T> {
        if self.0.is_empty() {
            Ok(value)
        } else {
            Err(invalid())
        }
    }
}
