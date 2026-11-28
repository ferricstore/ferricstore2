use std::collections::{BTreeMap, BTreeSet};

use super::super::wire::Held;
use super::{Settled, settle};

fn peers() -> BTreeMap<u64, String> {
    [1, 2, 3]
        .into_iter()
        .map(|id| (id, format!("member-{id}:7000")))
        .collect()
}

fn held(last_log: Option<u64>, term: u64) -> Held {
    Held {
        last_log,
        term,
        voter: true,
    }
}

/// What every member but `own` answered: `answers[i]` from the i-th other
/// member in id order.
fn others(own: u64, answers: &[Held]) -> BTreeMap<u64, Held> {
    peers()
        .into_keys()
        .filter(|id| *id != own)
        .zip(answers.iter().copied())
        .collect()
}

fn forms(voters: &[u64]) -> Settled {
    Settled::New {
        forms: Some(voters.iter().copied().collect::<BTreeSet<u64>>()),
    }
}

const JOINS: Settled = Settled::New { forms: None };

#[test]
fn a_group_is_new_while_no_other_member_holds_a_log_past_its_first_entry() {
    let empty = [held(None, 0), held(None, 0)];
    // Only the lowest member forms the group.
    assert_eq!(
        settle(&others(1, &empty), (1, true), &peers()),
        forms(&[1, 2, 3])
    );
    assert_eq!(settle(&others(2, &empty), (2, true), &peers()), JOINS);
    assert_eq!(settle(&others(3, &empty), (3, true), &peers()), JOINS);
    // The lowest member wrote the first entry and has not yet been elected:
    // the group is still new, and no one forms it again.
    let written = [held(Some(0), 1), held(None, 0)];
    assert_eq!(settle(&others(1, &written), (1, true), &peers()), JOINS);
    assert_eq!(settle(&others(2, &written), (2, true), &peers()), JOINS);
    assert_eq!(settle(&others(3, &written), (3, true), &peers()), JOINS);
}

/// The lowest voter forms a new group, with the voters alone; replicas join
/// it later as learners, and a list without a voter never forms (ADR-020).
#[test]
fn the_lowest_voter_forms_a_new_group_with_the_voters_alone() {
    let replica = Held {
        voter: false,
        ..held(None, 0)
    };
    let voter = held(None, 0);
    // Member 1 is a replica: member 2 forms the group of 2 and 3.
    assert_eq!(
        settle(&others(1, &[voter, voter]), (1, false), &peers()),
        JOINS
    );
    assert_eq!(
        settle(&others(2, &[replica, voter]), (2, true), &peers()),
        forms(&[2, 3])
    );
    assert_eq!(
        settle(&others(3, &[replica, voter]), (3, true), &peers()),
        JOINS
    );
    // Member 3 alone votes.
    assert_eq!(
        settle(&others(3, &[replica, replica]), (3, true), &peers()),
        forms(&[3])
    );
    // Nobody votes: nobody forms.
    assert_eq!(
        settle(&others(1, &[replica, replica]), (1, false), &peers()),
        JOINS
    );
}

#[test]
fn a_member_lost_its_log_once_another_holds_one_past_the_first_entry() {
    assert_eq!(
        settle(
            &others(3, &[held(Some(1), 3), held(None, 5)]),
            (3, true),
            &peers()
        ),
        Settled::Lost { term: 5 }
    );
    assert_eq!(
        settle(
            &others(3, &[held(Some(0), 2), held(Some(7), 4)]),
            (3, false),
            &peers()
        ),
        Settled::Lost { term: 4 }
    );
}
