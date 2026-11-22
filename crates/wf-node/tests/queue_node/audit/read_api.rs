//! `flow/read_api_test.exs`, its client-visible cases: attribute
//! discovery keeps a 256-byte value whole, and a lineage query finds a
//! Flow in its auto partition. (Its scan counts, LMDB rows, configured
//! limits and `:auto` atoms are Elixir's; the SDK checks query times before
//! sending.)

use wf_application::commands::flow::Term;

use super::{field, get, int, map, ok, text};
use crate::{TestResult, directory, open};

#[test]
fn attribute_discovery_keeps_a_256_byte_value_whole() -> TestResult {
    // Elixir: "attribute discovery retains maximum-size string values
    // outside the LMDB key" (its NUL-bearing value is in
    // `flow_info::discovery_counts_policy_indexed_values_of_live_flows`).
    let node = open(&directory("audit-read-long-value")?, 4)?;
    let long = "v".repeat(256);
    ok(
        &node,
        "FLOW.POLICY.SET",
        &[
            ("type", text("jobs")),
            ("indexed_attributes", Term::List(vec![text("color")])),
        ],
    )?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("flow-1")),
            ("type", text("jobs")),
            ("partition_key", text("partition-1")),
            ("attributes", map(&[("color", text(&long))])),
            ("run_at_ms", int(1_000)),
        ],
    )?;
    let values = ok(
        &node,
        "FLOW.ATTRIBUTE_VALUES",
        &[
            ("type", text("jobs")),
            ("attribute", text("color")),
            ("partition_key", text("partition-1")),
            ("count", int(10)),
        ],
    )?;
    assert_eq!(
        values,
        Term::List(vec![map(&[("count", int(1)), ("value", text(&long))])])
    );
    Ok(())
}

#[test]
fn a_lineage_query_finds_a_flow_in_its_auto_partition() -> TestResult {
    // Elixir: "list applies terminal-only filtering before truncating
    // active state indexes", its root query in an auto partition (its
    // terminal-only list has no SDK form: `list` takes no terminal_only,
    // and a terminal search refuses state `queued`).
    let node = open(&directory("audit-read-auto-root")?, 4)?;
    ok(
        &node,
        "FLOW.CREATE",
        &[
            ("id", text("active-list")),
            ("type", text("jobs")),
            ("state", text("queued")),
        ],
    )?;
    let partition = field(&get(&node, "active-list", &[])?, "partition_key");
    let query = "FROM runs WHERE partition_key = @partition_key AND root_flow_id = @lineage_id ORDER BY updated_at_ms ASC LIMIT 1 RETURN RECORDS";
    let result = ok(
        &node,
        "FLOW.QUERY",
        &[
            ("version", text("FQL1")),
            ("query", text(query)),
            (
                "params",
                map(&[
                    ("partition_key", partition),
                    ("lineage_id", text("active-list")),
                ]),
            ),
        ],
    )?;
    let Term::List(records) = field(&result, "records") else {
        return Err(format!("no records in {result:?}").into());
    };
    let ids: Vec<Term> = records.iter().map(|record| field(record, "id")).collect();
    assert_eq!(ids, [text("active-list")]);
    Ok(())
}
