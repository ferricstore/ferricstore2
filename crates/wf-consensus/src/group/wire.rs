//! Raft messages between members (ADR-018), as bytes.
//!
//! A request starts with its recipient: the group's id (the first eight
//! bytes of the SHA-256 of its member list, [`group_id`]) and the member's
//! id. A member refuses a request for another group or member, so traffic
//! that reaches it by a reused address or a mistyped member list moves
//! nothing. Then comes its kind (`1` append entries, `2` vote, `3`
//! pre-vote, `4` a full snapshot: the leader's vote, the snapshot's metadata
//! and its bytes); a response with its outcome (`0` an append accepted up to
//! an optional log id, `1` an append conflict at a log id, `2` a higher vote,
//! `3` a vote response, `4` a failure with its text, `5` a snapshot taken,
//! with the receiver's vote). Kind `6` asks what a member holds (ADR-019);
//! its answer is `6`, its last log index as a presence byte and the index,
//! and the term it knows. A vote is its term, node
//! and committed byte; an optional log id is a presence byte and the id's
//! term, node and index; an entry is its id, its payload length and payload
//! (`codec::encode`). Conflict hints are not sent; they are optional.

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::io;

use openraft::Snapshot;
use openraft::raft::{
    AppendEntriesRequest, AppendError, AppendResult, SnapshotResponse, VoteRequest, VoteResponse,
};
use openraft::type_config::alias::{LogIdOf, SnapshotOf, VoteOf};
use sha2::{Digest, Sha256};
use wf_ports::consensus::log::EntryId;

use super::{Config, codec, snapshot};

/// Whom a request is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Recipient {
    /// The group's id: [`group_id`] of its member list.
    pub(crate) group: u64,
    /// The member's id.
    pub(crate) member: u64,
}

/// A group's id: the first eight bytes of the SHA-256 of its member list,
/// each member's id and Raft address in id order. Every member is given the
/// same list, so they agree on it; another group lists other addresses.
pub(crate) fn group_id(peers: &BTreeMap<u64, String>) -> u64 {
    let mut digest = Sha256::new();
    for (id, address) in peers {
        digest.update(id.to_be_bytes());
        digest.update(
            u64::try_from(address.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        digest.update(address.as_bytes());
    }
    u64::from_be_bytes(
        digest
            .finalize()
            .first_chunk::<8>()
            .copied()
            .unwrap_or_default(),
    )
}

/// A request from one member to another.
pub(crate) enum Request {
    /// Entries, or a heartbeat.
    Append(AppendEntriesRequest<Config>),
    /// A candidate's vote request.
    Vote(VoteRequest<Config>),
    /// A pre-vote probe.
    PreVote(VoteRequest<Config>),
    /// A snapshot for a member the log no longer reaches.
    Snapshot(VoteOf<Config>, SnapshotOf<Config, Vec<u8>>),
    /// What the member holds, asked by a member that starts without a log
    /// (ADR-019).
    Status,
}

/// What a member holds: its last log index and the term it knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Held {
    /// Its last log index, if it holds any log.
    pub(crate) last_log: Option<u64>,
    /// The highest term it knows.
    pub(crate) term: u64,
}

/// The answer to a [`Request`].
pub(crate) enum Response {
    /// To an append.
    Append(AppendResult<Config>),
    /// To a vote or pre-vote.
    Vote(VoteResponse<Config>),
    /// The member could not handle the request.
    Failed(String),
    /// To a snapshot.
    Snapshot(SnapshotResponse<Config>),
    /// To a status request.
    Status(Held),
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "malformed peer message")
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_vote(out: &mut Vec<u8>, vote: &VoteOf<Config>) {
    out.extend_from_slice(&codec::vote_bytes(vote));
}

fn put_log_id(out: &mut Vec<u8>, log_id: Option<&LogIdOf<Config>>) {
    match log_id.map(codec::entry_id) {
        Some(id) => {
            out.push(1);
            put_u64(out, id.term);
            put_u64(out, id.node);
            put_u64(out, id.index);
        }
        None => out.push(0),
    }
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> io::Result<()> {
    out.extend_from_slice(
        &u32::try_from(bytes.len())
            .map_err(|_| invalid())?
            .to_be_bytes(),
    );
    out.extend_from_slice(bytes);
    Ok(())
}

/// A request for `recipient` as bytes.
///
/// # Errors
/// An entry or batch too large for the format.
pub(crate) fn encode_request(recipient: Recipient, request: &Request) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    put_u64(&mut out, recipient.group);
    put_u64(&mut out, recipient.member);
    match request {
        Request::Append(append) => {
            out.push(1);
            put_vote(&mut out, &append.vote);
            put_log_id(&mut out, append.prev_log_id.as_ref());
            put_log_id(&mut out, append.leader_commit.as_ref());
            let count = u32::try_from(append.entries.len()).map_err(|_| invalid())?;
            out.extend_from_slice(&count.to_be_bytes());
            for entry in &append.entries {
                put_log_id(&mut out, Some(&entry.log_id));
                put_bytes(&mut out, &codec::encode(entry)?)?;
            }
        }
        Request::Snapshot(vote, snapshot) => {
            out.push(4);
            put_vote(&mut out, vote);
            put_bytes(&mut out, &snapshot::meta_bytes(&snapshot.meta)?)?;
            put_bytes(&mut out, &snapshot.snapshot)?;
        }
        Request::Status => out.push(6),
        Request::Vote(vote) | Request::PreVote(vote) => {
            out.push(if matches!(request, Request::Vote(_)) {
                2
            } else {
                3
            });
            put_vote(&mut out, &vote.vote);
            put_log_id(&mut out, vote.last_log_id.as_ref());
        }
    }
    Ok(out)
}

/// A response as bytes.
///
/// # Errors
/// A failure text too long for the format.
pub(crate) fn encode_response(response: &Response) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    match response {
        Response::Append(Ok(matched)) => {
            out.push(0);
            put_log_id(&mut out, matched.as_ref());
        }
        Response::Append(Err(AppendError::Conflict { log_id, .. })) => {
            out.push(1);
            put_log_id(&mut out, Some(log_id));
        }
        Response::Append(Err(AppendError::HigherVote(vote))) => {
            out.push(2);
            put_vote(&mut out, vote);
        }
        Response::Vote(vote) => {
            out.push(3);
            put_vote(&mut out, &vote.vote);
            out.push(u8::from(vote.vote_granted));
            put_log_id(&mut out, vote.last_log_id.as_ref());
        }
        Response::Failed(text) => {
            out.push(4);
            put_bytes(&mut out, text.as_bytes())?;
        }
        Response::Snapshot(taken) => {
            out.push(5);
            put_vote(&mut out, &taken.vote);
        }
        Response::Status(held) => {
            out.push(6);
            match held.last_log {
                None => out.push(0),
                Some(index) => {
                    out.push(1);
                    put_u64(&mut out, index);
                }
            }
            put_u64(&mut out, held.term);
        }
    }
    Ok(out)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> io::Result<&'a [u8]> {
        if self.0.len() < length {
            return Err(invalid());
        }
        let (head, rest) = self.0.split_at(length);
        self.0 = rest;
        Ok(head)
    }

    fn u8(&mut self) -> io::Result<u8> {
        self.take(1)?.first().copied().ok_or_else(invalid)
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| invalid())?,
        ))
    }

    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| invalid())?,
        ))
    }

    fn vote(&mut self) -> io::Result<VoteOf<Config>> {
        codec::vote(self.take(17)?)
    }

    fn entry_id(&mut self) -> io::Result<Option<EntryId>> {
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

    fn log_id(&mut self) -> io::Result<Option<LogIdOf<Config>>> {
        Ok(self.entry_id()?.map(codec::log_id))
    }

    fn bytes(&mut self) -> io::Result<&'a [u8]> {
        let length = usize::try_from(self.u32()?).map_err(|_| invalid())?;
        self.take(length)
    }

    fn done<T>(&self, value: T) -> io::Result<T> {
        if self.0.is_empty() {
            Ok(value)
        } else {
            Err(invalid())
        }
    }
}

/// A request and its recipient from their bytes.
///
/// # Errors
/// Bytes no member wrote.
pub(crate) fn decode_request(bytes: &[u8]) -> io::Result<(Recipient, Request)> {
    let mut reader = Reader(bytes);
    let recipient = Recipient {
        group: reader.u64()?,
        member: reader.u64()?,
    };
    let request = match reader.u8()? {
        1 => {
            let vote = reader.vote()?;
            let prev_log_id = reader.log_id()?;
            let leader_commit = reader.log_id()?;
            let count = reader.u32()?;
            let mut entries = Vec::new();
            for _ in 0..count {
                let id = reader.entry_id()?.ok_or_else(invalid)?;
                entries.push(codec::decode(id, reader.bytes()?)?);
            }
            Request::Append(AppendEntriesRequest {
                vote,
                prev_log_id,
                entries,
                leader_commit,
            })
        }
        4 => {
            let vote = reader.vote()?;
            let meta = snapshot::parse_meta(reader.bytes()?)?;
            let data = reader.bytes()?.to_vec();
            Request::Snapshot(
                vote,
                Snapshot {
                    meta,
                    snapshot: data,
                },
            )
        }
        6 => Request::Status,
        kind @ (2 | 3) => {
            let vote = VoteRequest::new(reader.vote()?, reader.log_id()?);
            if kind == 2 {
                Request::Vote(vote)
            } else {
                Request::PreVote(vote)
            }
        }
        _ => return Err(invalid()),
    };
    reader.done((recipient, request))
}

/// A response from its bytes.
///
/// # Errors
/// Bytes no member wrote.
pub(crate) fn decode_response(bytes: &[u8]) -> io::Result<Response> {
    let mut reader = Reader(bytes);
    let response = match reader.u8()? {
        0 => Response::Append(Ok(reader.log_id()?)),
        1 => Response::Append(Err(AppendError::Conflict {
            log_id: reader.log_id()?.ok_or_else(invalid)?,
            hint: None,
        })),
        2 => Response::Append(Err(AppendError::HigherVote(reader.vote()?))),
        3 => {
            let vote = reader.vote()?;
            let granted = reader.u8()? == 1;
            Response::Vote(VoteResponse::new(vote, reader.log_id()?, granted))
        }
        4 => Response::Failed(String::from_utf8_lossy(reader.bytes()?).into_owned()),
        5 => Response::Snapshot(SnapshotResponse::new(reader.vote()?)),
        6 => {
            let last_log = match reader.u8()? {
                0 => None,
                1 => Some(reader.u64()?),
                _ => return Err(invalid()),
            };
            Response::Status(Held {
                last_log,
                term: reader.u64()?,
            })
        }
        _ => return Err(invalid()),
    };
    reader.done(response)
}
