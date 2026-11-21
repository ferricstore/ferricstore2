//! Counting rules a node test cannot reach: the shard sweeps an expired
//! terminal Flow within a round, and broad-index candidate choice needs
//! Flows written under different policies.

use super::{Counter, Kind, Read, Scope, Seen, StateFilter, Tally, reply};
use crate::commands::flow::meta::Attributes;
use crate::commands::flow::term::Term;

fn attributes(entries: &[(&str, Term)]) -> Attributes {
    entries
        .iter()
        .map(|(name, value)| ((*name).to_owned(), value.clone()))
        .collect()
}

fn read(kind: Kind) -> Read {
    Read {
        flow_type: b"t".to_vec(),
        scope: Scope::Auto,
        kind,
    }
}

fn seen<'a>(attributes: &'a Attributes, indexed: &'a [String], expired: bool) -> Seen<'a> {
    Seen {
        state: b"completed",
        leased: false,
        expired,
        attributes,
        indexed,
    }
}

fn count(term: &Term) -> Option<&Term> {
    term.get(b"count")
}

#[test]
fn attribute_reads_skip_expired_flows_and_state_counts_keep_them() {
    let acme = attributes(&[("tenant", Term::text("acme"))]);
    let names = vec!["tenant".to_owned()];
    let completed = StateFilter::Only(b"completed".to_vec());
    let stats = read(Kind::Stats {
        state: completed.clone(),
        filters: acme.clone(),
    });
    let mut counter = Counter::new(&stats, &names);
    counter.add(&seen(&acme, &names, true));
    counter.add(&seen(&acme, &names, false));
    assert_eq!(count(&reply(&stats, counter.finish())), Some(&Term::Int(1)));
    let discovery = read(Kind::Names {
        state: completed.clone(),
        count: 10,
    });
    let mut counter = Counter::new(&discovery, &names);
    counter.add(&seen(&acme, &names, true));
    assert_eq!(reply(&discovery, counter.finish()), Term::List(Vec::new()));
    let plain = read(Kind::Stats {
        state: completed,
        filters: Attributes::new(),
    });
    let mut counter = Counter::new(&plain, &names);
    counter.add_count(b"completed", 2, 0);
    assert_eq!(count(&reply(&plain, counter.finish())), Some(&Term::Int(2)));
}

#[test]
fn state_any_counts_through_the_rarest_indexed_candidate() {
    // Flow `a` was written when only `tenant` was indexed, `b` when both
    // were: across states the `region` candidate is rarer, so only `b`
    // counts, as Elixir's candidate choice gives.
    let both = attributes(&[("tenant", Term::text("acme")), ("region", Term::text("us"))]);
    let tenant_only = vec!["tenant".to_owned()];
    let tenant_and_region = vec!["tenant".to_owned(), "region".to_owned()];
    let stats = read(Kind::Stats {
        state: StateFilter::Any,
        filters: both.clone(),
    });
    let mut first = Counter::new(&stats, &tenant_and_region);
    first.add(&seen(&both, &tenant_only, false));
    let mut second = Counter::new(&stats, &tenant_and_region);
    second.add(&seen(&both, &tenant_and_region, false));
    let joined = first.finish().merge(second.finish());
    assert_eq!(joined, Tally::Candidates(vec![(1, 1), (2, 2)]));
    assert_eq!(count(&reply(&stats, joined)), Some(&Term::Int(1)));
}

#[test]
fn shard_parts_join_by_name_and_value() {
    let values = read(Kind::Values {
        state: StateFilter::Only(b"queued".to_vec()),
        attribute: "tenant".to_owned(),
        count: 5,
    });
    let names = vec!["tenant".to_owned()];
    let acme = attributes(&[("tenant", Term::text("acme"))]);
    let listed = attributes(&[(
        "tenant",
        Term::List(vec![Term::text("beta"), Term::text("acme")]),
    )]);
    let mut first = Counter::new(&values, &names);
    first.add(&seen(&acme, &names, false));
    let mut second = Counter::new(&values, &names);
    second.add(&seen(&listed, &[], false));
    let entry = |value: &str, held: i64| {
        Term::Map(vec![
            (b"count".to_vec(), Term::Int(held)),
            (b"value".to_vec(), Term::text(value)),
        ])
    };
    assert_eq!(
        reply(&values, first.finish().merge(second.finish())),
        Term::List(vec![entry("acme", 2), entry("beta", 1)])
    );
}
