//! Raft messages between members (ADR-018), as bytes.
//!
//! A request starts with its recipient: the group's id (the first eight
//! bytes of the SHA-256 of its member list, [`group_id`]) and the member's
//! id. A member refuses a request for another group or member, so traffic
//! that reaches it by a reused address or a mistyped member list moves
//! nothing. Then comes its kind (`1` append entries, `2` vote, `3`
//! pre-vote, `4` a full snapshot: the leader's vote, the snapshot's metadata
//! and its length, its bytes following in frames of their own); a response
//! with its outcome (`0` an append accepted up to
//! an optional log id, `1` an append conflict at a log id, `2` a higher vote,
//! `3` a vote response, `4` a failure with its text, `5` a snapshot taken,
//! with the receiver's vote). Kind `6` asks what a member holds (ADR-019);
//! its answer is `6`, its last log index as a presence byte and the index,
//! the term it knows, and whether it joins as a voter (ADR-020). Kind `7`
//! asks for the member's newest snapshot if its last index is past one
//! index and at most another (a leader trimming its log, ADR-018 phase 3);
//! its answer is `7`, a presence byte, then the snapshot's metadata and
//! length, its bytes following as for kind `4`. Kind `8` asks the leader
//! for its read index; its answer is `8` and the index. Kind `9` asks the
//! leader to give a member a role (ADR-020): the member's id and `1` for
//! voter or `0` for replica; its answer is `9`. A vote is its term, node
//! and committed byte; an optional log id is a presence byte and the id's
//! term, node and index; an entry is its id, its payload length and payload
//! (`codec::encode`). Conflict hints are not sent; they are optional.

mod append;
mod reader;
mod recipient;
#[cfg(test)]
mod tests;

use std::io;

use openraft::raft::{
    AppendEntriesRequest, AppendError, AppendResult, SnapshotResponse, VoteRequest, VoteResponse,
};
use openraft::type_config::alias::{LogIdOf, SnapshotMetaOf, VoteOf};

use super::roles::Role;
use super::{Config, codec, snapshot};
pub(crate) use append::encode_append;
use reader::Reader;
pub(crate) use recipient::{Recipient, group_id};

/// A request from one member to another.
pub(crate) enum Request {
    /// Entries, or a heartbeat.
    Append(AppendEntriesRequest<Config>),
    /// A candidate's vote request.
    Vote(VoteRequest<Config>),
    /// A pre-vote probe.
    PreVote(VoteRequest<Config>),
    /// A snapshot for a member the log no longer reaches: the leader's vote,
    /// the snapshot's metadata and its length in bytes, which follow.
    Snapshot(VoteOf<Config>, SnapshotMetaOf<Config>, u64),
    /// What the member holds, asked by a member that starts without a log
    /// (ADR-019).
    Status,
    /// The member's newest snapshot, if its last index is past `after` and
    /// at most `upto`.
    Newest {
        /// The index the snapshot must be past.
        after: u64,
        /// The highest last index the snapshot may have.
        upto: u64,
    },
    /// The index a follower must apply through before it answers a read
    /// (ADR-018), from a leader that confirmed it still leads.
    ReadIndex,
    /// Gives member `.0` role `.1`, asked of the leader (ADR-020).
    Change(u64, Role),
}

/// What a member holds: its last log index and the term it knows, and the
/// role it joins with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Held {
    /// Its last log index, if it holds any log.
    pub(crate) last_log: Option<u64>,
    /// The highest term it knows.
    pub(crate) term: u64,
    /// Whether it joins as a voter; a replica joins as a learner (ADR-020).
    pub(crate) voter: bool,
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
    /// To a request for the newest snapshot: its metadata and length in
    /// bytes, which follow, or none in the range asked.
    Newest(Option<(SnapshotMetaOf<Config>, u64)>),
    /// To a read index request.
    ReadIndex(u64),
    /// To a role change: committed.
    Changed,
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

fn put_snapshot(out: &mut Vec<u8>, meta: &SnapshotMetaOf<Config>, length: u64) -> io::Result<()> {
    put_bytes(out, &snapshot::meta_bytes(meta)?)?;
    put_u64(out, length);
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
        Request::Snapshot(vote, meta, length) => {
            out.push(4);
            put_vote(&mut out, vote);
            put_snapshot(&mut out, meta, *length)?;
        }
        Request::Status => out.push(6),
        Request::ReadIndex => out.push(8),
        Request::Change(member, role) => {
            out.push(9);
            put_u64(&mut out, *member);
            out.push(u8::from(*role == Role::Voter));
        }
        Request::Newest { after, upto } => {
            out.push(7);
            put_u64(&mut out, *after);
            put_u64(&mut out, *upto);
        }
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
            out.push(u8::from(held.voter));
        }
        Response::Newest(newest) => {
            out.push(7);
            out.push(u8::from(newest.is_some()));
            if let Some((meta, length)) = newest {
                put_snapshot(&mut out, meta, *length)?;
            }
        }
        Response::ReadIndex(index) => {
            out.push(8);
            put_u64(&mut out, *index);
        }
        Response::Changed => out.push(9),
    }
    Ok(out)
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
            let (meta, length) = reader.snapshot()?;
            Request::Snapshot(vote, meta, length)
        }
        6 => Request::Status,
        8 => Request::ReadIndex,
        9 => {
            let member = reader.u64()?;
            match reader.u8()? {
                0 => Request::Change(member, Role::Replica),
                1 => Request::Change(member, Role::Voter),
                _ => return Err(invalid()),
            }
        }
        7 => Request::Newest {
            after: reader.u64()?,
            upto: reader.u64()?,
        },
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
            let term = reader.u64()?;
            let voter = match reader.u8()? {
                0 => false,
                1 => true,
                _ => return Err(invalid()),
            };
            Response::Status(Held {
                last_log,
                term,
                voter,
            })
        }
        7 => Response::Newest(match reader.u8()? {
            0 => None,
            1 => Some(reader.snapshot()?),
            _ => return Err(invalid()),
        }),
        8 => Response::ReadIndex(reader.u64()?),
        9 => Response::Changed,
        _ => return Err(invalid()),
    };
    reader.done(response)
}
