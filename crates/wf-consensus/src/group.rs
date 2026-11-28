//! Replicated groups (ADR-018): `OpenRaft`'s type configuration for a group,
//! with the consensus log port (`wf_ports::consensus::log`) as its log
//! store.
//! `OpenRaft`'s types stay inside this crate.

mod ballot;
mod codec;
mod join;
mod log_store;
mod machine;
mod member;
mod peer;
mod proposer;
mod reads;
mod roles;
mod snapshot;
#[cfg(test)]
mod testing;
mod wire;

use std::fmt;

use openraft::vote::RaftLeaderId;
use openraft::{OptionalSend, impls};

pub use join::serve_member;
pub use log_store::LogStore;
pub use member::{Changed, Member, MemberConfig, MemberError};
pub use peer::{Link, MAX_FRAME_BYTES, Peers, serve};
pub use proposer::{Committed, Proposer};
pub use reads::{Current, Reader};
pub use roles::Role;
pub use snapshot::Stored;

/// An entry's application bytes: one shard round's changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Proposal(pub Vec<u8>);

impl fmt::Display for Proposal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "proposal of {} bytes", self.0.len())
    }
}

/// A replicated group's `OpenRaft` types, written out as
/// `openraft::declare_raft_types!` would with these choices: node ids are
/// `u64`, a member carries no address (the node's configuration holds
/// those), and an entry is a [`Proposal`] applied without a reply value.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd)]
pub struct Config;

impl openraft::RaftTypeConfig for Config {
    type D = Proposal;
    type R = ();
    type NodeId = u64;
    type Node = ();
    type Term = u64;
    type LeaderId = impls::leader_id_adv::LeaderId<Self::Term, Self::NodeId>;
    type Vote = impls::Vote<Self::LeaderId>;
    type Membership = impls::Membership<Self::NodeId, Self::Node>;
    type MembershipEdit = impls::ChangeMembers<Self::NodeId, Self::Node>;
    type Payload = impls::EntryPayload<Self::D, Self::Membership>;
    type Entry = impls::Entry<<Self::LeaderId as RaftLeaderId>::Committed, Self::Payload>;
    type Responder<T>
        = impls::ProgressResponder<Self, T>
    where
        T: OptionalSend + 'static;
    type Batch<T>
        = impls::InlineBatch<T>
    where
        T: OptionalSend + 'static;
    type AsyncRuntime = impls::TokioRuntime;
    type ErrorSource = impls::BoxedErrorSource;
}
