//! `FLOW.POLICY.SET` and `FLOW.POLICY.GET` (ADR-014 step 9): a set merges
//! into the stored policy and raises its generation, every shard applies it
//! as soon as the set replies, a retry follows its retry policy, and the
//! policies survive a restart with another shard count.

use wf_application::commands::flow::{self, FlowService, Term};
use wf_application::commands::queue::{QueueService, ServiceError};
use wf_node::composition::{QueueNode, QueueNodeConfig};
use wf_storage::log::GroupLogLimits;

use super::{TestResult, call, create, directory, open, open_with};

fn options(entries: &[(&str, Term)]) -> Term {
    Term::Map(
        entries
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect(),
    )
}

fn text(value: &str) -> Term {
    Term::text(value)
}

fn run(
    node: &QueueNode,
    name: &str,
    entries: &[(&str, Term)],
) -> TestResult<Result<Term, ServiceError>> {
    let command = flow::parse(name, &options(entries))?;
    call(|reply| node.execute(command, reply))
}

fn field(term: &Term, path: &[&str]) -> Term {
    path.iter().fold(term.clone(), |value, key| {
        value.get(key.as_bytes()).cloned().unwrap_or(Term::Nil)
    })
}

fn set(node: &QueueNode, entries: &[(&str, Term)]) -> TestResult<Result<Term, ServiceError>> {
    let mut all = vec![("type", text("tp"))];
    all.extend(entries.iter().cloned());
    run(node, "FLOW.POLICY.SET", &all)
}

fn get_flow(node: &QueueNode, id: &str, partition: &str) -> TestResult<Term> {
    Ok(run(
        node,
        "FLOW.GET",
        &[("id", text(id)), ("partition_key", text(partition))],
    )??)
}

/// Creates `id` in state `later` of partition `partition`.
fn create_later(node: &QueueNode, id: &str, partition: &str) -> TestResult {
    run(
        node,
        "FLOW.CREATE",
        &[
            ("id", text(id)),
            ("type", text("tp")),
            ("state", text("later")),
            ("partition_key", text(partition)),
            ("now_ms", Term::Int(1_000)),
        ],
    )??;
    Ok(())
}

fn retry_policy() -> Term {
    options(&[
        ("max_retries", Term::Int(1)),
        (
            "backoff",
            options(&[
                ("kind", text("fixed")),
                ("base_ms", Term::Int(50)),
                ("jitter_pct", Term::Int(0)),
            ]),
        ),
        ("exhausted_to", text("dead")),
    ])
}

/// Claims `id`'s type in `partition` at `now` and retries it.
fn claim_and_retry(node: &QueueNode, partition: &str, now: i64) -> TestResult {
    let parsed = flow::claim::parse(
        "FLOW.CLAIM_DUE",
        &options(&[
            ("type", text("tp")),
            ("worker", text("w")),
            ("now_ms", Term::Int(now)),
            ("partition_key", text(partition)),
            ("return", text("jobs_compact")),
        ]),
    )?;
    let shape = parsed.shape;
    let claimed = call(|reply| node.claim(parsed.request, reply))??;
    let Term::List(jobs) = flow::claim::reply(shape, claimed) else {
        return Err("claim reply is not a list".into());
    };
    let Some(Term::List(job)) = jobs.first() else {
        return Err("nothing claimed".into());
    };
    run(
        node,
        "FLOW.RETRY",
        &[
            ("id", job.first().cloned().unwrap_or(Term::Nil)),
            ("partition_key", text(partition)),
            ("lease_token", job.get(2).cloned().unwrap_or(Term::Nil)),
            ("fencing_token", job.get(3).cloned().unwrap_or(Term::Nil)),
            ("now_ms", Term::Int(now + 1)),
        ],
    )??;
    Ok(())
}

fn later_retention(entries: &[(&str, Term)]) -> Term {
    options(&[("later", options(&[("retention", options(entries))]))])
}

/// Sets, patches and replaces `tp`'s policy, ending at generation 3.
fn merge_patch_and_replace(node: &QueueNode) -> TestResult {
    let unset = run(node, "FLOW.POLICY.GET", &[("type", text("tp"))])??;
    assert_eq!(field(&unset, &["generation"]), Term::Int(0));
    assert_eq!(field(&unset, &["retry", "max_retries"]), Term::Int(3));
    let states = later_retention(&[
        ("ttl_ms", Term::Int(7_000)),
        ("history_max_events", Term::Int(9)),
    ]);
    let first = set(
        node,
        &[
            ("max_active_ms", Term::Int(90_000)),
            ("retry", retry_policy()),
            ("states", states),
        ],
    )??;
    assert_eq!(field(&first, &["generation"]), Term::Int(1));
    assert_eq!(
        field(&first, &["states", "later", "retention", "ttl_ms"]),
        Term::Int(7_000)
    );
    // A patch merges: the retry keeps its backoff, the state its retention.
    let patched = set(
        node,
        &[
            ("expected_generation", Term::Int(1)),
            ("retry", options(&[("max_retries", Term::Int(4))])),
            (
                "states",
                options(&[("later", options(&[("mode", text("FIFO"))]))]),
            ),
        ],
    )??;
    assert_eq!(field(&patched, &["generation"]), Term::Int(2));
    assert_eq!(field(&patched, &["retry", "max_retries"]), Term::Int(4));
    assert_eq!(
        field(&patched, &["retry", "backoff", "kind"]),
        text("fixed")
    );
    assert_eq!(field(&patched, &["states", "later", "mode"]), text("fifo"));
    assert_eq!(
        field(
            &patched,
            &["states", "later", "retention", "history_max_events"]
        ),
        Term::Int(9)
    );
    // A stale generation or an invalid field changes nothing.
    assert_eq!(
        set(node, &[("expected_generation", Term::Int(1))])?,
        Err(ServiceError::rejected(
            "ERR stale flow policy generation".to_owned()
        ))
    );
    assert_eq!(
        set(
            node,
            &[("retry", options(&[("max_attempts", Term::Int(2))]))]
        )?,
        Err(ServiceError::rejected(
            "ERR flow retry max_attempts was renamed to max_retries".to_owned()
        ))
    );
    let replaced = set(
        node,
        &[
            ("replace", Term::Bool(true)),
            ("max_active_ms", Term::Int(90_000)),
            ("retry", retry_policy()),
            ("states", later_retention(&[("ttl_ms", Term::Int(7_000))])),
        ],
    )??;
    assert_eq!(field(&replaced, &["generation"]), Term::Int(3));
    assert_eq!(field(&replaced, &["retry", "max_retries"]), Term::Int(1));
    Ok(())
}

/// Creates `f<index>` in partition `p<index>` for each index; every one
/// stores the policy's retention, wherever its shard.
fn every_shard_applies(node: &QueueNode, indexes: std::ops::Range<usize>) -> TestResult {
    for index in indexes {
        let (id, partition) = (format!("f{index}"), format!("p{index}"));
        create_later(node, &id, &partition)?;
        let created = get_flow(node, &id, &partition)?;
        assert_eq!(field(&created, &["retention_ttl_ms"]), Term::Int(7_000));
        assert_eq!(field(&created, &["max_active_ms"]), Term::Int(90_000));
    }
    Ok(())
}

/// A Flow that opts out of the active limit, retried once by the policy's
/// fixed backoff and then exhausted.
fn retry_follows_the_policy(node: &QueueNode) -> TestResult {
    run(
        node,
        "FLOW.CREATE",
        &[
            ("id", text("r1")),
            ("type", text("tp")),
            ("partition_key", text("pr")),
            ("now_ms", Term::Int(1_000)),
            ("run_at_ms", Term::Int(1_000)),
            ("max_active_ms", text("infinity")),
        ],
    )??;
    assert_eq!(
        field(&get_flow(node, "r1", "pr")?, &["max_active_ms"]),
        Term::Nil
    );
    claim_and_retry(node, "pr", 1_001)?;
    let retried = get_flow(node, "r1", "pr")?;
    assert_eq!(field(&retried, &["state"]), text("queued"));
    assert_eq!(field(&retried, &["next_run_at_ms"]), Term::Int(1_052));
    claim_and_retry(node, "pr", 1_100)
}

#[test]
fn policies_merge_apply_on_every_shard_and_survive_a_restart() -> TestResult {
    let path = directory("flow-policy")?;
    {
        let node = open(&path, 4)?;
        merge_patch_and_replace(&node)?;
        every_shard_applies(&node, 0..8)?;
        retry_follows_the_policy(&node)?;
    }
    let node = open(&path, 3)?;
    let exhausted = get_flow(&node, "r1", "pr")?;
    assert_eq!(field(&exhausted, &["state"]), text("dead"));
    assert_eq!(field(&exhausted, &["attempts"]), Term::Int(2));
    let after = run(
        &node,
        "FLOW.POLICY.GET",
        &[("type", text("tp")), ("state", text("later"))],
    )??;
    assert_eq!(field(&after, &["generation"]), Term::Int(3));
    assert_eq!(field(&after, &["retention", "ttl_ms"]), Term::Int(7_000));
    every_shard_applies(&node, 8..16)
}

#[test]
fn a_snapshot_keeps_policies_after_the_log_holding_them_is_gone() -> TestResult {
    let path = directory("flow-policy-snapshot")?;
    let config = |shards| QueueNodeConfig {
        shards,
        log: GroupLogLimits {
            max_segment_bytes: 4_096,
            ..QueueNodeConfig::default().log
        },
        snapshot_segments: 0,
        ..QueueNodeConfig::default()
    };
    {
        let node = open_with(&path, config(4))?;
        set(
            &node,
            &[(
                "states",
                options(&[(
                    "later",
                    options(&[("retention", options(&[("ttl_ms", Term::Int(7_000))]))]),
                )]),
            )],
        )??;
        let ids: Vec<String> = (0..300).map(|index| format!("fill{index}")).collect();
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        for chunk in ids.chunks(100) {
            assert!(create(&node, chunk)?.iter().all(Result::is_ok));
        }
        assert_eq!(node.snapshot()?, 1);
    }
    let node = open_with(&path, config(7))?;
    let policy = run(&node, "FLOW.POLICY.GET", &[("type", text("tp"))])??;
    assert_eq!(field(&policy, &["generation"]), Term::Int(1));
    for index in 0..8 {
        let (id, partition) = (format!("f{index}"), format!("p{index}"));
        create_later(&node, &id, &partition)?;
        let created = get_flow(&node, &id, &partition)?;
        assert_eq!(field(&created, &["retention_ttl_ms"]), Term::Int(7_000));
    }
    Ok(())
}
