//! `FLOW.QUERY` ordered by update time walks the shard's catalog in that
//! order (Elixir's `native` order over its `flow_runs_tenant_*_updated`
//! indexes): a page reads only the rows it returns, a cursor seeks to its
//! row, and pages keep the sorted order, ties broken by the SHA-256 of the
//! id. Any other order is a bounded top-k over the candidates.

use sha2::{Digest, Sha256};
use wf_application::commands::flow::{self, FlowService, Term};
use wf_node::composition::QueueNode;

use super::{TestResult, call, directory, open};

fn object(entries: Vec<(&str, Term)>) -> Term {
    Term::Map(
        entries
            .into_iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value))
            .collect(),
    )
}

/// One seeded Flow: id, type, partition, state, update time, priority.
type Seed = (String, &'static str, &'static str, &'static str, i64, i64);

fn create(node: &QueueNode, seed: &Seed) -> TestResult {
    let (id, flow_type, partition, state, now, priority) = seed;
    let command = flow::parse(
        "FLOW.CREATE",
        &object(vec![
            ("id", Term::text(id)),
            ("type", Term::text(flow_type)),
            ("partition_key", Term::text(partition)),
            ("state", Term::text(state)),
            ("now_ms", Term::Int(*now)),
            ("run_at_ms", Term::Int(*now)),
            ("priority", Term::Int(*priority)),
        ]),
    )?;
    call(|reply| node.execute(command, reply))??;
    Ok(())
}

fn query(node: &QueueNode, text: &str, cursor: Option<Term>) -> TestResult<Term> {
    let mut params = Vec::new();
    let text = match cursor {
        Some(cursor) => {
            params.push(("c", cursor));
            text.replace(" RETURN", " CURSOR @c RETURN")
        }
        None => text.to_owned(),
    };
    let command = flow::parse(
        "FLOW.QUERY",
        &object(vec![
            ("version", Term::text("FQL1")),
            ("query", Term::text(&text)),
            ("params", object(params)),
        ]),
    )?;
    Ok(call(|reply| node.execute(command, reply))??)
}

fn at(term: &Term, keys: &[&str]) -> Term {
    keys.iter().fold(term.clone(), |value, key| {
        value.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
    })
}

fn ids(result: &Term) -> Vec<String> {
    match result.get(b"records") {
        Some(Term::List(records)) => records
            .iter()
            .filter_map(|record| match record.get(b"id") {
                Some(Term::Bin(id)) => Some(String::from_utf8_lossy(id).into_owned()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Every page of `text` (which ends in `LIMIT n RETURN RECORDS`), in turn.
fn pages(node: &QueueNode, text: &str) -> TestResult<Vec<String>> {
    let mut all = Vec::new();
    let mut cursor = None;
    for _ in 0..100 {
        let page = query(node, text, cursor.take())?;
        all.extend(ids(&page));
        match at(&page, &["page", "cursor"]) {
            Term::Nil => return Ok(all),
            next => cursor = Some(next),
        }
    }
    Err("the pages never ended".into())
}

/// The seeded ids `keep` selects, by update time (`descending` or not),
/// then by the SHA-256 of the id.
fn sorted(seeds: &[Seed], descending: bool, keep: impl Fn(&Seed) -> bool) -> Vec<String> {
    let mut kept: Vec<(i64, [u8; 32], String)> = seeds
        .iter()
        .filter(|seed| keep(seed))
        .map(|seed| {
            let time = if descending { -seed.4 } else { seed.4 };
            (
                time,
                Sha256::digest(seed.0.as_bytes()).into(),
                seed.0.clone(),
            )
        })
        .collect();
    kept.sort();
    kept.into_iter().map(|(_, _, id)| id).collect()
}

/// Thirty `tq` Flows in partition `p`, four to an update time, a third of
/// them waiting and half of priority 1; three `other` Flows beside them;
/// three `tq` Flows in partition `q`.
fn seeds() -> Vec<Seed> {
    let mut seeds: Vec<Seed> = (0_i64..30)
        .map(|index| {
            let state = if index % 3 == 0 { "waiting" } else { "ready" };
            (
                format!("r{index:02}"),
                "tq",
                "p",
                state,
                1_000 + index / 4,
                index % 2,
            )
        })
        .collect();
    for index in 0..3 {
        seeds.push((format!("o{index}"), "other", "p", "ready", 1_002, 0));
        seeds.push((format!("x{index}"), "tq", "q", "ready", 1_003, 0));
    }
    seeds
}

fn plan_order(node: &QueueNode, text: &str) -> TestResult<Term> {
    Ok(at(
        &query(node, &format!("EXPLAIN {text}"), None)?,
        &["plan", "order"],
    ))
}

#[test]
fn explain_names_the_order_a_query_runs_in() -> TestResult {
    let node = open(&directory("query-order-plan")?, 4)?;
    for seed in &seeds() {
        create(&node, seed)?;
    }
    let scope = "FROM runs WHERE partition_key = 'p' AND type = 'tq'";
    for (text, order) in [
        (
            format!("{scope} AND state = 'ready' ORDER BY updated_at_ms DESC LIMIT 2 RETURN RECORDS"),
            "native",
        ),
        (
            format!("{scope} ORDER BY updated_at_ms ASC LIMIT 2 RETURN RECORDS"),
            "native",
        ),
        (
            "FROM runs WHERE partition_key = 'p' ORDER BY updated_at_ms DESC LIMIT 2 RETURN RECORDS"
                .to_owned(),
            "native",
        ),
        (
            format!("{scope} ORDER BY priority ASC LIMIT 2 RETURN RECORDS"),
            "bounded_top_k",
        ),
        (
            format!("{scope} ORDER BY updated_at_ms DESC, created_at_ms DESC LIMIT 2 RETURN RECORDS"),
            "bounded_top_k",
        ),
        // An order field an equality fixes orders nothing.
        (
            format!(
                "{scope} AND priority = 1 ORDER BY priority ASC, updated_at_ms DESC LIMIT 2 RETURN RECORDS"
            ),
            "native",
        ),
        // So does an `IN` of one value, but not of two, nor one on another
        // field.
        (
            format!(
                "{scope} AND priority IN (1) ORDER BY priority ASC, updated_at_ms DESC LIMIT 2 RETURN RECORDS"
            ),
            "native",
        ),
        (
            format!(
                "{scope} AND priority IN (0, 1) ORDER BY priority ASC, updated_at_ms DESC LIMIT 2 RETURN RECORDS"
            ),
            "bounded_top_k",
        ),
        (
            format!(
                "{scope} AND state IN ('ready') ORDER BY priority ASC, updated_at_ms DESC LIMIT 2 RETURN RECORDS"
            ),
            "bounded_top_k",
        ),
        (format!("{scope} RETURN COUNT"), "none"),
    ] {
        assert_eq!(plan_order(&node, &text)?, Term::text(order), "{text}");
    }
    Ok(())
}

#[test]
fn a_page_in_update_order_reads_only_the_rows_it_returns() -> TestResult {
    let node = open(&directory("query-order-reads")?, 4)?;
    for index in 0..200 {
        create(
            &node,
            &(format!("n{index:03}"), "tq", "p", "ready", 1_000 + index, 0),
        )?;
    }
    let text = "FROM runs WHERE partition_key = 'p' AND type = 'tq' AND state = 'ready' ORDER BY updated_at_ms DESC LIMIT 2 RETURN RECORDS";
    let first = query(&node, text, None)?;
    assert_eq!(ids(&first), ["n199", "n198"]);
    // Two rows and the one that shows another page follows, each tested by
    // the three predicates.
    assert_eq!(at(&first, &["usage", "scanned_entries"]), Term::Int(3));
    assert_eq!(at(&first, &["usage", "residual_checks"]), Term::Int(9));
    let analyzed = query(&node, &format!("EXPLAIN ANALYZE {text}"), None)?;
    assert_eq!(at(&analyzed, &["actual", "scanned_entries"]), Term::Int(3));
    // A cursor seeks to its own row, which is passed over, then reads on.
    let second = query(&node, text, Some(at(&first, &["page", "cursor"])))?;
    assert_eq!(ids(&second), ["n197", "n196"]);
    assert_eq!(at(&second, &["usage", "scanned_entries"]), Term::Int(4));
    // The last page reads to the end of the set: its cursor row and its
    // hundred rows.
    let ascending = text.replace("DESC LIMIT 2", "ASC LIMIT 100");
    let half = query(&node, &ascending, None)?;
    let last = query(&node, &ascending, Some(at(&half, &["page", "cursor"])))?;
    assert_eq!(ids(&last).first().map(String::as_str), Some("n100"));
    assert_eq!(ids(&last).last().map(String::as_str), Some("n199"));
    assert_eq!(at(&last, &["usage", "scanned_entries"]), Term::Int(101));
    assert_eq!(at(&last, &["page", "has_more"]), Term::Bool(false));
    // A cursor finds the update time past an order field that `IS MISSING`
    // fixes (a Flow with no `max_active_ms`).
    let untimed = text.replace(
        "ORDER BY updated_at_ms DESC",
        "AND max_active_ms IS MISSING ORDER BY max_active_ms ASC, updated_at_ms DESC",
    );
    let first = query(&node, &untimed, None)?;
    assert_eq!(ids(&first), ["n199", "n198"]);
    let second = query(&node, &untimed, Some(at(&first, &["page", "cursor"])))?;
    assert_eq!(ids(&second), ["n197", "n196"]);
    assert_eq!(at(&second, &["usage", "scanned_entries"]), Term::Int(4));
    Ok(())
}

#[test]
fn pages_in_update_order_keep_the_sorted_order_across_ties() -> TestResult {
    let node = open(&directory("query-order-pages")?, 4)?;
    let seeds = seeds();
    for seed in &seeds {
        create(&node, seed)?;
    }
    let in_p = |seed: &Seed| seed.2 == "p";
    let tq = |seed: &Seed| in_p(seed) && seed.1 == "tq";
    let ready = |seed: &Seed| tq(seed) && seed.3 == "ready";
    let scope = "FROM runs WHERE partition_key = 'p'";
    let cases: Vec<(String, Vec<String>)> = vec![
        (
            format!("{scope} AND type = 'tq' AND state = 'ready' ORDER BY updated_at_ms DESC"),
            sorted(&seeds, true, ready),
        ),
        (
            format!("{scope} AND type = 'tq' AND state = 'ready' ORDER BY updated_at_ms ASC"),
            sorted(&seeds, false, ready),
        ),
        (
            format!(
                "{scope} AND type = 'tq' AND state IN ('ready', 'waiting') ORDER BY updated_at_ms DESC"
            ),
            sorted(&seeds, true, tq),
        ),
        (
            format!("{scope} ORDER BY updated_at_ms DESC"),
            sorted(&seeds, true, in_p),
        ),
        (
            format!("{scope} AND state = 'ready' ORDER BY updated_at_ms ASC"),
            sorted(&seeds, false, |seed| in_p(seed) && seed.3 == "ready"),
        ),
        (
            format!(
                "{scope} AND type = 'tq' AND state = 'ready' AND priority = 1 ORDER BY updated_at_ms DESC"
            ),
            sorted(&seeds, true, |seed| ready(seed) && seed.5 == 1),
        ),
        // A fixed order field first; then the same order by the top-k path,
        // as no seeded Flow was updated after it was created.
        (
            format!(
                "{scope} AND type = 'tq' AND priority = 1 ORDER BY priority DESC, updated_at_ms DESC"
            ),
            sorted(&seeds, true, |seed| tq(seed) && seed.5 == 1),
        ),
        (
            format!(
                "{scope} AND type = 'tq' AND state = 'ready' ORDER BY updated_at_ms DESC, created_at_ms DESC"
            ),
            sorted(&seeds, true, ready),
        ),
    ];
    for (text, expected) in cases {
        for limit in [3, 4, 100] {
            let paged = pages(&node, &format!("{text} LIMIT {limit} RETURN RECORDS"))?;
            assert_eq!(paged, expected, "{text} LIMIT {limit}");
        }
    }
    Ok(())
}
