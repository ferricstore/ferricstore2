//! `flow/history_retention_restart_test.exs` (rest-of-suite audit): an id
//! created again after retention removed its Flow, across a restart,
//! starts a new history.

use std::time::{Duration, Instant};

use wf_application::commands::flow::Term;
use wf_node::composition::{QueueNode, QueueNodeConfig};

use super::{events, field, get, int, ok, text};
use crate::{TestResult, directory, open_with};

fn node(path: &std::path::Path) -> TestResult<QueueNode> {
    open_with(
        path,
        QueueNodeConfig {
            shards: 4,
            retention_ms: 200,
            ..QueueNodeConfig::default()
        },
    )
}

#[test]
fn a_flow_id_reused_after_retention_and_a_restart_starts_a_new_history() -> TestResult {
    let path = directory("audit-rest-retention-reuse")?;
    let create = |node: &QueueNode| {
        ok(
            node,
            "FLOW.CREATE",
            &[
                ("id", text("reused")),
                ("type", text("t")),
                ("partition_key", text("p")),
            ],
        )
    };
    {
        let first = node(&path)?;
        create(&first)?;
        ok(
            &first,
            "FLOW.CANCEL",
            &[
                ("id", text("reused")),
                ("partition_key", text("p")),
                ("fencing_token", int(0)),
            ],
        )?;
        let started = Instant::now();
        while get(&first, "reused", &[("partition_key", text("p"))])? != Term::Nil {
            if started.elapsed() > Duration::from_secs(10) {
                return Err("retention never removed the Flow".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let second = node(&path)?;
    create(&second)?;
    let history = events(&second, "reused", &[("partition_key", text("p"))])?;
    assert_eq!(history.len(), 1, "{history:?}");
    assert_eq!(
        history.first().map(|event| field(event, "event")),
        Some(text("created"))
    );
    Ok(())
}
