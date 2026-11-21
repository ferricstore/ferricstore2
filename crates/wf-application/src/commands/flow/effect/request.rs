//! Reading effect and ledger requests in the order Elixir checks them.
//!
//! Elixir checks the lease and the remaining reservation options only after
//! loading the Flow, so a missing Flow is reported before them; those
//! checks are kept here with their outcome and reported by the shard in
//! that order.

use super::super::opts::{MAX_EXACT, Opts};
use super::super::term::Term;
use super::super::{FlowError, keys};
use super::{MAX_FIELD_BYTES, MAX_KEY_BYTES, Status};
use crate::routing::Partition;

mod ledger;

pub use ledger::{LedgerList, ledger};

/// The lease a request presents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    /// The lease token.
    pub token: Vec<u8>,
    /// The fencing token.
    pub fence: i64,
}

/// The options of a reservation Elixir checks after the lease.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReserveFields {
    /// What the effect does, for replay and conflict checks.
    pub operation_digest: Vec<u8>,
    /// The key the policy may require.
    pub idempotency_key: Option<Vec<u8>>,
    /// The client's clock.
    pub now_ms: Option<u64>,
}

/// `FLOW.EFFECT.RESERVE`.
#[derive(Clone, Debug, PartialEq)]
pub struct Reserve {
    /// The Flow.
    pub id: Vec<u8>,
    /// Its explicit partition.
    pub partition_key: Option<Vec<u8>>,
    /// The effect's key within the Flow.
    pub effect_key: Vec<u8>,
    /// What kind of effect it is.
    pub effect_type: Vec<u8>,
    /// The lease, or why it is invalid.
    pub lease: Result<Lease, String>,
    /// The other options, or the first invalid one.
    pub fields: Result<ReserveFields, String>,
    /// The scope a refusal names, when given.
    pub governance_scope: Option<Vec<u8>>,
}

/// `FLOW.EFFECT.CONFIRM`, `FAIL` or `COMPENSATE`.
#[derive(Clone, Debug, PartialEq)]
pub struct Update {
    /// The Flow.
    pub id: Vec<u8>,
    /// Its explicit partition.
    pub partition_key: Option<Vec<u8>>,
    /// The effect's key within the Flow.
    pub effect_key: Vec<u8>,
    /// The terminal status asked for.
    pub status: Status,
    /// The lease, or why it is invalid.
    pub lease: Result<Lease, String>,
    /// The client's clock, or why it is invalid.
    pub now_ms: Result<Option<u64>, String>,
    /// The provider's id for the effect.
    pub external_id: Option<Vec<u8>>,
    /// What went wrong.
    pub error: Option<Vec<u8>>,
    /// Why.
    pub reason: Option<Vec<u8>>,
    /// How long the effect took.
    pub latency_ms: Option<u64>,
}

/// `FLOW.EFFECT.GET`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Get {
    /// The Flow.
    pub id: Vec<u8>,
    /// Its explicit partition.
    pub partition_key: Option<Vec<u8>>,
    /// The effect's key within the Flow.
    pub effect_key: Vec<u8>,
}

pub(super) fn rejected(text: String) -> FlowError {
    FlowError::rejected(text)
}

/// The native layer's `require_binary/2`.
pub(super) fn native(args: &Term, key: &str) -> Result<Vec<u8>, FlowError> {
    match args.get(key.as_bytes()) {
        Some(Term::Bin(value)) => Ok(value.clone()),
        _ => Err(FlowError::BadRequest(format!(
            "ERR native field {key} must be binary"
        ))),
    }
}

/// `validate_bounded_binary/3`.
fn bounded(field: &str, value: &[u8]) -> Result<(), FlowError> {
    if value.is_empty() || value.len() > MAX_KEY_BYTES {
        return Err(rejected(format!(
            "ERR flow {field} must be a non-empty string of at most {MAX_KEY_BYTES} bytes"
        )));
    }
    Ok(())
}

pub(super) fn partition_key(opts: &Opts<'_>) -> Result<Option<Vec<u8>>, FlowError> {
    match opts.get("partition_key") {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Bin(value)) if value.len() <= MAX_KEY_BYTES => Ok(Some(value.clone())),
        Some(_) => Err(rejected(
            "ERR flow partition_key must be a string".to_owned(),
        )),
    }
}

fn lease(opts: &Opts<'_>) -> Result<Lease, String> {
    let token = match opts.get("lease_token") {
        Some(Term::Bin(token)) if !token.is_empty() => token.clone(),
        _ => return Err("ERR flow lease_token must be a non-empty string".to_owned()),
    };
    match opts.get("fencing_token") {
        Some(Term::Int(fence)) => Ok(Lease {
            token,
            fence: *fence,
        }),
        _ => Err("ERR flow fencing_token must be an integer".to_owned()),
    }
}

fn now_ms(opts: &Opts<'_>) -> Result<Option<u64>, String> {
    match opts.get("now_ms") {
        None => Ok(None),
        Some(Term::Int(now)) => u64::try_from(*now)
            .ok()
            .filter(|now| *now <= MAX_EXACT)
            .map(Some)
            .ok_or_else(|| "ERR flow now_ms must be a non-negative integer".to_owned()),
        Some(_) => Err("ERR flow now_ms must be a non-negative integer".to_owned()),
    }
}

/// The storage key Elixir writes an effect under, for its size check and
/// conflict decision ids.
#[must_use]
pub fn storage_key(id: &[u8], partition_key: Option<&[u8]>, effect_key: &[u8]) -> Vec<u8> {
    let partition = Partition::for_new_flow(id, partition_key);
    let mut key = keys::governance_prefix(&partition, "e");
    key.extend_from_slice(crate::routing::base64url_nopad(id).as_bytes());
    key.push(b':');
    key.extend_from_slice(crate::routing::base64url_nopad(effect_key).as_bytes());
    key
}

pub(super) fn key_size(key: &[u8]) -> Result<(), String> {
    if key.len() > MAX_KEY_BYTES {
        return Err(format!("ERR key too large (max {MAX_KEY_BYTES} bytes)"));
    }
    Ok(())
}

fn reserve_fields(opts: &Opts<'_>, key: &[u8]) -> Result<ReserveFields, String> {
    let operation_digest = match opts.get("operation_digest") {
        Some(Term::Bin(digest)) if !digest.is_empty() && digest.len() <= MAX_FIELD_BYTES => {
            digest.clone()
        }
        _ => {
            return Err(format!(
                "ERR flow operation_digest must be a non-empty string of at most {MAX_FIELD_BYTES} bytes"
            ));
        }
    };
    let idempotency_key = match opts.get("idempotency_key") {
        None | Some(Term::Nil) => None,
        Some(Term::Bin(key)) if key.len() <= MAX_FIELD_BYTES => Some(key.clone()),
        Some(_) => {
            return Err(format!(
                "ERR flow idempotency_key must be a string of at most {MAX_FIELD_BYTES} bytes"
            ));
        }
    };
    let now_ms = now_ms(opts)?;
    key_size(key)?;
    Ok(ReserveFields {
        operation_digest,
        idempotency_key,
        now_ms,
    })
}

/// Reads `FLOW.EFFECT.RESERVE`.
///
/// # Errors
/// Elixir's text for an option it checks before loading the Flow.
pub fn reserve(args: &Term) -> Result<Reserve, FlowError> {
    let id = native(args, "id")?;
    let effect_key = native(args, "effect_key")?;
    let effect_type = native(args, "effect_type")?;
    let opts = Opts::new(
        "FLOW.EFFECT.RESERVE",
        args,
        &[
            "id",
            "effect_key",
            "effect_type",
            "partition_key",
            "lease_token",
            "fencing_token",
            "operation_digest",
            "idempotency_key",
            "governance_scope",
            "now_ms",
            "deadline_ms",
        ],
    )?;
    bounded("id", &id)?;
    bounded("effect_key", &effect_key)?;
    bounded("effect_type", &effect_type)?;
    let partition_key = partition_key(&opts)?;
    let key = storage_key(&id, partition_key.as_deref(), &effect_key);
    Ok(Reserve {
        lease: lease(&opts),
        fields: reserve_fields(&opts, &key),
        governance_scope: optional_text(&opts, "governance_scope")?,
        id,
        partition_key,
        effect_key,
        effect_type,
    })
}

fn optional_text(opts: &Opts<'_>, key: &str) -> Result<Option<Vec<u8>>, FlowError> {
    match opts.get(key) {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Bin(value)) if value.len() <= MAX_FIELD_BYTES => Ok(Some(value.clone())),
        Some(_) => Err(rejected(format!("ERR flow {key} must be a string"))),
    }
}

/// Reads a terminal move: `FLOW.EFFECT.CONFIRM`, `FAIL` or `COMPENSATE`.
///
/// # Errors
/// Elixir's text for an option it checks before loading the Flow.
pub fn update(name: &str, args: &Term, status: Status) -> Result<Update, FlowError> {
    let id = native(args, "id")?;
    let effect_key = native(args, "effect_key")?;
    let opts = Opts::new(
        name,
        args,
        &[
            "id",
            "effect_key",
            "partition_key",
            "lease_token",
            "fencing_token",
            "external_id",
            "error",
            "reason",
            "latency_ms",
            "now_ms",
            "deadline_ms",
        ],
    )?;
    bounded("id", &id)?;
    bounded("effect_key", &effect_key)?;
    let external_id = optional_text(&opts, "external_id")?;
    let error = optional_text(&opts, "error")?;
    let reason = optional_text(&opts, "reason")?;
    let latency_ms = match opts.get("latency_ms") {
        None | Some(Term::Nil) => None,
        Some(Term::Int(latency)) => Some(
            u64::try_from(*latency)
                .ok()
                .filter(|latency| *latency <= MAX_EXACT)
                .ok_or_else(|| {
                    rejected("ERR flow latency_ms must be a non-negative integer".to_owned())
                })?,
        ),
        Some(_) => {
            return Err(rejected(
                "ERR flow latency_ms must be a non-negative integer".to_owned(),
            ));
        }
    };
    let partition_key = partition_key(&opts)?;
    let key = storage_key(&id, partition_key.as_deref(), &effect_key);
    Ok(Update {
        lease: lease(&opts),
        now_ms: now_ms(&opts).and_then(|now| key_size(&key).map(|()| now)),
        id,
        partition_key,
        effect_key,
        status,
        external_id,
        error,
        reason,
        latency_ms,
    })
}

/// Reads `FLOW.EFFECT.GET`.
///
/// # Errors
/// Elixir's text for an invalid option.
pub fn get(args: &Term) -> Result<Get, FlowError> {
    let id = native(args, "id")?;
    let effect_key = native(args, "effect_key")?;
    let opts = Opts::new(
        "FLOW.EFFECT.GET",
        args,
        &["id", "effect_key", "partition_key", "deadline_ms"],
    )?;
    bounded("id", &id)?;
    bounded("effect_key", &effect_key)?;
    let partition_key = partition_key(&opts)?;
    key_size(&storage_key(&id, partition_key.as_deref(), &effect_key)).map_err(rejected)?;
    Ok(Get {
        id,
        partition_key,
        effect_key,
    })
}
