//! An in-process network for the gate: a message reaches its target
//! member's Raft handle directly unless the target is stopped, the link is
//! cut, or the seeded loss drops it.

use std::collections::{BTreeMap, BTreeSet};
use std::future::{Future, ready};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use openraft::error::{RPCError, ReplicationClosed, StreamingError, Unreachable};
use openraft::network::{RPCOption, RaftNetworkFactory, RaftNetworkV2};
use openraft::raft::{
    AppendEntriesRequest, AppendResult, SnapshotResponse, VoteRequest, VoteResponse,
};
use openraft::type_config::alias::{SnapshotOf, VoteOf};
use openraft::{AnyError, OptionalSend, Raft};

use super::machine::Machine;
use wf_consensus::group::Config;

pub(super) type Member = Raft<Config, Machine>;

/// Message loss and cut links, with a seeded generator.
pub(super) struct Faults {
    /// Messages dropped per thousand.
    pub(super) loss: u64,
    /// Links that deliver nothing, both ways.
    pub(super) cut: BTreeSet<(u64, u64)>,
    state: u64,
}

impl Faults {
    fn lost(&mut self) -> bool {
        // xorshift64: deterministic per seed.
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state % 1_000 < self.loss
    }

    /// A number below `bound`, from the same generator.
    pub(super) fn below(&mut self, bound: u64) -> u64 {
        self.lost();
        self.state.checked_rem(bound).unwrap_or(0)
    }
}

#[derive(Clone)]
pub(super) struct Router {
    members: Arc<Mutex<BTreeMap<u64, Member>>>,
    faults: Arc<Mutex<Faults>>,
}

fn unreachable(text: &str) -> Unreachable<Config> {
    Unreachable::new(&AnyError::error(text))
}

impl Router {
    pub(super) fn new(seed: u64) -> Self {
        Self {
            members: Arc::default(),
            faults: Arc::new(Mutex::new(Faults {
                loss: 0,
                cut: BTreeSet::new(),
                state: seed | 1,
            })),
        }
    }

    pub(super) fn faults(&self) -> MutexGuard<'_, Faults> {
        self.faults.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn members(&self) -> MutexGuard<'_, BTreeMap<u64, Member>> {
        self.members.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn add(&self, id: u64, member: Member) {
        self.members().insert(id, member);
    }

    pub(super) fn remove(&self, id: u64) -> Option<Member> {
        self.members().remove(&id)
    }

    pub(super) fn member(&self, id: u64) -> Option<Member> {
        self.members().get(&id).cloned()
    }

    fn deliver(&self, from: u64, to: u64) -> Result<Member, Unreachable<Config>> {
        {
            let mut faults = self.faults();
            if faults.cut.contains(&(from.min(to), from.max(to))) || faults.lost() {
                return Err(unreachable("lost"));
            }
        }
        self.member(to).ok_or_else(|| unreachable("stopped"))
    }

    /// The network of member `from`.
    pub(super) fn from(&self, from: u64) -> Network {
        Network {
            router: self.clone(),
            from,
        }
    }
}

pub(super) struct Network {
    router: Router,
    from: u64,
}

impl RaftNetworkFactory<Config> for Network {
    type Network = Link;

    fn new_client(&mut self, target: u64, (): &()) -> impl Future<Output = Link> + Send {
        ready(Link {
            router: self.router.clone(),
            from: self.from,
            to: target,
        })
    }
}

pub(super) struct Link {
    router: Router,
    from: u64,
    to: u64,
}

impl RaftNetworkV2<Config> for Link {
    type SnapshotData = Vec<u8>;

    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<Config>,
        _option: RPCOption,
    ) -> Result<AppendResult<Config>, RPCError<Config>> {
        let member = self.router.deliver(self.from, self.to)?;
        member
            .append_entries(rpc)
            .await
            .map_err(|_| RPCError::Unreachable(unreachable("failed")))
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<Config>,
        _option: RPCOption,
    ) -> Result<VoteResponse<Config>, RPCError<Config>> {
        let member = self.router.deliver(self.from, self.to)?;
        member
            .vote(rpc)
            .await
            .map_err(|_| RPCError::Unreachable(unreachable("failed")))
    }

    async fn pre_vote(
        &mut self,
        rpc: VoteRequest<Config>,
        _option: RPCOption,
    ) -> Result<VoteResponse<Config>, RPCError<Config>> {
        let member = self.router.deliver(self.from, self.to)?;
        member
            .pre_vote(rpc)
            .await
            .map_err(|_| RPCError::Unreachable(unreachable("failed")))
    }

    async fn full_snapshot(
        &mut self,
        vote: VoteOf<Config>,
        snapshot: SnapshotOf<Config, Vec<u8>>,
        _cancel: impl Future<Output = ReplicationClosed> + OptionalSend + 'static,
        _option: RPCOption,
    ) -> Result<SnapshotResponse<Config>, StreamingError<Config>> {
        let member = self.router.deliver(self.from, self.to)?;
        member
            .install_full_snapshot(vote, snapshot)
            .await
            .map_err(|_| StreamingError::Unreachable(unreachable("failed")))
    }
}
