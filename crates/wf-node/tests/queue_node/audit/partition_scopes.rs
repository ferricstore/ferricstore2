//! `flow_test/sections/flow_native_due_index_mirrors_create_claim_due.exs`:
//! a partition scopes every command on a Flow; claims take one partition,
//! listed ones, auto buckets or any partition, every due state when none is
//! named, higher priorities first across shards, and share small limits
//! fairly over shards. (Its native index tables are Elixir's.)

use wf_application::commands::flow::Term;
use wf_application::commands::queue::{Partition, shard_for_slot};
use wf_node::composition::QueueNode;

use super::{claimed, events, field, get, int, ok, refusal, run, text, texts};
use crate::{TestResult, directory, open};

const SHARDS: u16 = 4;

fn create(node: &QueueNode, id: &str, flow_type: &str, extra: &[(&str, Term)]) -> TestResult {
    let mut entries = vec![
        ("id", text(id)),
        ("type", text(flow_type)),
        ("run_at_ms", int(1_000)),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    ok(node, "FLOW.CREATE", &entries)?;
    Ok(())
}

fn claim_with(node: &QueueNode, flow_type: &str, extra: &[(&str, Term)]) -> TestResult<Vec<Term>> {
    let mut entries = vec![
        ("type", text(flow_type)),
        ("worker", text("worker-a")),
        ("lease_ms", int(30_000)),
        ("limit", int(10)),
        ("now_ms", int(1_000)),
    ];
    entries.retain(|(key, _)| !extra.iter().any(|(given, _)| given == key));
    entries.extend(extra.iter().cloned());
    claimed(node, &entries)
}

fn ids(records: &[Term]) -> Vec<Term> {
    let mut ids: Vec<Term> = records.iter().map(|record| field(record, "id")).collect();
    ids.sort_by(|left, right| match (left, right) {
        (Term::Bin(left), Term::Bin(right)) => left.cmp(right),
        _ => std::cmp::Ordering::Equal,
    });
    ids
}

/// The shard a named partition lives on with `SHARDS` shards.
fn shard_of(partition: &str) -> Option<u16> {
    shard_for_slot(Partition::parse(partition.as_bytes()).slot(), SHARDS)
}

/// Two partition names, `prefix`-numbered, on different shards.
fn on_two_shards(prefix: &str) -> TestResult<(String, String)> {
    let first = format!("{prefix}-0");
    let home = shard_of(&first);
    let other = (1..512)
        .map(|index| format!("{prefix}-{index}"))
        .find(|name| shard_of(name) != home)
        .ok_or("no partition on another shard")?;
    Ok((first, other))
}

#[test]
fn a_partition_scopes_claims_reads_completions_and_history() -> TestResult {
    // Elixir: "partition_key scopes claim, complete, retry, get, and
    // history" and "Flow native due index mirrors create and claim_due"
    // (its index tables are Elixir's).
    let node = open(&directory("audit-scopes-commands")?, SHARDS)?;
    create(
        &node,
        "scoped",
        "scoped",
        &[
            ("partition_key", text("tenant")),
            ("state", text("queued")),
            ("now_ms", int(999)),
        ],
    )?;
    assert_eq!(get(&node, "scoped", &[])?, Term::Nil);
    assert_eq!(
        field(
            &get(&node, "scoped", &[("partition_key", text("tenant"))])?,
            "id"
        ),
        text("scoped")
    );
    assert_eq!(
        claim_with(
            &node,
            "scoped",
            &[("state", text("queued")), ("limit", int(1))]
        )?,
        Vec::<Term>::new()
    );
    let records = claim_with(
        &node,
        "scoped",
        &[
            ("partition_key", text("tenant")),
            ("state", text("queued")),
            ("limit", int(1)),
        ],
    )?;
    let [lease] = records.as_slice() else {
        return Err("one claim".into());
    };
    assert_eq!(field(lease, "partition_key"), text("tenant"));
    let finish = |partition: Option<&str>| {
        let mut entries = vec![
            ("id", text("scoped")),
            ("lease_token", field(lease, "lease_token")),
            ("fencing_token", field(lease, "fencing_token")),
        ];
        entries.extend(partition.map(|partition| ("partition_key", text(partition))));
        run(&node, "FLOW.COMPLETE", &entries)
    };
    assert_eq!(refusal(finish(None)?)?, "ERR flow not found");
    finish(Some("tenant"))??;
    assert_eq!(
        field(
            &get(&node, "scoped", &[("partition_key", text("tenant"))])?,
            "state"
        ),
        text("completed")
    );
    let names: Vec<Term> = events(&node, "scoped", &[("partition_key", text("tenant"))])?
        .iter()
        .map(|fields| field(fields, "event"))
        .collect();
    assert_eq!(names, [text("created"), text("claimed"), text("completed")]);
    assert_eq!(
        claim_with(&node, "scoped", &[("partition_key", text("tenant"))])?,
        Vec::<Term>::new()
    );
    Ok(())
}

#[test]
fn claims_take_one_partition_listed_ones_or_every_auto_bucket() -> TestResult {
    // Elixir: "flow_claim_due only scans the selected partition", "...
    // can scan selected partition_keys in one call" and "flow_create
    // without partition auto-spreads and default claim_due fetches across
    // buckets".
    let node = open(&directory("audit-scopes-partitions")?, SHARDS)?;
    for (id, partition) in [("a", "pa"), ("b", "pb"), ("c", "pc")] {
        create(&node, id, "scan", &[("partition_key", text(partition))])?;
    }
    assert_eq!(
        ids(&claim_with(
            &node,
            "scan",
            &[("partition_key", text("pa"))]
        )?),
        [text("a")]
    );
    assert_eq!(
        claim_with(&node, "scan", &[("partition_key", text("pa"))])?,
        Vec::<Term>::new()
    );
    let listed = claim_with(&node, "scan", &[("partition_keys", texts(&["pb", "pc"]))])?;
    assert_eq!(ids(&listed), [text("b"), text("c")]);

    let spread: Vec<String> = (0..32).map(|index| format!("auto-{index}")).collect();
    for id in &spread {
        create(&node, id, "spread", &[])?;
    }
    let mut buckets: Vec<Term> = spread
        .iter()
        .map(|id| field(&get(&node, id, &[]).unwrap_or(Term::Nil), "partition_key"))
        .collect();
    buckets.sort_by_key(|bucket| format!("{bucket:?}"));
    buckets.dedup();
    assert!(buckets.len() > 1, "{buckets:?}");
    let taken = claim_with(&node, "spread", &[("limit", int(32))])?;
    assert_eq!(taken.len(), 32);
    assert!(taken.iter().all(|record| matches!(field(record, "partition_key"), Term::Bin(key) if key.starts_with(b"__flow_auto__"))));
    Ok(())
}

#[test]
fn any_partition_and_unnamed_states_take_every_due_flow() -> TestResult {
    // Elixir: "flow_claim_due can scan any partition and selected states"
    // and "flow_claim_due omitted state claims any due state".
    let node = open(&directory("audit-scopes-any")?, SHARDS)?;
    create(&node, "auto-queued", "any", &[])?;
    create(&node, "queued", "any", &[("partition_key", text("pb"))])?;
    create(
        &node,
        "ready",
        "any",
        &[("partition_key", text("pa")), ("state", text("ready"))],
    )?;
    create(
        &node,
        "held",
        "any",
        &[("partition_key", text("pb")), ("state", text("held"))],
    )?;
    let selected = claim_with(
        &node,
        "any",
        &[
            ("partition_key", text("any")),
            ("states", texts(&["queued", "ready"])),
        ],
    )?;
    assert_eq!(
        ids(&selected),
        [text("auto-queued"), text("queued"), text("ready")]
    );
    let rest = claim_with(
        &node,
        "any",
        &[("partition_key", text("any")), ("state", text("any"))],
    )?;
    let [held] = rest.as_slice() else {
        return Err(format!("one held Flow expected, got {rest:?}").into());
    };
    assert_eq!(
        (field(held, "id"), field(held, "partition_key")),
        (text("held"), text("pb"))
    );

    create(&node, "omitted-queued", "omitted", &[])?;
    create(
        &node,
        "omitted-ready",
        "omitted",
        &[("state", text("ready"))],
    )?;
    let both = claim_with(&node, "omitted", &[])?;
    assert_eq!(ids(&both), [text("omitted-queued"), text("omitted-ready")]);
    Ok(())
}

#[test]
fn any_partition_claims_take_higher_priority_first_and_share_shards() -> TestResult {
    // Elixir: "flow_claim_due any partition drains higher priority across
    // shards first", "... rotates start shard per type" and "... spreads
    // small limits across shards under skew".
    let node = open(&directory("audit-scopes-fair")?, SHARDS)?;
    let any = [
        ("partition_key", text("any")),
        ("state", text("any")),
        ("limit", int(1)),
    ];
    let (low, high) = on_two_shards("priority")?;
    create(
        &node,
        "low",
        "priority",
        &[("partition_key", text(&low)), ("priority", int(0))],
    )?;
    create(
        &node,
        "high",
        "priority",
        &[("partition_key", text(&high)), ("priority", int(2))],
    )?;
    let first = claim_with(&node, "priority", &any)?;
    assert_eq!(ids(&first), [text("high")]);

    let (left, right) = on_two_shards("rotate")?;
    for (id, partition) in [("0a", &left), ("0b", &left), ("1a", &right), ("1b", &right)] {
        create(&node, id, "rotate", &[("partition_key", text(partition))])?;
    }
    let partition_of = |records: Vec<Term>| {
        records
            .first()
            .map_or(Term::Nil, |record| field(record, "partition_key"))
    };
    let one = partition_of(claim_with(&node, "rotate", &any)?);
    let two = partition_of(claim_with(&node, "rotate", &any)?);
    assert_ne!(
        one, two,
        "two limit-1 claims took the same shard's partition"
    );

    let (hot, other) = on_two_shards("skew")?;
    for (id, partition) in [("hot-a", &hot), ("hot-b", &hot), ("other", &other)] {
        create(&node, id, "skew", &[("partition_key", text(partition))])?;
    }
    let pair = claim_with(
        &node,
        "skew",
        &[
            ("partition_key", text("any")),
            ("state", text("any")),
            ("limit", int(2)),
        ],
    )?;
    let mut partitions: Vec<Term> = pair
        .iter()
        .map(|record| field(record, "partition_key"))
        .collect();
    partitions.sort_by_key(|partition| format!("{partition:?}"));
    let mut expected = vec![text(&hot), text(&other)];
    expected.sort_by_key(|partition| format!("{partition:?}"));
    assert_eq!(partitions, expected);
    Ok(())
}
