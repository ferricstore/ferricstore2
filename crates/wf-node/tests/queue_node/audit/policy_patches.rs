//! `flow_policy_generation_test.exs` (rest-of-suite audit): policy patches
//! merge per state and field, generations advance, concurrent patches all
//! land, and a replace resets what it omits.

use std::sync::Arc;

use wf_application::commands::flow::Term;
use wf_node::composition::QueueNode;

use super::{field, int, map, ok, refusal, run, text};
use crate::{TestResult, directory, open};

fn set(node: &QueueNode, flow_type: &str, entries: &[(&str, Term)]) -> TestResult<Term> {
    let mut all = vec![("type", text(flow_type))];
    all.extend(entries.iter().cloned());
    ok(node, "FLOW.POLICY.SET", &all)
}

fn states(node: &QueueNode, flow_type: &str) -> TestResult<Term> {
    Ok(field(
        &ok(node, "FLOW.POLICY.GET", &[("type", text(flow_type))])?,
        "states",
    ))
}

#[test]
fn state_patches_merge_by_state_and_by_field() -> TestResult {
    // Elixir: "policy set merges independent type and state patches",
    // "string-key state patches override the equivalent normalized
    // setting" and "policy set validates the optional response state".
    let node = open(&directory("audit-rest-policy-patches")?, 4)?;
    let first = set(
        &node,
        "merge",
        &[(
            "states",
            map(&[(
                "queued",
                map(&[
                    ("mode", text("fifo")),
                    ("retry", map(&[("max_retries", int(7))])),
                ]),
            )]),
        )],
    )?;
    assert_eq!(field(&first, "generation"), int(1));
    let second = set(
        &node,
        "merge",
        &[(
            "states",
            map(&[
                (
                    "queued",
                    map(&[("retry", map(&[("exhausted_to", text("dead"))]))]),
                ),
                ("review", map(&[("mode", text("fifo"))])),
            ]),
        )],
    )?;
    assert_eq!(field(&second, "generation"), int(2));
    let merged = states(&node, "merge")?;
    let queued = field(&merged, "queued");
    assert_eq!(field(&queued, "mode"), text("fifo"));
    assert_eq!(field(&field(&queued, "retry"), "max_retries"), int(7));
    assert_eq!(
        field(&field(&queued, "retry"), "exhausted_to"),
        text("dead")
    );
    assert_eq!(field(&field(&merged, "review"), "mode"), text("fifo"));
    set(
        &node,
        "merge",
        &[(
            "states",
            map(&[(
                "queued",
                map(&[
                    ("mode", text("parallel")),
                    ("retry", map(&[("max_retries", int(9))])),
                ]),
            )]),
        )],
    )?;
    let queued = field(&states(&node, "merge")?, "queued");
    assert_eq!(field(&queued, "mode"), text("parallel"));
    assert_eq!(field(&field(&queued, "retry"), "max_retries"), int(9));
    let refused = run(
        &node,
        "FLOW.POLICY.SET",
        &[("type", text("merge")), ("state", int(1))],
    )?;
    assert_eq!(refusal(refused)?, "ERR flow state must be a string");
    Ok(())
}

#[test]
fn concurrent_patches_all_land() -> TestResult {
    // Elixir: "concurrent policy patches preserve both writers" and "a
    // burst of concurrent policy patches preserves every writer".
    let node = Arc::new(open(&directory("audit-rest-policy-burst")?, 4)?);
    let writers: Vec<_> = (0..12)
        .map(|index| {
            let node = Arc::clone(&node);
            std::thread::spawn(move || -> Result<(), String> {
                let mode = if index % 2 == 0 { "fifo" } else { "parallel" };
                let state = format!("s{index}");
                set(
                    &node,
                    "burst",
                    &[(
                        "states",
                        map(&[(state.as_str(), map(&[("mode", text(mode))]))]),
                    )],
                )
                .map(drop)
                .map_err(|error| error.to_string())
            })
        })
        .collect();
    for writer in writers {
        writer.join().map_err(|_| "a writer panicked")??;
    }
    let policy = ok(&node, "FLOW.POLICY.GET", &[("type", text("burst"))])?;
    assert_eq!(field(&policy, "generation"), int(12));
    let Term::Map(all) = field(&policy, "states") else {
        return Err("states expected".into());
    };
    assert_eq!(all.len(), 12);
    Ok(())
}
