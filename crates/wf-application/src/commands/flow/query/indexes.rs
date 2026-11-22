//! `FLOW.QUERY.INDEXES [index-id]` (ADR-014 step 12), in the
//! `ferric.flow.query.indexes/v1` contract the SDK validates.
//!
//! This node has no index lifecycle: every shard keeps one catalog of its
//! Flows by partition, type and state, ordered by update time, built with
//! the records. The reply describes that catalog as one always-active,
//! fully covered index, so a client sees what queries read.

use super::super::FlowError;
use super::super::term::{Term, map};

/// The catalog's index id.
pub const CATALOG_ID: &str = "flow_catalog_partition_type_state_updated";
const CONTRACT: &str = "ferric.flow.query.indexes/v1";
const STATISTICS_MAX_AGE_MS: u64 = 300_000;
const USAGE: &str = "ERR syntax error; use FLOW.QUERY.INDEXES [index-id]";
const COVERING: [&str; 16] = [
    "attempts",
    "correlation_id",
    "created_at_ms",
    "lease_deadline_ms",
    "max_active_ms",
    "next_run_at_ms",
    "parent_flow_id",
    "partition_key",
    "priority",
    "root_flow_id",
    "run_id",
    "run_state",
    "state",
    "type",
    "updated_at_ms",
    "version",
];

/// A `FLOW.QUERY.INDEXES` request: every index, or the one named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Indexes {
    /// The index asked for.
    pub id: Option<Vec<u8>>,
}

/// Reads the text command's arguments.
///
/// # Errors
/// Elixir's usage text for more than one argument or a non-text one.
pub fn parse(args: &[Term]) -> Result<Indexes, FlowError> {
    match args {
        [] => Ok(Indexes { id: None }),
        [Term::Bin(id)] => Ok(Indexes {
            id: Some(id.clone()),
        }),
        _ => Err(FlowError::rejected(USAGE.to_owned())),
    }
}

/// Elixir's index id: 1 to 64 bytes of letters, digits, `_`, `-`, `:` and
/// `.`.
fn valid_id(id: &[u8]) -> bool {
    (1..=64).contains(&id.len())
        && id
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-:.".contains(byte))
}

fn texts(items: &[&str]) -> Term {
    Term::List(items.iter().map(|item| Term::text(item)).collect())
}

fn field(name: &str, encoding: &str) -> Term {
    map(vec![
        ("direction", Term::text("asc")),
        ("encoding", Term::text(encoding)),
        ("name", Term::text(name)),
    ])
}

fn progress(shards: u64, extra: Vec<(&'static str, Term)>) -> Vec<(&'static str, Term)> {
    let mut entries = vec![
        ("completed_shards", Term::int(shards)),
        ("current_phases", texts(&["done"])),
        ("phase_counts", map(vec![("done", Term::int(shards))])),
        ("scope", Term::text("catalog_build")),
        ("total_shards", Term::int(shards)),
    ];
    entries.extend(extra);
    entries.sort_by_key(|(key, _)| *key);
    entries
}

fn catalog(shards: u64, now_ms: u64) -> Term {
    map(vec![
        (
            "build",
            map(progress(
                shards,
                vec![
                    ("scanned_records", Term::Int(0)),
                    ("written_bytes", Term::Int(0)),
                    ("written_entries", Term::Int(0)),
                ],
            )),
        ),
        ("build_id", Term::text("catalog-1")),
        (
            "count_prefixes",
            Term::List(vec![Term::Int(1), Term::Int(2), Term::Int(3)]),
        ),
        (
            "coverage",
            map(vec![
                ("complete_shards", Term::int(shards)),
                ("total_shards", Term::int(shards)),
                ("validation", Term::text("passed")),
            ]),
        ),
        ("covering_fields", texts(&COVERING)),
        (
            "fields",
            Term::List(vec![
                field("partition_key", "hashed"),
                field("type", "hashed"),
                field("state", "hashed"),
                field("updated_at_ms", "ordered"),
            ]),
        ),
        (
            "format",
            map(vec![
                ("counter", Term::text("wf.catalog.count/v1")),
                ("entry", Term::text("wf.catalog.entry/v1")),
                ("key", Term::text("wf.catalog.key/v1")),
                ("query_row", Term::text("wf.catalog.row/v1")),
                ("reverse", Term::text("wf.catalog.reverse/v1")),
            ]),
        ),
        ("id", Term::text(CATALOG_ID)),
        ("queryable", Term::Bool(true)),
        (
            "retirement",
            map(vec![("status", Term::text("not_applicable"))]),
        ),
        ("source", Term::text("runs")),
        ("state", Term::text("active")),
        (
            "statistics",
            map(vec![
                ("fresh_samples", Term::Int(0)),
                ("future_samples", Term::Int(0)),
                ("newest_age_ms", Term::Nil),
                ("newest_collected_at_ms", Term::Nil),
                ("oldest_age_ms", Term::Nil),
                ("oldest_collected_at_ms", Term::Nil),
                ("samples", Term::Int(0)),
                ("stale_samples", Term::Int(0)),
                ("status", Term::text("missing")),
            ]),
        ),
        (
            "validation",
            map(progress(
                shards,
                vec![
                    ("checked_entries", Term::Int(0)),
                    ("checked_records", Term::Int(0)),
                    ("failure_reason", Term::Nil),
                    ("mismatches", Term::Int(0)),
                    ("status", Term::text("passed")),
                    ("validated_at_ms", Term::int(now_ms)),
                ],
            )),
        ),
        ("version", Term::Int(1)),
        ("workloads", Term::List(Vec::new())),
    ])
}

/// The reply for a node of `shards` shards at `now_ms`.
///
/// # Errors
/// Elixir's text for an index id this node does not have.
pub fn reply(request: &Indexes, shards: usize, now_ms: u64) -> Result<Term, FlowError> {
    // An id is checked before its lookup, as Elixir's
    // `IndexDefinition.valid_id?` does (audit gap G41).
    if request.id.as_ref().is_some_and(|id| !valid_id(id)) {
        return Err(FlowError::rejected(format!(
            "ERR invalid query index id; use {}",
            USAGE.trim_start_matches("ERR syntax error; use ")
        )));
    }
    if request
        .id
        .as_ref()
        .is_some_and(|id| id.as_slice() != CATALOG_ID.as_bytes())
    {
        return Err(FlowError::rejected(
            "ERR query index not found; use FLOW.QUERY.INDEXES to list valid indexes".to_owned(),
        ));
    }
    let shards = u64::try_from(shards).unwrap_or(u64::MAX);
    Ok(map(vec![
        ("contract_version", Term::text(CONTRACT)),
        ("indexes", Term::List(vec![catalog(shards, now_ms)])),
        ("observed_at_ms", Term::int(now_ms)),
        (
            "registry",
            map(vec![
                ("catalog_version", Term::Int(1)),
                ("epoch", Term::Int(1)),
            ]),
        ),
        (
            "services",
            map(vec![
                ("lifecycle_worker", Term::text("ready")),
                ("registry", Term::text("ready")),
                ("statistics_store", Term::text("ready")),
                ("statistics_worker", Term::text("ready")),
            ]),
        ),
        ("statistics_max_age_ms", Term::int(STATISTICS_MAX_AGE_MS)),
    ]))
}
