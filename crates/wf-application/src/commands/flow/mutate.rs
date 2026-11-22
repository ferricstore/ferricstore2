//! Single-Flow lifecycle mutations (ADR-014 step 3): `FLOW.TRANSITION`,
//! `STEP_CONTINUE`, `RETRY`, `COMPLETE`, `FAIL`, `CANCEL` and
//! `EXTEND_LEASE`, read as `MutationAttrs` reads them.

use std::collections::BTreeMap;

use super::FlowError;
use super::create::{partition_key, values};
use super::meta::{self, Attributes};
use wf_domain::policy::RetryOverride;

use super::opts::Opts;
use super::term::Term;

const COMMON: &[&str] = &[
    "id",
    "partition_key",
    "now_ms",
    "fencing_token",
    "lease_token",
    "attributes",
    "attributes_merge",
    "attributes_delete",
    "state_meta",
    "values",
    "value_refs",
    "drop_values",
    "override_values",
    // Elixir accepts a governed claim's options here; the Flow's credit is
    // released when it leaves `running` either way (ADR-014 step 15).
    "governance_limit_scope",
    "governance_shard_id",
];

const DEFAULT_LEASE_MS: u64 = 30_000;

/// What a mutation does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Move to a nonterminal state, releasing any lease.
    Transition {
        /// Expected state.
        from: Vec<u8>,
        /// New state.
        to: Vec<u8>,
        /// New due time; the command time when absent.
        run_at_ms: Option<u64>,
        /// New claim priority.
        priority: Option<u8>,
        /// Replacement payload.
        payload: Option<Vec<u8>>,
    },
    /// Advance the logical step and keep the lease with a new fence.
    StepContinue {
        /// Expected logical state.
        from: Vec<u8>,
        /// New logical state.
        to: Vec<u8>,
        /// Lease length.
        lease_ms: u64,
        /// Expected owner.
        worker: Option<Vec<u8>>,
        /// Replacement payload.
        payload: Option<Vec<u8>>,
        /// Reply with a compact job instead of the record.
        compact: bool,
    },
    /// Release the lease and schedule another attempt.
    Retry {
        /// Due time; the retry policy decides when absent.
        run_at_ms: Option<u64>,
        /// Replacement payload.
        payload: Option<Vec<u8>>,
        /// Error of this attempt.
        error: Option<Vec<u8>>,
        /// This command's retry fields, over the type's and state's.
        policy: RetryOverride,
    },
    /// Finish successfully.
    Complete {
        /// Replacement payload.
        payload: Option<Vec<u8>>,
        /// Result.
        result: Option<Vec<u8>>,
        /// Retention override.
        ttl_ms: Option<u64>,
    },
    /// Finish with an error.
    Fail {
        /// Replacement payload.
        payload: Option<Vec<u8>>,
        /// Error.
        error: Option<Vec<u8>>,
        /// Retention override.
        ttl_ms: Option<u64>,
    },
    /// Cancel.
    Cancel {
        /// Expected version.
        expected_version: Option<u64>,
        /// Expected state.
        expect_state: Option<Vec<u8>>,
        /// Cancellation reason, stored as the error.
        reason: Option<Vec<u8>>,
        /// Retention override.
        ttl_ms: Option<u64>,
    },
    /// Move the lease deadline.
    ExtendLease {
        /// New lease length from the command time.
        lease_ms: u64,
        /// Reply `OK` instead of the record (`return: ok_on_success`).
        ok_on_success: bool,
        /// The node renewed the Flow's credit lease first (audit G54); a
        /// client never sets it.
        renewed: bool,
    },
}

/// One mutation of one Flow.
#[derive(Clone, Debug, PartialEq)]
pub struct Mutation {
    /// Flow id.
    pub id: Vec<u8>,
    /// Partition, if sent.
    pub partition_key: Option<Vec<u8>>,
    /// Client clock.
    pub now_ms: Option<u64>,
    /// Current fence.
    pub fence: u64,
    /// Current lease token; required for every change but transition and
    /// cancel.
    pub lease_token: Option<Vec<u8>>,
    /// Attributes to set.
    pub attributes_merge: Attributes,
    /// Attribute names to remove.
    pub attributes_delete: Vec<String>,
    /// Metadata entries for the state the change enters.
    pub state_meta: BTreeMap<String, Term>,
    /// Named values to write.
    pub values: BTreeMap<Vec<u8>, Vec<u8>>,
    /// References attached by name (audit gap G9).
    pub value_refs: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Named values to drop.
    pub drop_values: Vec<Vec<u8>>,
    /// Named values whose different bytes may replace the stored ones.
    pub override_values: Vec<Vec<u8>>,
    /// A stored value as the payload (audit gap G16); a given payload wins.
    pub payload_ref: Option<Vec<u8>>,
    /// The change.
    pub change: Change,
}

fn id(opts: &Opts<'_>) -> Result<Vec<u8>, FlowError> {
    match opts.get("id") {
        Some(Term::Bin(id)) if !id.is_empty() => Ok(id.clone()),
        _ => Err(FlowError::rejected(
            "ERR flow id must be a non-empty string".to_owned(),
        )),
    }
}

fn state(opts: &Opts<'_>, key: &str, name: &str) -> Result<Vec<u8>, FlowError> {
    match opts.get(key) {
        Some(Term::Bin(value)) if !value.is_empty() => Ok(value.clone()),
        _ => Err(FlowError::flow(name, "must be a non-empty string")),
    }
}

fn not_running(state: &[u8]) -> Result<(), FlowError> {
    if state == b"running" {
        return Err(FlowError::rejected(
            "ERR flow running state is only entered by FLOW.CLAIM_DUE".to_owned(),
        ));
    }
    if state.len() > 64 {
        return Err(FlowError::flow("state", "is longer than 64 bytes"));
    }
    Ok(())
}

fn lease_token(opts: &Opts<'_>, required: bool) -> Result<Option<Vec<u8>>, FlowError> {
    match opts.get("lease_token") {
        None | Some(Term::Nil) if !required => Ok(None),
        Some(Term::Bin(token)) if !token.is_empty() => Ok(Some(token.clone())),
        _ => Err(FlowError::flow("lease_token", "must be a non-empty string")),
    }
}

fn fence(opts: &Opts<'_>) -> Result<u64, FlowError> {
    opts.non_neg_integer("fencing_token")?
        .ok_or_else(|| FlowError::flow("fencing_token", "is required"))
}

/// A value of any term, stored (audit gap G5).
fn binary(opts: &Opts<'_>, key: &str) -> Result<Option<Vec<u8>>, FlowError> {
    super::create::stored(opts, key)
}

fn names(opts: &Opts<'_>, key: &str) -> Result<Vec<Vec<u8>>, FlowError> {
    opts.binary_list(key)
}

fn deletes(opts: &Opts<'_>) -> Result<Vec<String>, FlowError> {
    let mut keys: Vec<String> = Vec::new();
    for name in names(opts, "attributes_delete")? {
        let key = meta::attribute_key(&name)?;
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    Ok(keys)
}

fn lease_ms(opts: &Opts<'_>) -> Result<u64, FlowError> {
    Ok(opts
        .positive_integer("lease_ms")?
        .unwrap_or(DEFAULT_LEASE_MS))
}

/// Option names one command takes besides [`COMMON`].
fn extra(name: &str) -> Option<&'static [&'static str]> {
    Some(match name {
        "FLOW.TRANSITION" => &[
            "from_state",
            "to_state",
            "run_at_ms",
            "priority",
            "payload",
            "payload_ref",
        ],
        "FLOW.STEP_CONTINUE" => &[
            "from_state",
            "to_state",
            "lease_ms",
            "worker",
            "payload",
            "payload_ref",
            "return",
        ],
        "FLOW.RETRY" => &["run_at_ms", "payload", "payload_ref", "error", "retry"],
        "FLOW.COMPLETE" => &["payload", "result", "ttl_ms", "return"],
        "FLOW.FAIL" => &["payload", "error", "ttl_ms"],
        "FLOW.CANCEL" => &["expected_version", "expect_state", "reason", "ttl_ms"],
        "FLOW.EXTEND_LEASE" => &["lease_ms", "return"],
        _ => return None,
    })
}

fn change(name: &str, opts: &Opts<'_>) -> Result<Change, FlowError> {
    Ok(match name {
        "FLOW.TRANSITION" => {
            let from = state(opts, "from_state", "from")?;
            let to = state(opts, "to_state", "to")?;
            not_running(&to)?;
            Change::Transition {
                from,
                to,
                run_at_ms: opts.non_neg_integer("run_at_ms")?,
                priority: match opts.get("priority") {
                    None | Some(Term::Nil) => None,
                    Some(_) => Some(opts.priority(0)?),
                },
                payload: binary(opts, "payload")?,
            }
        }
        "FLOW.STEP_CONTINUE" => {
            let from = state(opts, "from_state", "from")?;
            let to = state(opts, "to_state", "to")?;
            not_running(&to)?;
            Change::StepContinue {
                from,
                to,
                lease_ms: lease_ms(opts)?,
                worker: opts.binary_or_nil("worker")?,
                payload: binary(opts, "payload")?,
                compact: matches!(opts.get("return"), Some(Term::Bin(shape)) if shape.eq_ignore_ascii_case(b"jobs_compact")),
            }
        }
        "FLOW.RETRY" => Change::Retry {
            run_at_ms: opts.non_neg_integer("run_at_ms")?,
            payload: binary(opts, "payload")?,
            error: binary(opts, "error")?,
            policy: opts
                .get("retry")
                .map(super::policy::retry_override)
                .transpose()?
                .flatten()
                .unwrap_or_default(),
        },
        "FLOW.COMPLETE" => Change::Complete {
            payload: binary(opts, "payload")?,
            result: binary(opts, "result")?,
            ttl_ms: opts.positive_integer("ttl_ms")?,
        },
        "FLOW.FAIL" => Change::Fail {
            payload: binary(opts, "payload")?,
            error: binary(opts, "error")?,
            ttl_ms: opts.positive_integer("ttl_ms")?,
        },
        "FLOW.CANCEL" => Change::Cancel {
            expected_version: opts.non_neg_integer("expected_version")?,
            expect_state: opts.binary_or_nil("expect_state")?,
            reason: binary(opts, "reason")?,
            ttl_ms: opts.positive_integer("ttl_ms")?,
        },
        _ => Change::ExtendLease {
            lease_ms: lease_ms(opts)?,
            ok_on_success: matches!(opts.get("return"), Some(Term::Bin(mode)) if mode.eq_ignore_ascii_case(b"ok_on_success")),
            renewed: false,
        },
    })
}

/// Whether `name` is a single-Flow mutation this module reads.
#[must_use]
pub fn is_mutation(name: &str) -> bool {
    extra(name).is_some()
}

/// Reads one single-Flow mutation.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse(name: &str, args: &Term) -> Result<Mutation, FlowError> {
    let extra = extra(name).ok_or_else(|| {
        FlowError::rejected(format!("ERR {name} is not supported by this server"))
    })?;
    // Elixir names the option to use instead of a reference to a stored
    // value (`reject_public_value_ref_input/3`).
    for (key, instead) in [
        ("result_ref", "result"),
        ("error_ref", "error"),
        ("reason_ref", "reason"),
    ] {
        if args.get(key.as_bytes()).is_some() {
            return Err(FlowError::rejected(format!(
                "ERR flow {key} input is not supported; use {instead}"
            )));
        }
    }
    let allowed: Vec<&str> = COMMON.iter().chain(extra).copied().collect();
    let opts = Opts::new(name, args, &allowed)?;
    let id = id(&opts)?;
    let required_lease = !matches!(name, "FLOW.TRANSITION" | "FLOW.CANCEL");
    let lease_token = lease_token(&opts, required_lease)?;
    let change = change(name, &opts)?;
    let fence = fence(&opts)?;
    let mut attributes_merge = meta::attributes(opts.get("attributes"))?;
    attributes_merge.extend(meta::attributes(opts.get("attributes_merge"))?);
    Ok(Mutation {
        id,
        partition_key: partition_key(&opts)?,
        now_ms: opts.non_neg_integer("now_ms")?,
        fence,
        lease_token,
        attributes_merge,
        attributes_delete: deletes(&opts)?,
        state_meta: meta::state_meta_update(opts.get("state_meta"))?,
        values: values(&opts)?,
        value_refs: super::create::value_refs(&opts)?,
        drop_values: names(&opts, "drop_values")?,
        override_values: names(&opts, "override_values")?,
        payload_ref: super::create::payload_ref(&opts)?.filter(|_| !opts.has("payload")),
        change,
    })
}
