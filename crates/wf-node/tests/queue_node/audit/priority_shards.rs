//! The node's own checks beside Elixir's priority tests (audit gap G30):
//! a claim without a priority filter takes its own type's highest priority
//! across shards, all a shard holds at it first, then the next priority
//! wherever it is.

use wf_application::commands::flow::Term;
use wf_application::commands::queue::{Partition, shard_for_slot};

use super::field;
use super::int;
use super::priority_retries::{claim, create, one};
use super::text;
use crate::{TestResult, directory, open};

/// Two partition names on different shards of a 4-shard node.
fn on_two_shards(prefix: &str) -> TestResult<(String, String)> {
    let shard = |name: &str| shard_for_slot(Partition::parse(name.as_bytes()).slot(), 4);
    let first = format!("{prefix}-0");
    let other = (1..512)
        .map(|index| format!("{prefix}-{index}"))
        .find(|name| shard(name) != shard(&first))
        .ok_or("no partition on another shard")?;
    Ok((first, other))
}

#[test]
fn a_claim_takes_its_own_types_highest_priority_across_shards() -> TestResult {
    // The node's own check beside Elixir's priority tests (audit gap G30):
    // another type's higher priority on a shard does not steer a claim to
    // that shard's lower-priority Flow of the claim's type.
    let node = open(&directory("audit-priority-types")?, 4)?;
    let (shared, apart) = on_two_shards("types")?;
    let at = |partition: &str, priority: i64| {
        vec![
            ("partition_key", text(partition)),
            ("priority", int(priority)),
        ]
    };
    create(&node, "b-high", "type-b", &at(&shared, 2))?;
    create(&node, "a-low", "type-a", &at(&shared, 0))?;
    create(&node, "a-mid", "type-a", &at(&apart, 1))?;
    let any = [("partition_key", text("any")), ("state", text("any"))];
    let first = one(claim(&node, "type-a", 1_000, &any)?)?;
    assert_eq!(field(&first, "id"), text("a-mid"));
    let second = one(claim(&node, "type-a", 1_000, &any)?)?;
    assert_eq!(field(&second, "id"), text("a-low"));
    let both = claim(
        &node,
        "type-b",
        1_000,
        &[&any[..], &[("limit", int(5))][..]].concat(),
    )?;
    assert_eq!(
        both.iter()
            .map(|record| field(record, "id"))
            .collect::<Vec<_>>(),
        [text("b-high")]
    );
    Ok(())
}

/// `count` partition names, each on its own shard of a 4-shard node.
fn on_shards(prefix: &str, count: usize) -> TestResult<Vec<String>> {
    let shard = |name: &str| shard_for_slot(Partition::parse(name.as_bytes()).slot(), 4);
    let mut names: Vec<String> = Vec::new();
    for index in 0..512 {
        let name = format!("{prefix}-{index}");
        if !names.iter().any(|taken| shard(taken) == shard(&name)) {
            names.push(name);
        }
    }
    names.truncate(count);
    if names.len() == count {
        Ok(names)
    } else {
        Err("too few shards".into())
    }
}

fn sorted_ids(records: &[Term]) -> Vec<Term> {
    let mut ids: Vec<Term> = records.iter().map(|record| field(record, "id")).collect();
    ids.sort_by_key(|id| match id {
        Term::Bin(bytes) => bytes.clone(),
        _ => Vec::new(),
    });
    ids
}

#[test]
fn claims_take_the_highest_priority_from_shards_holding_several() -> TestResult {
    // The node's own check beside Elixir's priority tests (audit gap G30):
    // a shard holding the top priority gives all it holds at it, then the
    // next priority comes from wherever it is.
    let node = open(&directory("audit-priority-several")?, 4)?;
    let shards = on_shards("several", 4)?;
    let [home, rest @ ..] = shards.as_slice() else {
        return Err("four partitions".into());
    };
    let at = |partition: &str, priority: i64| {
        vec![
            ("partition_key", text(partition)),
            ("priority", int(priority)),
        ]
    };
    for index in 1..=5 {
        create(&node, &format!("top-{index}"), "several", &at(home, 2))?;
    }
    create(&node, "low", "several", &at(home, 0))?;
    for (index, partition) in rest.iter().enumerate() {
        create(&node, &format!("mid-{index}"), "several", &at(partition, 1))?;
    }
    let any = |limit: i64| {
        vec![
            ("partition_key", text("any")),
            ("state", text("any")),
            ("limit", int(limit)),
        ]
    };
    let tops: Vec<Term> = (1..=5).map(|index| text(&format!("top-{index}"))).collect();
    assert_eq!(sorted_ids(&claim(&node, "several", 1_000, &any(5))?), tops);
    let mids = sorted_ids(&claim(&node, "several", 1_000, &any(2))?);
    assert_eq!(mids.len(), 2);
    assert!(!mids.contains(&text("low")), "{mids:?}");

    let (pair, other) = (&shards[0], &shards[1]);
    create(&node, "pair-top", "pair", &at(pair, 2))?;
    create(&node, "pair-low", "pair", &at(pair, 0))?;
    create(&node, "pair-mid", "pair", &at(other, 1))?;
    // Higher priority first: the order of the reply is the order claimed.
    let pair_ids: Vec<Term> = claim(&node, "pair", 1_000, &any(2))?
        .iter()
        .map(|record| field(record, "id"))
        .collect();
    assert_eq!(pair_ids, [text("pair-top"), text("pair-mid")]);
    Ok(())
}
