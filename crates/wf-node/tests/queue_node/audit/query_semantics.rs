//! `flow/query/engine_test.exs`, its query semantics through `FLOW.QUERY`
//! (the SDK builders' text): terminal reads keep every state that shares an
//! update time, metadata searches add no state filter, stuck reads order by
//! lease deadline before limiting and keep both bounds, and a missing run
//! has an empty history. (Its engine installation, capability manifests,
//! storage outages and index seeks are Elixir's.)

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{claimed, field, int, map, ok, text, texts};
use crate::{TestResult, directory, open};

const P: &str = "query-semantics";

fn query(node: &QueueNode, fql: &str, params: &[(&str, Term)]) -> TestResult<Term> {
    let mut all = vec![("partition_key", text(P))];
    all.extend(params.iter().cloned());
    ok(
        node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            ("query", text(fql)),
            ("params", map(&all)),
        ],
    )
}

fn ids(result: &Term) -> TestResult<Vec<Term>> {
    match field(result, "records") {
        Term::List(records) => Ok(records.iter().map(|record| field(record, "id")).collect()),
        other => Err(format!("records expected, got {other:?}").into()),
    }
}

fn create(node: &QueueNode, id: &str, flow_type: &str, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text(flow_type)),
        ("partition_key", text(P)),
    ];
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

#[test]
fn a_terminal_read_keeps_every_state_sharing_an_update_time() -> TestResult {
    // Elixir: "terminal query returns every state when records share an
    // update timestamp".
    let node = open(&directory("audit-semantics-terminals")?, 1)?;
    for state in ["completed", "failed", "cancelled"] {
        create(
            &node,
            state,
            "terminals",
            &[("state", text(state)), ("now_ms", int(5_000))],
        )?;
    }
    let fql = "FROM runs WHERE partition_key = @partition_key AND type = @type AND state IN (@terminal_0, @terminal_1, @terminal_2) ORDER BY updated_at_ms DESC LIMIT 20 RETURN RECORDS";
    let result = query(
        &node,
        fql,
        &[
            ("type", text("terminals")),
            ("terminal_0", text("completed")),
            ("terminal_1", text("failed")),
            ("terminal_2", text("cancelled")),
        ],
    )?;
    let mut found = ids(&result)?;
    found.sort_by_key(|id| format!("{id:?}"));
    let mut expected = vec![text("cancelled"), text("completed"), text("failed")];
    expected.sort_by_key(|id| format!("{id:?}"));
    assert_eq!(found, expected);
    Ok(())
}

#[test]
fn a_metadata_search_adds_no_state_filter() -> TestResult {
    // Elixir: "default metadata queries do not invent a queued-state
    // filter".
    let node = open(&directory("audit-semantics-metadata")?, 1)?;
    ok(
        &node,
        "FLOW.POLICY.SET",
        &[
            ("type", text("metadata")),
            ("indexed_attributes", texts(&["region"])),
        ],
    )?;
    for (id, state, now, region) in [
        ("queued-eu", "queued", 1_000, "eu"),
        ("failed-eu", "failed", 2_000, "eu"),
        ("cancelled-eu", "cancelled", 2_500, "eu"),
        ("queued-us", "queued", 3_000, "us"),
    ] {
        create(
            &node,
            id,
            "metadata",
            &[
                ("state", text(state)),
                ("now_ms", int(now)),
                ("attributes", map(&[("region", text(region))])),
            ],
        )?;
    }
    let search = "FROM runs WHERE partition_key = @partition_key AND type = @type AND attribute['region'] = @attribute_0 ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS";
    let found = query(
        &node,
        search,
        &[("type", text("metadata")), ("attribute_0", text("eu"))],
    )?;
    assert_eq!(
        ids(&found)?,
        [text("queued-eu"), text("failed-eu"), text("cancelled-eu")]
    );
    let terminal = "FROM runs WHERE partition_key = @partition_key AND type = @type AND state IN (@completed, @failed) AND attribute.region = @region ORDER BY updated_at_ms ASC LIMIT 10 RETURN RECORDS";
    let terminal_params = [
        ("type", text("metadata")),
        ("completed", text("completed")),
        ("failed", text("failed")),
        ("region", text("eu")),
    ];
    assert_eq!(
        ids(&query(&node, terminal, &terminal_params)?)?,
        [text("failed-eu")]
    );
    Ok(())
}

fn claim_with_lease(node: &QueueNode, flow_type: &str, worker: &str, lease_ms: i64) -> TestResult {
    let records = claimed(
        node,
        &[
            ("type", text(flow_type)),
            ("partition_key", text(P)),
            ("worker", text(worker)),
            ("lease_ms", int(lease_ms)),
            ("limit", int(1)),
            ("now_ms", int(1_000)),
        ],
    )?;
    assert_eq!(records.len(), 1);
    Ok(())
}

const STUCK: &str = "FROM runs WHERE partition_key = @partition_key AND type = @type AND state = @state AND lease_deadline_ms BETWEEN @lease_from_ms AND @lease_to_ms ORDER BY lease_deadline_ms";

#[test]
fn stuck_reads_order_by_lease_deadline_and_keep_both_bounds() -> TestResult {
    // Elixir: "default stuck queries honor descending lease-deadline order
    // before limiting" and "stuck queries enforce both lease-deadline bounds
    // without widening the index read".
    let node = open(&directory("audit-semantics-stuck")?, 1)?;
    for (id, worker, lease_ms) in [
        ("earlier", "worker-earlier", 50),
        ("later", "worker-later", 100),
    ] {
        create(
            &node,
            id,
            "stuck",
            &[("run_at_ms", int(1_000)), ("now_ms", int(1_000))],
        )?;
        claim_with_lease(&node, "stuck", worker, lease_ms)?;
    }
    let window = |from: i64, to: i64| {
        vec![
            ("type", text("stuck")),
            ("state", text("running")),
            ("lease_from_ms", int(from)),
            ("lease_to_ms", int(to)),
        ]
    };
    let newest = query(
        &node,
        &format!("{STUCK} DESC LIMIT 1 RETURN RECORDS"),
        &window(0, 1_101),
    )?;
    assert_eq!(ids(&newest)?, [text("later")]);
    let bounded = query(
        &node,
        &format!("{STUCK} ASC LIMIT 10 RETURN RECORDS"),
        &window(1_075, 1_200),
    )?;
    assert_eq!(ids(&bounded)?, [text("later")]);
    Ok(())
}

#[test]
fn a_missing_run_has_an_empty_history_page() -> TestResult {
    // Elixir: "missing runs return a complete empty history page contract".
    let node = open(&directory("audit-semantics-missing-history")?, 1)?;
    let history = ok(
        &node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            (
                "query",
                text(
                    "FROM events WHERE partition_key = @partition_key AND run_id = @run_id ORDER BY event_id ASC LIMIT 10 RETURN RECORDS",
                ),
            ),
            (
                "params",
                map(&[("partition_key", text(P)), ("run_id", text("missing"))]),
            ),
        ],
    )?;
    assert_eq!(field(&history, "records"), Term::List(Vec::new()));
    assert_eq!(field(&history, "cursor"), Term::Nil);
    Ok(())
}

#[test]
fn a_flow_created_finished_keeps_its_retention_from_command_time() -> TestResult {
    // The node's own check (audit gap G40), found by the terminal-read port:
    // a Flow created in a terminal state with a client clock in the past
    // keeps its retention from the command's own time, as a Flow that
    // finishes later does (G13), instead of being removed at once.
    let node = open(&directory("audit-semantics-created-finished")?, 1)?;
    create(
        &node,
        "done",
        "finished",
        &[("state", text("completed")), ("now_ms", int(5_000))],
    )?;
    std::thread::sleep(std::time::Duration::from_millis(1_200));
    let record = super::get(&node, "done", &[("partition_key", text(P))])?;
    assert_eq!(field(&record, "state"), text("completed"));
    let Term::Int(until) = field(&record, "terminal_retention_until_ms") else {
        return Err(format!("a retention deadline expected in {record:?}").into());
    };
    let wall = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?;
    assert!(until > wall, "{until} is not after {wall}");
    Ok(())
}
