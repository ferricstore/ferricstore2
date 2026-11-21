//! Typed-map `FLOW.CLAIM_DUE` and `FLOW.RECLAIM`, read as
//! `Ferricstore.Flow.ClaimDueApi` reads them, and Elixir's reply shapes.
//!
//! `FLOW.RECLAIM` is a claim for state `running` without the reclaim
//! option: it takes only Flows whose lease expired.

use super::FlowError;
use super::create::partition_key;
use super::opts::Opts;
use super::record::{DEFAULT_PAYLOAD_MAX_BYTES, Include};
use super::term::{Term, map};
use crate::commands::queue::{ClaimFlows, ClaimGovernance, ClaimedFlow};

mod filters;

use filters::states;
pub(super) use filters::values;

const ALLOWED: &[&str] = &[
    "type",
    "state",
    "states",
    "worker",
    "lease_ms",
    "limit",
    "priority",
    "now_ms",
    "block_ms",
    "governance_limit_scope",
    "governance_shard_id",
    "return",
    "full",
    "payload",
    "payload_max_bytes",
    "values",
    "value_max_bytes",
    "reclaim_expired",
    "reclaim_ratio",
    "partition_key",
    "partition_keys",
];

const DEFAULT_LEASE_MS: u64 = 30_000;
const MAX_WORKER_BYTES: usize = 256;
/// Elixir's default `flow_max_claim_limit`.
const MAX_CLAIM_LIMIT: u64 = 10_000;
/// Longest blocking wait, as in Elixir.
const MAX_BLOCK_MS: u64 = 0xFFFF_FFFF;

/// What each claimed Flow looks like in the reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimReturn {
    /// The record map.
    Records,
    /// A map of id, type, state, run state, partition, lease and fence.
    Jobs,
    /// `[id, partition, lease token, fence]`.
    JobsCompact,
    /// The compact job and the attributes map.
    JobsCompactAttrs,
    /// The compact job and the run state.
    JobsCompactState,
    /// The compact job, the run state and the attributes map.
    JobsCompactStateAttrs,
}

/// A claim request and its reply shape.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedClaim {
    /// What to claim.
    pub request: ClaimFlows,
    /// How to reply.
    pub shape: ClaimReturn,
}

fn shape(value: Option<&Term>) -> Result<ClaimReturn, FlowError> {
    let Some(value) = value else {
        return Ok(ClaimReturn::Records);
    };
    let Term::Bin(text) = value else {
        return Err(FlowError::rejected(
            "ERR flow claim return must be records, jobs, jobs_compact, jobs_compact_attrs, \
             jobs_compact_state, or jobs_compact_state_attrs"
                .to_owned(),
        ));
    };
    let upper = String::from_utf8_lossy(text).to_uppercase();
    let base = upper.replacen("JOB_", "JOBS_", 1).replace("_WITH_", "_");
    let base = base.replace("_ATTRIBUTES", "_ATTRS");
    Ok(match base.as_str() {
        "RECORDS" | "RECORD" | "FULL" => ClaimReturn::Records,
        "JOBS" | "JOB" => ClaimReturn::Jobs,
        "JOBS_COMPACT" => ClaimReturn::JobsCompact,
        "JOBS_COMPACT_ATTRS" => ClaimReturn::JobsCompactAttrs,
        "JOBS_COMPACT_STATE" => ClaimReturn::JobsCompactState,
        "JOBS_COMPACT_STATE_ATTRS" => ClaimReturn::JobsCompactStateAttrs,
        _ => {
            return Err(FlowError::rejected(
                "ERR flow claim return must be records, jobs, or compact jobs".to_owned(),
            ));
        }
    })
}

/// Listed partitions, and whether every partition is allowed.
pub type Scope = (Option<Vec<Vec<u8>>>, bool);

/// The scope of a claim's single partition key, as Elixir reads it: `ANY`
/// is every partition, `AUTO` or none the auto partitions, `GLOBAL` the
/// unpartitioned Flows this node never has; any other name is that
/// partition. Returns the listed partitions and whether any is allowed.
#[must_use]
pub fn scope(partition_key: Option<&[u8]>) -> Scope {
    let Some(key) = partition_key else {
        return (None, false);
    };
    match String::from_utf8_lossy(key).to_uppercase().as_str() {
        "ANY" => (None, true),
        "AUTO" => (None, false),
        "GLOBAL" => (Some(Vec::new()), false),
        _ => (Some(vec![key.to_vec()]), false),
    }
}

fn partitions(opts: &Opts<'_>) -> Result<Scope, FlowError> {
    let Some(keys) = opts.get("partition_keys") else {
        return Ok(scope(partition_key(opts)?.as_deref()));
    };
    if opts.has("partition_key") {
        return Err(FlowError::rejected(
            "ERR flow partition_key and partition_keys are mutually exclusive".to_owned(),
        ));
    }
    match keys {
        Term::List(items) if !items.is_empty() => items
            .iter()
            .map(|item| match item {
                Term::Bin(key) if !key.is_empty() => Ok(key.clone()),
                _ => Err(FlowError::flow(
                    "partition_keys",
                    "must contain non-empty strings",
                )),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|mut keys| {
                keys.dedup();
                (Some(keys), false)
            }),
        _ => Err(FlowError::flow(
            "partition_keys",
            "must be a non-empty list",
        )),
    }
}

fn include(opts: &Opts<'_>, records: bool) -> Result<Option<Include>, FlowError> {
    let full = opts.boolean("full", records)?;
    let payload = opts.boolean("payload", full)?;
    let max_bytes = opts
        .non_neg_integer("payload_max_bytes")?
        .unwrap_or(DEFAULT_PAYLOAD_MAX_BYTES);
    if max_bytes > DEFAULT_PAYLOAD_MAX_BYTES {
        return Err(FlowError::flow(
            "payload_max_bytes",
            &format!("exceeds maximum {DEFAULT_PAYLOAD_MAX_BYTES}"),
        ));
    }
    let values = values(opts)?;
    Ok(Some(Include {
        payload,
        max_bytes,
        values,
    }))
}

/// Reads `FLOW.CLAIM_DUE` or `FLOW.RECLAIM`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse(name: &str, args: &Term) -> Result<ParsedClaim, FlowError> {
    let opts = Opts::new(name, args, ALLOWED)?;
    let reclaim_only = name == "FLOW.RECLAIM";
    let flow_type = match opts.get("type") {
        Some(Term::Bin(value)) if !value.is_empty() => value.clone(),
        _ => return Err(FlowError::flow("type", "must be a non-empty string")),
    };
    if flow_type.as_slice() == super::schedule::SCHEDULE_TYPE.as_bytes() {
        return Err(FlowError::rejected(
            "ERR flow type is reserved for internal use".to_owned(),
        ));
    }
    let states = if reclaim_only {
        vec![b"running".to_vec()]
    } else {
        states(&opts)?
    };
    let worker = opts.required_binary("worker")?;
    if worker.len() > MAX_WORKER_BYTES {
        return Err(FlowError::flow(
            "worker",
            &format!("is longer than {MAX_WORKER_BYTES} bytes"),
        ));
    }
    let lease_ms = opts
        .positive_integer("lease_ms")?
        .unwrap_or(DEFAULT_LEASE_MS);
    let limit = opts.positive_integer("limit")?.unwrap_or(1);
    if limit > MAX_CLAIM_LIMIT {
        return Err(FlowError::flow(
            "limit",
            &format!("exceeds maximum {MAX_CLAIM_LIMIT}"),
        ));
    }
    let priority = match opts.get("priority") {
        None | Some(Term::Nil) => None,
        Some(_) => Some(opts.priority(0)?),
    };
    let now_ms = opts.non_neg_integer("now_ms")?;
    if let Some(now) = now_ms
        && now > super::MAX_EXACT.saturating_sub(lease_ms)
    {
        return Err(FlowError::flow(
            "lease_ms",
            "deadline exceeds the exact integer limit",
        ));
    }
    let shape = shape(opts.get("return"))?;
    let records = include(&opts, shape == ClaimReturn::Records)?;
    let reclaim_expired = opts.boolean("reclaim_expired", true)? && !reclaim_only;
    let ratio = match opts.get("reclaim_ratio") {
        None => 25,
        Some(Term::Int(value)) if (0..=100).contains(value) => u8::try_from(*value).unwrap_or(25),
        Some(_) => {
            return Err(FlowError::rejected(
                "ERR flow reclaim_ratio must be an integer between 0 and 100".to_owned(),
            ));
        }
    };
    let block_ms = match opts.non_neg_integer("block_ms")? {
        Some(value) if value > MAX_BLOCK_MS => {
            return Err(FlowError::flow(
                "block_ms",
                &format!("exceeds maximum {MAX_BLOCK_MS}"),
            ));
        }
        value => value,
    };
    let (partitions, any_partition) = partitions(&opts)?;
    let request = ClaimFlows {
        flow_type,
        states,
        worker,
        lease_ms,
        limit: usize::try_from(limit).unwrap_or(usize::MAX),
        priority,
        partitions,
        any_partition,
        reclaim: reclaim_expired.then_some(ratio),
        block_ms,
        cancellation: None,
        now_ms,
        records,
        governance: governance(&opts),
    };
    Ok(ParsedClaim { request, shape })
}

/// The governed-claim options as given; Elixir checks them when claiming.
fn governance(opts: &Opts<'_>) -> ClaimGovernance {
    let scope = opts.get("governance_limit_scope");
    let shard = opts.get("governance_shard_id");
    ClaimGovernance {
        given: [scope, shard]
            .iter()
            .any(|value| value.is_some_and(|value| *value != Term::Nil)),
        scope: match scope {
            Some(Term::Bin(scope)) if !scope.is_empty() => Some(scope.clone()),
            _ => None,
        },
        shard: match shard {
            Some(Term::Int(shard)) => Some(*shard),
            _ => None,
        },
    }
}

fn job(record: &Term, shape: ClaimReturn) -> Term {
    let get = |key: &str| record.get(key.as_bytes()).cloned().unwrap_or(Term::Nil);
    let run_state = || match get("run_state") {
        Term::Nil => get("state"),
        state => state,
    };
    let attributes = || match get("attributes") {
        Term::Nil => Term::Map(Vec::new()),
        attributes => attributes,
    };
    let compact = || {
        vec![
            get("id"),
            get("partition_key"),
            get("lease_token"),
            get("fencing_token"),
        ]
    };
    match shape {
        ClaimReturn::Records => record.clone(),
        ClaimReturn::Jobs => map(vec![
            ("id", get("id")),
            ("type", get("type")),
            ("state", get("state")),
            ("run_state", get("run_state")),
            ("partition_key", get("partition_key")),
            ("lease_token", get("lease_token")),
            ("fencing_token", get("fencing_token")),
        ]),
        ClaimReturn::JobsCompact => Term::List(compact()),
        ClaimReturn::JobsCompactAttrs => {
            let mut items = compact();
            items.push(attributes());
            Term::List(items)
        }
        ClaimReturn::JobsCompactState => {
            let mut items = compact();
            items.push(run_state());
            Term::List(items)
        }
        ClaimReturn::JobsCompactStateAttrs => {
            let mut items = compact();
            items.push(run_state());
            items.push(attributes());
            Term::List(items)
        }
    }
}

/// The reply to a typed claim: one entry per claimed Flow in `shape`.
#[must_use]
pub fn reply(shape: ClaimReturn, claimed: Vec<ClaimedFlow>) -> Term {
    Term::List(
        claimed
            .into_iter()
            .filter_map(|flow| flow.record)
            .map(|record| job(&record, shape))
            .collect(),
    )
}
