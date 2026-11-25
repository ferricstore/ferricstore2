//! A deadline's edges, on a fixed clock.

use wf_application::commands::flow::Term;

use super::{Late, admit, check_at};

const NOW: i64 = 1_800_000_000_000;

fn args(deadline: Term) -> Term {
    Term::Map(vec![
        (b"id".to_vec(), Term::text("f1")),
        (b"deadline_ms".to_vec(), deadline),
    ])
}

#[test]
fn a_deadline_passes_only_once_the_clock_is_beyond_it() {
    assert_eq!(check_at(&args(Term::Int(NOW + 1)), NOW), Ok(()));
    assert_eq!(
        check_at(&args(Term::Int(NOW)), NOW),
        Ok(()),
        "not yet passed"
    );
    assert_eq!(
        check_at(&args(Term::Int(NOW - 1)), NOW),
        Err(Late::Exceeded)
    );
    assert_eq!(check_at(&args(Term::Int(1)), NOW), Err(Late::Exceeded));
}

#[test]
fn zero_is_no_deadline_and_a_negative_or_other_value_is_invalid() {
    assert_eq!(check_at(&args(Term::Int(0)), NOW), Ok(()));
    assert_eq!(check_at(&args(Term::Int(-1)), NOW), Err(Late::Invalid));
    assert_eq!(check_at(&args(Term::text("soon")), NOW), Err(Late::Invalid));
    assert_eq!(check_at(&args(Term::Nil), NOW), Err(Late::Invalid));
    assert_eq!(check_at(&Term::Map(Vec::new()), NOW), Ok(()));
}

#[test]
fn an_admitted_request_loses_only_its_deadline() {
    assert_eq!(
        admit(args(Term::Int(0))),
        Ok(Term::Map(vec![(b"id".to_vec(), Term::text("f1"))]))
    );
}
