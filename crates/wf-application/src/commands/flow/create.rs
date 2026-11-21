//! `FLOW.CREATE` options, read as `MutationAttrs.create_attrs/3` reads them.

use std::collections::BTreeMap;

use super::meta::{self, Attributes};
use super::opts::Opts;
use super::term::Term;
use super::value_codec;
use super::{FlowError, MAX_FLOW_ID_BYTES};

const ALLOWED: &[&str] = &[
    "id",
    "type",
    "state",
    "partition_key",
    "payload",
    "payload_ref",
    "parent_flow_id",
    "root_flow_id",
    "correlation_id",
    "run_at_ms",
    "now_ms",
    "priority",
    "idempotent",
    "retention_ttl_ms",
    "ttl_ms",
    "max_active_ms",
    "history_hot_max_events",
    "history_max_events",
    "attributes",
    "state_meta",
    "values",
    "value_refs",
];

const MAX_TYPE_BYTES: usize = 128;
const MAX_STATE_BYTES: usize = 64;
const MAX_CORRELATION_ID_BYTES: usize = 256;
const MAX_REF_BYTES: usize = 256;
const MAX_PARTITION_KEY_BYTES: usize = 256;
const MAX_HISTORY_HOT_MAX_EVENTS: u64 = 10_000;
const MAX_HISTORY_MAX_EVENTS: u64 = 1_000_000;
/// Longest accepted `max_active_ms`: 365 days.
pub const MAX_ACTIVE_MS: u64 = 31_536_000_000;

/// How long a Flow may stay active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaxActive {
    /// At most this many milliseconds.
    Ms(u64),
    /// No limit.
    Infinity,
}

/// One Flow to create, with everything the shard needs to decide it.
#[derive(Clone, Debug, PartialEq)]
pub struct CreateFlow {
    /// Flow id.
    pub id: Vec<u8>,
    /// Flow type.
    pub flow_type: Vec<u8>,
    /// Initial state.
    pub state: Vec<u8>,
    /// Explicit partition, if sent.
    pub partition_key: Option<Vec<u8>>,
    /// Payload, if any.
    pub payload: Option<Vec<u8>>,
    /// Parent Flow id.
    pub parent_flow_id: Option<Vec<u8>>,
    /// Root Flow id; the Flow itself when absent.
    pub root_flow_id: Option<Vec<u8>>,
    /// Correlation id.
    pub correlation_id: Option<Vec<u8>>,
    /// A retry of an earlier create answers the existing Flow.
    pub idempotent: bool,
    /// Client clock, used as the command time when sent.
    pub now_ms: Option<u64>,
    /// First eligible time; the command time when absent.
    pub run_at_ms: Option<u64>,
    /// Claim priority, 0 through 2.
    pub priority: u8,
    /// Terminal retention override.
    pub retention_ttl_ms: Option<u64>,
    /// Active-time limit.
    pub max_active_ms: Option<MaxActive>,
    /// Hot history cap override.
    pub history_hot_max_events: Option<u64>,
    /// History cap override.
    pub history_max_events: Option<u64>,
    /// Attributes.
    pub attributes: Attributes,
    /// Metadata for the initial state.
    pub state_meta: BTreeMap<String, Term>,
    /// Named values, by name.
    pub values: BTreeMap<Vec<u8>, Vec<u8>>,
    /// References attached by name (audit gap G9).
    pub value_refs: BTreeMap<Vec<u8>, Vec<u8>>,
    /// A stored value as the payload, when no payload is given (G16).
    pub payload_ref: Option<Vec<u8>>,
}

fn too_long(key: &str, max: usize) -> FlowError {
    FlowError::flow(key, &format!("is longer than {max} bytes"))
}

fn bounded(key: &str, value: Option<&[u8]>, max: usize) -> Result<(), FlowError> {
    match value {
        Some(value) if value.len() > max => Err(too_long(key, max)),
        _ => Ok(()),
    }
}

fn ref_size(key: &str, value: Option<&[u8]>) -> Result<(), FlowError> {
    match value {
        Some(value) if value.len() > MAX_REF_BYTES => Err(FlowError::rejected(format!(
            "ERR flow {key} too large (max {MAX_REF_BYTES} bytes)"
        ))),
        _ => Ok(()),
    }
}

fn retention_ttl_ms(opts: &Opts<'_>) -> Result<Option<u64>, FlowError> {
    if opts.has("ttl_ms") {
        return Err(FlowError::rejected(
            "ERR flow ttl_ms was renamed to retention_ttl_ms".to_owned(),
        ));
    }
    match opts.get("retention_ttl_ms") {
        None => Ok(None),
        Some(Term::Int(value)) if *value > 0 => Ok(u64::try_from(*value).ok()),
        Some(_) => Err(FlowError::flow(
            "retention_ttl_ms",
            "must be a positive integer",
        )),
    }
}

/// `optional_max_active_ms/1`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid value.
pub fn max_active_ms(opts: &Opts<'_>) -> Result<Option<MaxActive>, FlowError> {
    match opts.get("max_active_ms") {
        None => Ok(None),
        Some(Term::Int(value)) => match u64::try_from(*value) {
            Ok(value) if (1..=MAX_ACTIVE_MS).contains(&value) => Ok(Some(MaxActive::Ms(value))),
            _ => Err(max_active_error()),
        },
        Some(Term::Nil) => Ok(Some(MaxActive::Infinity)),
        Some(Term::Bin(text)) if text == b"infinity" || text == b"INFINITY" => {
            Ok(Some(MaxActive::Infinity))
        }
        Some(_) => Err(max_active_error()),
    }
}

fn max_active_error() -> FlowError {
    FlowError::flow(
        "max_active_ms",
        &format!("must be between 1 and {MAX_ACTIVE_MS} or infinity"),
    )
}

fn history_caps(opts: &Opts<'_>) -> Result<(Option<u64>, Option<u64>), FlowError> {
    let hot = match opts.get("history_hot_max_events") {
        None => None,
        Some(Term::Int(value)) if *value >= 0 => {
            let value = u64::try_from(*value).unwrap_or(u64::MAX);
            if value > MAX_HISTORY_HOT_MAX_EVENTS {
                return Err(FlowError::flow(
                    "history_hot_max_events",
                    &format!("exceeds maximum {MAX_HISTORY_HOT_MAX_EVENTS}"),
                ));
            }
            Some(value)
        }
        Some(_) => {
            return Err(FlowError::flow(
                "history_hot_max_events",
                "must be a non-negative integer",
            ));
        }
    };
    let max = match opts.get("history_max_events") {
        None => None,
        Some(Term::Int(value)) if *value > 0 => {
            let value = u64::try_from(*value).unwrap_or(u64::MAX);
            if value > MAX_HISTORY_MAX_EVENTS {
                return Err(FlowError::flow(
                    "history_max_events",
                    &format!("exceeds maximum {MAX_HISTORY_MAX_EVENTS}"),
                ));
            }
            Some(value)
        }
        Some(_) => {
            return Err(FlowError::flow(
                "history_max_events",
                "must be a positive integer",
            ));
        }
    };
    if let (Some(hot), Some(max)) = (hot, max)
        && max < hot
    {
        return Err(FlowError::rejected(
            "ERR flow history_max_events must be greater than or equal to history_hot_max_events"
                .to_owned(),
        ));
    }
    Ok((hot, max))
}

/// A value option of any term, in the value codec's stored form (audit gap
/// G5); `None` when absent.
///
/// # Errors
/// The term cannot be encoded.
pub fn stored(opts: &Opts<'_>, key: &str) -> Result<Option<Vec<u8>>, FlowError> {
    match opts.get(key) {
        None | Some(Term::Nil) => Ok(None),
        Some(value) => value_codec::store(value)
            .map(Some)
            .ok_or_else(|| FlowError::flow(key, "is too large")),
    }
}

/// The `values` option: names to values of any term, stored.
///
/// # Errors
/// The Elixir server's refusal text for an invalid value.
pub fn values(opts: &Opts<'_>) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, FlowError> {
    match opts.get("values") {
        None | Some(Term::Nil) => Ok(BTreeMap::new()),
        Some(Term::Map(entries)) => entries
            .iter()
            .map(|(name, value)| match value_codec::store(value) {
                Some(bytes) if !name.is_empty() => Ok((name.clone(), bytes)),
                _ => Err(FlowError::rejected(
                    "ERR flow values must map names to values".to_owned(),
                )),
            })
            .collect(),
        Some(_) => Err(FlowError::rejected(
            "ERR flow values must be a map".to_owned(),
        )),
    }
}

/// The `payload_ref` option: a stored value as the payload (audit gap
/// G16).
///
/// # Errors
/// Anything but a non-empty reference of at most 256 bytes.
pub fn payload_ref(opts: &Opts<'_>) -> Result<Option<Vec<u8>>, FlowError> {
    match opts.get("payload_ref") {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Bin(bytes)) if !bytes.is_empty() && bytes.len() <= MAX_REF_BYTES => {
            Ok(Some(bytes.clone()))
        }
        Some(_) => Err(FlowError::rejected(format!(
            "ERR flow payload_ref must be a reference of 1 to {MAX_REF_BYTES} bytes"
        ))),
    }
}

/// The `value_refs` option: names to references this Flow attaches but
/// does not own (audit gap G9).
///
/// # Errors
/// A name that is empty, or a reference that is not a non-empty binary up
/// to 256 bytes.
pub fn value_refs(opts: &Opts<'_>) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, FlowError> {
    match opts.get("value_refs") {
        None | Some(Term::Nil) => Ok(BTreeMap::new()),
        Some(Term::Map(entries)) => entries
            .iter()
            .map(|(name, reference)| match reference {
                Term::Bin(bytes)
                    if !name.is_empty() && !bytes.is_empty() && bytes.len() <= MAX_REF_BYTES =>
                {
                    Ok((name.clone(), bytes.clone()))
                }
                _ => Err(FlowError::rejected(
                    "ERR flow value_refs must map names to references".to_owned(),
                )),
            })
            .collect(),
        Some(_) => Err(FlowError::rejected(
            "ERR flow value_refs must be a map".to_owned(),
        )),
    }
}

/// The partition key option: a non-empty binary up to 256 bytes.
///
/// # Errors
/// The Elixir server's refusal text for an invalid value.
pub fn partition_key(opts: &Opts<'_>) -> Result<Option<Vec<u8>>, FlowError> {
    match opts.get("partition_key") {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Bin(value)) if !value.is_empty() => Ok(Some(value.clone())),
        // Elixir's text; `FLOW.INFO` and `FLOW.STATS` keep its shorter one.
        Some(_) => Err(FlowError::flow(
            "partition_key",
            "must be a non-empty string or :global",
        )),
    }
}

/// Reads `FLOW.CREATE`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid value.
pub fn parse(args: &Term) -> Result<CreateFlow, FlowError> {
    let opts = Opts::new("FLOW.CREATE", args, ALLOWED)?;
    let id = match opts.get("id") {
        Some(Term::Bin(id)) if !id.is_empty() => id.clone(),
        _ => {
            return Err(FlowError::rejected(
                "ERR flow id must be a non-empty string".to_owned(),
            ));
        }
    };
    bounded("id", Some(&id), MAX_FLOW_ID_BYTES)?;
    let flow_type = opts.required_binary("type")?;
    bounded("type", Some(&flow_type), MAX_TYPE_BYTES)?;
    // The schedule namespace is internal (ADR-014 step 13).
    if flow_type.as_slice() == super::schedule::SCHEDULE_TYPE.as_bytes() {
        return Err(FlowError::rejected(
            "ERR flow type is reserved for internal use".to_owned(),
        ));
    }
    if id.starts_with(super::schedule::FLOW_ID_PREFIX.as_bytes()) {
        return Err(FlowError::rejected(
            "ERR flow id is reserved for internal use".to_owned(),
        ));
    }
    let state = opts.optional_binary("state", b"queued")?;
    bounded("state", Some(&state), MAX_STATE_BYTES)?;
    if state == b"running" {
        return Err(FlowError::rejected(
            "ERR flow running state is only entered by FLOW.CLAIM_DUE".to_owned(),
        ));
    }
    let parent_flow_id = opts.binary_or_nil("parent_flow_id")?;
    ref_size("parent_flow_id", parent_flow_id.as_deref())?;
    let root_flow_id = opts.binary_or_nil("root_flow_id")?;
    ref_size("root_flow_id", root_flow_id.as_deref())?;
    let correlation_id = opts.binary_or_nil("correlation_id")?;
    ref_size("correlation_id", correlation_id.as_deref())?;
    bounded(
        "correlation_id",
        correlation_id.as_deref(),
        MAX_CORRELATION_ID_BYTES,
    )?;
    let idempotent = opts.boolean("idempotent", false)?;
    let now_ms = opts.non_neg_integer("now_ms")?;
    let run_at_ms = opts.non_neg_integer("run_at_ms")?.or(now_ms);
    let retention_ttl_ms = retention_ttl_ms(&opts)?;
    let max_active_ms = max_active_ms(&opts)?;
    let (history_hot_max_events, history_max_events) = history_caps(&opts)?;
    let priority = opts.priority(0)?;
    let partition_key = partition_key(&opts)?;
    bounded(
        "partition_key",
        partition_key.as_deref(),
        MAX_PARTITION_KEY_BYTES,
    )?;
    let attributes = meta::attributes(opts.get("attributes"))?;
    let state_meta = meta::state_meta_update(opts.get("state_meta"))?;
    // Elixir stores any term; it is kept in the value codec's form and read
    // back as sent (audit gap G5).
    let payload = stored(&opts, "payload")?;
    // A given payload wins over a reference, as in Elixir.
    let payload_ref = payload_ref(&opts)?.filter(|_| payload.is_none());
    Ok(CreateFlow {
        id,
        flow_type,
        state,
        partition_key,
        payload,
        parent_flow_id,
        root_flow_id,
        correlation_id,
        idempotent,
        now_ms,
        run_at_ms,
        priority,
        retention_ttl_ms,
        max_active_ms,
        history_hot_max_events,
        history_max_events,
        attributes,
        state_meta,
        values: values(&opts)?,
        value_refs: value_refs(&opts)?,
        payload_ref,
    })
}
