//! Member roles (ADR-020): voters vote, lead and count toward every
//! quorum; replicas are `OpenRaft` learners, which take and apply every
//! committed entry but never vote. The voters form a group (`join`); its
//! leader then adds every other listed member as a learner, and changes
//! one member's role at a time when asked, which needs no joint
//! configuration.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use openraft::error::{ChangeMembershipError, ClientWriteError, RaftError};
use openraft::impls::ChangeMembers;
use openraft::storage::RaftStateMachine;
use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::{Raft, RaftMetrics};

use super::Config;
use super::peer::{self, Link};
use super::wire::{self, Recipient};

/// A member's role in its group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Votes, may lead and counts toward every quorum.
    Voter,
    /// Takes and applies every committed entry, but never votes.
    Replica,
}

/// The role member `member` holds in the membership `metrics` show, if it
/// is in it.
pub(super) fn held(metrics: &RaftMetrics<Config>, member: u64) -> Option<Role> {
    let membership = metrics.membership_config.membership();
    if membership.voter_ids().any(|voter| voter == member) {
        Some(Role::Voter)
    } else {
        membership.get_node(&member).map(|()| Role::Replica)
    }
}

/// How long a change refused for now (another change still being
/// committed, a new leader that has not committed an entry of its own
/// term, a replica the leader has not added yet) is tried again.
const TRY_FOR: Duration = Duration::from_secs(5);

/// Whether `error` refuses a change only for now.
fn passing(error: &RaftError<Config, ClientWriteError<Config>>) -> bool {
    matches!(
        error.api_error(),
        Some(
            ClientWriteError::LogEntryDiscarded(_)
                | ClientWriteError::ChangeMembershipError(
                    ChangeMembershipError::InProgress(_)
                        | ChangeMembershipError::UncommittedLeaderLog(_)
                        | ChangeMembershipError::LearnerNotFound(_)
                )
        )
    )
}

/// Gives member `member` `role` on `raft`, which leads: a replica becomes
/// a voter, a voter a replica (kept as a learner). A member that already
/// holds the role changes nothing; the last voter is never demoted. A
/// change refused for now is tried again for up to [`TRY_FOR`].
///
/// # Errors
/// The reply text of a refusal: the last voter, a member that does not
/// lead, or a change still refused for now.
pub(super) async fn change<SM>(
    raft: &Raft<Config, SM>,
    member: u64,
    role: Role,
) -> Result<(), String>
where
    SM: RaftStateMachine<Config>,
{
    let started = Instant::now();
    loop {
        let (holds, voters) = {
            let metrics = raft.metrics();
            let metrics = metrics.borrow_watched();
            let voters = metrics.membership_config.membership().voter_ids().count();
            (held(&metrics, member), voters)
        };
        let edit = match (role, holds) {
            (Role::Voter, Some(Role::Voter)) | (Role::Replica, Some(Role::Replica)) => {
                return Ok(());
            }
            (Role::Replica, Some(Role::Voter)) if voters <= 1 => {
                return Err("ERR a group needs at least one voter".to_owned());
            }
            (Role::Replica, Some(Role::Voter)) => {
                ChangeMembers::RemoveVoters(BTreeSet::from([member]))
            }
            (Role::Voter, _) => ChangeMembers::AddVoterIds(BTreeSet::from([member])),
            // Not added yet: added as a replica, so nothing to demote.
            (Role::Replica, None) => return Ok(()),
        };
        match raft.change_membership(edit, true).await {
            Ok(_) => return Ok(()),
            Err(error) if passing(&error) && started.elapsed() < TRY_FOR => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(error) if passing(&error) => {
                return Err(format!(
                    "ERR the group's membership is changing; try again ({error})"
                ));
            }
            Err(error)
                if matches!(
                    error.api_error(),
                    Some(ClientWriteError::ForwardToLeader(_))
                ) =>
            {
                return Err(LEADER_CHANGED.to_owned());
            }
            Err(error) => return Err(format!("ERR {error}")),
        }
    }
}

/// How long a member waits for the leader to answer a change it asked:
/// the leader tries it again for up to [`TRY_FOR`].
const ASK_FOR: Duration = Duration::from_secs(10);

/// A member asked for a change it can make only as the leader, and it no
/// longer leads.
const LEADER_CHANGED: &str = "ERR the group's leader changed; try again";

/// A change's outcome where it was asked: refused for good, or for now
/// (no leader known, the leader asked no longer leading or not answering).
enum Asked {
    Refused(String),
    Again(String),
}

/// Asks `leader` of the group `peers` lists for the change.
async fn ask_leader(
    (peers, limit): (&std::collections::BTreeMap<u64, String>, u32),
    leader: u64,
    (member, role): (u64, Role),
) -> Result<(), Asked> {
    let address = peers
        .get(&leader)
        .ok_or_else(|| Asked::Refused(format!("ERR the leader, member {leader}, is not listed")))?;
    let recipient = Recipient {
        group: wire::group_id(peers),
        member: leader,
    };
    let mut link = Link::to(address.clone(), recipient, limit);
    match peer::change(&mut link, (member, role), ASK_FOR).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(refusal)) if refusal == LEADER_CHANGED => Err(Asked::Again(refusal)),
        Ok(Err(refusal)) => Err(Asked::Refused(refusal)),
        Err(error) => Err(Asked::Again(format!(
            "ERR the leader did not answer; try again ({error})"
        ))),
    }
}

/// Where a member asks for a change: its id, every member's Raft address
/// and its frame limit.
pub(super) type Asker<'a> = (u64, &'a std::collections::BTreeMap<u64, String>, u32);

/// Gives member `member` `role`: on `raft` when the asking member leads,
/// otherwise asked of the leader it knows. While no leader is known, or
/// the one asked no longer leads or does not answer (a leadership just
/// changing), it asks again for up to [`TRY_FOR`].
///
/// # Errors
/// The leader's refusal, or still no leader known or answering.
pub(super) async fn asked<SM>(
    raft: &Raft<Config, SM>,
    (own, peers, limit): Asker<'_>,
    change_asked: (u64, Role),
) -> Result<(), String>
where
    SM: RaftStateMachine<Config>,
{
    let started = Instant::now();
    loop {
        let leader = raft.metrics().borrow_watched().current_leader;
        let outcome = match leader {
            Some(leader) if leader == own => {
                let (member, role) = change_asked;
                return change(raft, member, role).await;
            }
            Some(leader) => ask_leader((peers, limit), leader, change_asked).await,
            None => Err(Asked::Again("ERR no leader is known; try again".to_owned())),
        };
        match outcome {
            Ok(()) => return Ok(()),
            Err(Asked::Again(_)) if started.elapsed() < TRY_FOR => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(Asked::Again(refusal) | Asked::Refused(refusal)) => return Err(refusal),
        }
    }
}

/// How often a member looks at what it owes its group as a leader. A
/// timer rather than every change of its metrics, which change with every
/// append.
const LOOK_EVERY: Duration = Duration::from_millis(200);

/// How long a leader waits for a hand-off it asked for before it asks
/// again.
const HAND_AGAIN: Duration = Duration::from_millis(500);

/// How long a leader without a vote waits, every entry committed, before it
/// hands leadership on.
const SETTLED: Duration = Duration::from_millis(200);

/// What a leader still owes its group: the listed members missing from
/// the membership, and the voter to hand leadership to when it leads
/// without voting. `OpenRaft` steps down only a leader removed from the
/// membership; one demoted, and so kept as a learner, would lead on, so it
/// hands leadership to the most up-to-date voter once its demotion is
/// committed and so is every entry it appended (its machine stopped taking
/// writes when it was demoted, so a round it decided is not lost). Nothing
/// while member `own` does not lead.
fn owed<SM>(raft: &Raft<Config, SM>, listed: &BTreeSet<u64>, own: u64) -> (Vec<u64>, Option<u64>)
where
    SM: RaftStateMachine<Config>,
{
    let metrics = raft.metrics();
    let metrics = metrics.borrow_watched();
    if metrics.current_leader != Some(own) {
        return (Vec::new(), None);
    }
    let membership = metrics.membership_config.membership();
    let missing = listed
        .iter()
        .copied()
        .filter(|member| membership.get_node(member).is_none())
        .collect();
    let committed = metrics.committed_membership_config == metrics.membership_config
        && metrics.cluster_committed.map(|log_id| log_id.index) == metrics.last_log_index;
    let hand_to = if committed && held(&metrics, own) != Some(Role::Voter) {
        let matched = |voter: &u64| {
            metrics
                .replication
                .as_ref()
                .and_then(|replication| replication.get(voter).copied().flatten())
        };
        membership.voter_ids().max_by_key(matched)
    } else {
        None
    };
    (missing, hand_to)
}

/// While member `own` leads, adds every member of `listed` missing from
/// the membership as a learner (one that started late, or one a leader
/// elected before it added it left out: the voters formed the group, so a
/// missing member is a replica), and hands leadership on when it leads
/// without voting ([`owed`]). Looks every [`LOOK_EVERY`]; returns once
/// `raft` stops.
pub(super) async fn keep<SM>(raft: Raft<Config, SM>, listed: BTreeSet<u64>, own: u64)
where
    SM: RaftStateMachine<Config>,
{
    // Since when every entry was committed while this member led without
    // voting: a round its shards decided before they stopped taking writes
    // may still be on its way to the log, so it hands leadership on only
    // once nothing was appended for [`SETTLED`].
    let mut settled: Option<Instant> = None;
    // When this member last handed leadership on, so the hand-off is not
    // asked again on every look while it is under way.
    let mut handed: Option<Instant> = None;
    loop {
        tokio::time::sleep(LOOK_EVERY).await;
        if raft.metrics().borrow_watched().running_state.is_err() {
            return;
        }
        let (missing, hand_to) = owed(&raft, &listed, own);
        for member in missing {
            // Refused while a change is still being committed, or before
            // this leader committed an entry of its own term: looked at
            // again.
            let _added = raft.add_learner(member, (), false).await;
        }
        settled = hand_to.and(settled.or_else(|| Some(Instant::now())));
        let due = settled.is_some_and(|at| at.elapsed() >= SETTLED)
            && handed.is_none_or(|at| at.elapsed() >= HAND_AGAIN);
        if let Some(to) = hand_to.filter(|_| due) {
            handed = Some(Instant::now());
            if raft.trigger().transfer_leader(to).await.is_err() {
                return;
            }
        }
    }
}
