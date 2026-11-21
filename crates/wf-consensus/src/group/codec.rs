//! `OpenRaft` entries and votes as the entry log's bytes.
//!
//! An entry's bytes start with its payload kind: `0` blank, `1` a proposal
//! (its bytes follow), `2` a membership (the joint configurations, each a
//! count and its voters, then every member). A vote is its term, node and a
//! committed byte. Integers are big-endian.

use std::collections::BTreeSet;
use std::io;

use openraft::impls::leader_id_adv::LeaderId;
use openraft::type_config::alias::{EntryOf, LogIdOf, MembershipOf, VoteOf};
use openraft::{EntryPayload, LogId, Membership, Vote};
use wf_ports::consensus::log::EntryId;

use super::{Config, Proposal};

const BLANK: u8 = 0;
const NORMAL: u8 = 1;
const MEMBERSHIP: u8 = 2;

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("malformed {what}"))
}

pub(super) fn entry_id(log_id: &LogIdOf<Config>) -> EntryId {
    EntryId {
        term: log_id.leader_id.term,
        node: log_id.leader_id.node_id,
        index: log_id.index,
    }
}

pub(super) fn log_id(id: EntryId) -> LogIdOf<Config> {
    LogId::new(
        LeaderId {
            term: id.term,
            node_id: id.node,
        },
        id.index,
    )
}

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_ids(out: &mut Vec<u8>, ids: &BTreeSet<u64>) -> Option<()> {
    out.extend_from_slice(&u32::try_from(ids.len()).ok()?.to_be_bytes());
    for id in ids {
        put_u64(out, *id);
    }
    Some(())
}

/// An entry's payload as bytes.
pub(super) fn encode(entry: &EntryOf<Config>) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    match &entry.payload {
        EntryPayload::Blank => out.push(BLANK),
        EntryPayload::Normal(proposal) => {
            out.push(NORMAL);
            out.extend_from_slice(&proposal.0);
        }
        EntryPayload::Membership(membership) => {
            out.push(MEMBERSHIP);
            put_membership(&mut out, membership)?;
        }
    }
    Ok(out)
}

/// A membership: its joint configurations, each a count and its voters,
/// then every member.
pub(super) fn put_membership(
    out: &mut Vec<u8>,
    membership: &MembershipOf<Config>,
) -> io::Result<()> {
    let configs = membership.get_joint_config();
    out.extend_from_slice(
        &u32::try_from(configs.len())
            .map_err(|_| invalid("membership"))?
            .to_be_bytes(),
    );
    for config in configs {
        put_ids(out, config).ok_or_else(|| invalid("membership"))?;
    }
    let members: BTreeSet<u64> = membership.nodes().map(|(id, ())| *id).collect();
    put_ids(out, &members).ok_or_else(|| invalid("membership"))
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let (head, rest) = self.0.split_first_chunk::<N>()?;
        self.0 = rest;
        Some(*head)
    }

    fn ids(&mut self) -> Option<BTreeSet<u64>> {
        let count = u32::from_be_bytes(self.take()?);
        (0..count)
            .map(|_| Some(u64::from_be_bytes(self.take()?)))
            .collect()
    }
}

pub(super) fn membership(bytes: &[u8]) -> Option<MembershipOf<Config>> {
    let mut reader = Reader(bytes);
    let count = u32::from_be_bytes(reader.take()?);
    let configs = (0..count)
        .map(|_| reader.ids())
        .collect::<Option<Vec<_>>>()?;
    let members = reader.ids()?;
    reader.0.is_empty().then_some(())?;
    Membership::new(configs, members).ok()
}

/// The entry with `id` whose payload is `bytes`.
pub(super) fn decode(id: EntryId, bytes: &[u8]) -> io::Result<EntryOf<Config>> {
    let (&kind, rest) = bytes.split_first().ok_or_else(|| invalid("entry"))?;
    let payload = match kind {
        BLANK if rest.is_empty() => EntryPayload::Blank,
        NORMAL => EntryPayload::Normal(Proposal(rest.to_vec())),
        MEMBERSHIP => {
            EntryPayload::Membership(membership(rest).ok_or_else(|| invalid("membership"))?)
        }
        _ => return Err(invalid("entry")),
    };
    Ok(openraft::Entry {
        log_id: log_id(id),
        payload,
    })
}

pub(super) fn vote_bytes(vote: &VoteOf<Config>) -> Vec<u8> {
    let mut out = Vec::with_capacity(17);
    put_u64(&mut out, vote.leader_id.term);
    put_u64(&mut out, vote.leader_id.node_id);
    out.push(u8::from(vote.committed));
    out
}

pub(super) fn vote(bytes: &[u8]) -> io::Result<VoteOf<Config>> {
    let mut reader = Reader(bytes);
    let term = reader.take().map(u64::from_be_bytes);
    let node = reader.take().map(u64::from_be_bytes);
    let committed = reader.take::<1>().map(|[byte]| byte);
    match (term, node, committed, reader.0.is_empty()) {
        (Some(term), Some(node), Some(0), true) => Ok(Vote::new(term, node)),
        (Some(term), Some(node), Some(1), true) => Ok(Vote::new_committed(term, node)),
        _ => Err(invalid("vote")),
    }
}
