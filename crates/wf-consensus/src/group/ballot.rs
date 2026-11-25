//! Whether this member may vote and stand for election (ADR-019).
//!
//! Raft's safety rests on a voter keeping its log and its vote. A member
//! that starts without a log may have lost one, and its vote could then
//! elect a member that lacks entries only it and a stopped member held:
//! those entries would be lost. So such a member first asks every other
//! member what it holds. If none holds a log beyond a forming group's first
//! entry, the group is new and it votes. Otherwise it votes once it applied
//! a commit point that a leader at or above every other member's term set
//! in its own term: such a leader holds every entry committed before it, so
//! the member then holds every entry its lost log had taken.

#[cfg(test)]
mod tests;

use std::sync::{Mutex, PoisonError};

use openraft::raft::AppendEntriesRequest;
use openraft::vote::RaftLeaderId;

use super::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Standing {
    /// It votes.
    Votes,
    /// It started without a log and has not heard from every other member.
    Asking,
    /// The group exists and this member lost its log: it waits to apply a
    /// commit point set by a leader at `term` or above.
    CatchingUp { term: u64 },
}

#[derive(Debug)]
struct State {
    standing: Standing,
    /// The first commit point heard from the newest leader that committed
    /// an entry of its own term: (that term, the point's index). Kept from
    /// the start, since the leader may catch this member up before every
    /// other member has answered.
    point: Option<(u64, u64)>,
}

/// This member's standing, shared by its peer server and its join task.
pub(super) struct Ballot(Mutex<State>);

impl Ballot {
    /// A member that starts with a log votes; one without asks first.
    pub(super) const fn new(fresh: bool) -> Self {
        Self(Mutex::new(State {
            standing: if fresh {
                Standing::Asking
            } else {
                Standing::Votes
            },
            point: None,
        }))
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        // The state is plain values: a panic elsewhere cannot leave it half
        // written.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether this member may vote now.
    pub(super) fn votes(&self) -> bool {
        self.state().standing == Standing::Votes
    }

    /// No other member holds a log: the group is new.
    pub(super) fn open(&self) {
        self.state().standing = Standing::Votes;
    }

    /// Another member holds a log, so this member lost its own; `term` is
    /// the highest term the other members know.
    pub(super) fn catch_up(&self, term: u64) {
        self.state().standing = Standing::CatchingUp { term };
    }

    /// Notes the commit point of an append from a leader that committed an
    /// entry of its own term. A later leader's point replaces an earlier
    /// one; within one term the first stays, so writes do not move it.
    pub(super) fn hear(&self, append: &AppendEntriesRequest<Config>) {
        let mut state = self.state();
        if state.standing == Standing::Votes {
            return;
        }
        let leader = append.vote.leader_id();
        if let Some(commit) = &append.leader_commit
            && commit.committed_leader_id() == leader
            && state.point.is_none_or(|(term, _)| leader.term() > term)
        {
            state.point = Some((leader.term(), commit.index));
        }
    }

    /// Whether this member votes, now that it applied up to `applied`.
    pub(super) fn applied(&self, applied: Option<u64>) -> bool {
        let mut state = self.state();
        if let Standing::CatchingUp { term } = state.standing
            && let Some((leader, point)) = state.point
            && leader >= term
            && applied.is_some_and(|applied| applied >= point)
        {
            state.standing = Standing::Votes;
        }
        state.standing == Standing::Votes
    }
}
