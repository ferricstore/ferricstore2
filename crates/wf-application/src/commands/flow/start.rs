//! `FLOW.START_AND_CLAIM` and `FLOW.RUN_STEPS_MANY` (ADR-014 step 6).
//!
//! Start-and-claim creates a Flow already claimed by its worker in its first
//! logical state; it reads every create option. Run-steps creates each item
//! that way, continues it through the remaining states one millisecond apart
//! on the same lease, and completes it, as Elixir's deterministic step chain
//! does.

use super::FlowError;
use super::create::{self, CreateFlow};
use super::opts::Opts;
use super::term::Term;

const DEFAULT_LEASE_MS: u64 = 30_000;

/// A Flow created claimed.
#[derive(Clone, Debug, PartialEq)]
pub struct StartAndClaim {
    /// The create, with the initial logical state as its state.
    pub create: CreateFlow,
    /// Claiming worker.
    pub worker: Vec<u8>,
    /// Lease length.
    pub lease_ms: u64,
}

fn worker(opts: &Opts<'_>) -> Result<Vec<u8>, FlowError> {
    let worker = opts.required_binary("worker")?;
    if worker.len() > 256 {
        return Err(FlowError::flow("worker", "is longer than 256 bytes"));
    }
    Ok(worker)
}

/// Reads `FLOW.START_AND_CLAIM`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse_start(args: &Term) -> Result<StartAndClaim, FlowError> {
    let Term::Map(entries) = args else {
        return Err(FlowError::rejected(
            "ERR native FLOW.START_AND_CLAIM payload must be a map".to_owned(),
        ));
    };
    let opts = Opts::new(
        "FLOW.START_AND_CLAIM",
        args,
        &[
            "id",
            "type",
            "initial_state",
            "worker",
            "lease_ms",
            "now_ms",
            "partition_key",
            "payload",
            "parent_flow_id",
            "root_flow_id",
            "correlation_id",
            "priority",
            "retention_ttl_ms",
            "max_active_ms",
            "history_hot_max_events",
            "history_max_events",
            "attributes",
            "state_meta",
            "values",
        ],
    )?;
    let initial_state = opts.required_binary("initial_state")?;
    let worker = worker(&opts)?;
    let lease_ms = opts
        .positive_integer("lease_ms")?
        .unwrap_or(DEFAULT_LEASE_MS);
    let mut create_entries: Vec<(Vec<u8>, Term)> = entries
        .iter()
        .filter(|(key, _)| !matches!(key.as_slice(), b"initial_state" | b"worker" | b"lease_ms"))
        .cloned()
        .collect();
    create_entries.push((b"state".to_vec(), Term::Bin(initial_state)));
    let create = create::parse(&Term::Map(create_entries))?;
    Ok(StartAndClaim {
        create,
        worker,
        lease_ms,
    })
}

/// A deterministic step chain run for many new Flows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSteps {
    /// Flow type.
    pub flow_type: Vec<u8>,
    /// Logical states in order; the first is the initial one.
    pub states: Vec<Vec<u8>>,
    /// Worker owning the chain's lease.
    pub worker: Vec<u8>,
    /// Lease length from the start.
    pub lease_ms: u64,
    /// Client clock.
    pub now_ms: Option<u64>,
    /// Payload of every Flow.
    pub payload: Option<Vec<u8>>,
    /// Result of every Flow.
    pub result: Option<Vec<u8>>,
    /// Terminal retention override.
    pub retention_ttl_ms: Option<u64>,
    /// Ids and partitions, in request order.
    pub items: Vec<(Vec<u8>, Option<Vec<u8>>)>,
}

/// A value of any term, stored (audit gap G5).
fn binary(opts: &Opts<'_>, key: &str) -> Result<Option<Vec<u8>>, FlowError> {
    super::create::stored(opts, key)
}

fn steps(opts: &Opts<'_>) -> Result<Vec<Vec<u8>>, FlowError> {
    if let Some(states) = opts.get("states") {
        let Term::List(states) = states else {
            return Err(FlowError::flow("states", "must be a non-empty list"));
        };
        if states.is_empty() {
            return Err(FlowError::flow("states", "must be a non-empty list"));
        }
        return states
            .iter()
            .map(|state| match state {
                Term::Bin(name) if !name.is_empty() && name.as_slice() != b"running" => {
                    Ok(name.clone())
                }
                _ => Err(FlowError::flow("states", "must contain non-empty strings")),
            })
            .collect();
    }
    let count = opts
        .positive_integer("steps")?
        .ok_or_else(|| FlowError::flow("steps", "is required"))?;
    if count > 1_000 {
        return Err(FlowError::flow("steps", "exceeds maximum 1000"));
    }
    Ok((1..=count)
        .map(|step| format!("step_{step}").into_bytes())
        .collect())
}

/// Reads `FLOW.RUN_STEPS_MANY`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option or item.
pub fn parse_run_steps(args: &Term) -> Result<RunSteps, FlowError> {
    let opts = Opts::new(
        "FLOW.RUN_STEPS_MANY",
        args,
        &[
            "type",
            "states",
            "steps",
            "worker",
            "lease_ms",
            "now_ms",
            "payload",
            "result",
            "retention_ttl_ms",
            "items",
        ],
    )?;
    let flow_type = opts.required_binary("type")?;
    let states = steps(&opts)?;
    let worker = worker(&opts)?;
    let items = match opts.get("items") {
        Some(Term::List(items)) if !items.is_empty() => items
            .iter()
            .map(|item| {
                let id = match item.get(b"id") {
                    Some(Term::Bin(id)) if !id.is_empty() => id.clone(),
                    _ => {
                        return Err(FlowError::rejected(
                            "ERR native flow item is invalid".to_owned(),
                        ));
                    }
                };
                let partition = match item.get(b"partition_key") {
                    None | Some(Term::Nil) => None,
                    Some(Term::Bin(key)) if !key.is_empty() => Some(key.clone()),
                    Some(_) => {
                        return Err(FlowError::rejected(
                            "ERR native flow item is invalid".to_owned(),
                        ));
                    }
                };
                Ok((id, partition))
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(FlowError::rejected(
                "ERR flow items must be a non-empty list".to_owned(),
            ));
        }
    };
    Ok(RunSteps {
        flow_type,
        states,
        worker,
        lease_ms: opts
            .positive_integer("lease_ms")?
            .unwrap_or(DEFAULT_LEASE_MS),
        now_ms: opts.non_neg_integer("now_ms")?,
        payload: binary(&opts, "payload")?,
        result: binary(&opts, "result")?,
        retention_ttl_ms: opts.positive_integer("retention_ttl_ms")?,
        items,
    })
}
