use openraft::impls::Vote;
use openraft::raft::AppendEntriesRequest;
use wf_ports::consensus::log::EntryId;

use super::super::{Config, codec};
use super::Ballot;

/// A heartbeat from member `leader` at `term`, with its commit point as
/// (the term that wrote it, its index).
fn heartbeat(term: u64, leader: u64, commit: Option<(u64, u64)>) -> AppendEntriesRequest<Config> {
    AppendEntriesRequest {
        vote: Vote::new_committed(term, leader),
        prev_log_id: None,
        entries: Vec::new(),
        leader_commit: commit.map(|(written, index)| {
            codec::log_id(EntryId {
                term: written,
                node: leader,
                index,
            })
        }),
    }
}

#[test]
fn a_member_with_a_log_votes_and_one_without_waits_to_be_settled() {
    assert!(Ballot::new(false).votes());
    let fresh = Ballot::new(true);
    assert!(!fresh.votes());
    assert!(!fresh.applied(Some(u64::MAX)));
    fresh.open();
    assert!(fresh.votes());
}

#[test]
fn a_member_that_lost_its_log_votes_once_it_applied_a_current_leader_s_commit() {
    let ballot = Ballot::new(true);
    ballot.catch_up(4);
    assert!(!ballot.votes());
    // Nothing to wait for yet: no leader has said where it committed.
    assert!(!ballot.applied(Some(100)));
    ballot.hear(&heartbeat(4, 2, Some((4, 9))));
    assert!(!ballot.applied(None));
    assert!(!ballot.applied(Some(8)));
    assert!(!ballot.votes());
    assert!(ballot.applied(Some(9)));
    assert!(ballot.votes());
}

#[test]
fn a_point_heard_before_every_member_answered_counts_once_it_did() {
    let ballot = Ballot::new(true);
    ballot.hear(&heartbeat(4, 2, Some((4, 9))));
    assert!(!ballot.applied(Some(9)));
    ballot.catch_up(4);
    assert!(ballot.applied(Some(9)));
}

#[test]
fn only_an_own_term_commit_from_a_leader_at_the_asked_term_lets_it_vote() {
    let ballot = Ballot::new(true);
    ballot.catch_up(4);
    for ignored in [
        // A leader below every other member's term may be stale.
        heartbeat(3, 2, Some((3, 9))),
        // A commit point from an earlier term may not cover every committed
        // entry yet; nor may no commit point.
        heartbeat(5, 2, Some((4, 9))),
        heartbeat(5, 2, None),
    ] {
        ballot.hear(&ignored);
        assert!(!ballot.applied(Some(100)));
    }
    ballot.hear(&heartbeat(4, 2, Some((4, 9))));
    assert!(ballot.applied(Some(9)));
}

#[test]
fn a_leader_s_first_point_holds_until_a_later_leader_s_replaces_it() {
    // Writes under one leader do not move the point.
    let ballot = Ballot::new(true);
    ballot.catch_up(4);
    ballot.hear(&heartbeat(4, 2, Some((4, 9))));
    ballot.hear(&heartbeat(4, 2, Some((4, 50))));
    assert!(ballot.applied(Some(9)));
    // A later leader's point replaces it.
    let ballot = Ballot::new(true);
    ballot.catch_up(4);
    ballot.hear(&heartbeat(4, 2, Some((4, 9))));
    ballot.hear(&heartbeat(6, 3, Some((6, 50))));
    assert!(!ballot.applied(Some(49)));
    assert!(ballot.applied(Some(50)));
}
