use std::collections::BTreeMap;

use super::super::wire::Held;
use super::{Settled, settle};

fn peers() -> BTreeMap<u64, String> {
    [1, 2, 3]
        .into_iter()
        .map(|id| (id, format!("member-{id}:7000")))
        .collect()
}

fn held(last_log: Option<u64>, term: u64) -> Held {
    Held { last_log, term }
}

#[test]
fn a_group_is_new_while_no_other_member_holds_a_log_past_its_first_entry() {
    let empty = [held(None, 0), held(None, 0)];
    // Only the lowest member forms the group.
    assert_eq!(settle(&empty, 1, &peers()), Settled::New { form: true });
    assert_eq!(settle(&empty, 2, &peers()), Settled::New { form: false });
    assert_eq!(settle(&empty, 3, &peers()), Settled::New { form: false });
    // The lowest member wrote the first entry and has not yet been elected:
    // the group is still new, and no one forms it again.
    let forming = [held(Some(0), 1), held(None, 0)];
    assert_eq!(settle(&forming, 1, &peers()), Settled::New { form: false });
    assert_eq!(settle(&forming, 2, &peers()), Settled::New { form: false });
}

#[test]
fn a_member_lost_its_log_once_another_holds_one_past_the_first_entry() {
    assert_eq!(
        settle(&[held(Some(1), 3), held(None, 5)], 1, &peers()),
        Settled::Lost { term: 5 }
    );
    assert_eq!(
        settle(&[held(Some(0), 2), held(Some(7), 4)], 2, &peers()),
        Settled::Lost { term: 4 }
    );
}
