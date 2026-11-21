//! `FLOW.CREATE_MANY` beyond the queue fast path (audit gap G2), as
//! Elixir's `flow_create_many` decides it.
//!
//! Each item becomes the options of one `FLOW.CREATE`: the batch's shared
//! options, then the item's own: a map of any create options, or a list
//! `[id]`, `[id, payload]` or `[id, partition, payload]`. A batch that names
//! its partition and is not independent is all or nothing. A batch without
//! one (`MIXED`) is all or nothing per shard group: a failing group's items
//! all fail, other groups are created. An independent batch decides each
//! item alone. Ids repeat only in independent batches.

use super::FlowError;
use super::create::{self, CreateFlow};
use super::term::Term;
use crate::commands::queue::ServiceError;

/// Most items one batch takes.
const MAX_ITEMS: usize = 100_000;

/// One batch of creates.
#[derive(Clone, Debug, PartialEq)]
pub struct CreateMany {
    /// The batch's partition; `None` when each item names its own or gets
    /// an auto bucket.
    pub partition_key: Option<Vec<u8>>,
    /// Items succeed or fail on their own.
    pub independent: bool,
    /// Reply with a single `OK` when every item succeeds.
    pub ok_on_success: bool,
    /// The creates, in request order.
    pub items: Vec<CreateFlow>,
}

impl CreateMany {
    /// Whether the whole batch is decided all or nothing.
    #[must_use]
    pub const fn atomic(&self) -> bool {
        !self.independent && self.partition_key.is_some()
    }
}

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

/// The item's own options.
fn item_entries(item: &Term) -> Result<Vec<(Vec<u8>, Term)>, FlowError> {
    let named = |key: &str, value: &Term| (key.as_bytes().to_vec(), value.clone());
    match item {
        Term::Bin(_) => Ok(vec![named("id", item)]),
        Term::List(fields) => match fields.as_slice() {
            [id] => Ok(vec![named("id", id)]),
            [id, payload] => Ok(vec![named("id", id), named("payload", payload)]),
            [id, partition, payload] => Ok(vec![
                named("id", id),
                named("partition_key", partition),
                named("payload", payload),
            ]),
            _ => Err(rejected("ERR native flow item is invalid")),
        },
        Term::Map(entries) => Ok(entries.clone()),
        _ => Err(rejected("ERR native flow item is invalid")),
    }
}

/// Reads one batch.
///
/// # Errors
/// The Elixir server's refusal text for invalid options or items.
pub fn parse(args: &Term) -> Result<CreateMany, FlowError> {
    let Term::Map(entries) = args else {
        return Err(rejected(
            "ERR native FLOW.CREATE_MANY payload must be a map",
        ));
    };
    let get = |key: &str| {
        entries
            .iter()
            .find(|(name, _)| name.as_slice() == key.as_bytes())
            .map(|(_, value)| value)
    };
    let independent = match get("independent") {
        None | Some(Term::Nil) => false,
        Some(Term::Bool(value)) => *value,
        Some(_) => return Err(FlowError::flow("independent", "must be a boolean")),
    };
    let ok_on_success = matches!(get("return"), Some(Term::Bin(mode)) if mode.eq_ignore_ascii_case(b"ok_on_success"));
    let partition_key = match get("partition_key") {
        None | Some(Term::Nil) => None,
        Some(Term::Bin(key)) if !key.is_empty() => Some(key.clone()),
        Some(_) => {
            return Err(FlowError::flow(
                "partition_key",
                "must be a non-empty string",
            ));
        }
    };
    let items = match get("items") {
        Some(Term::List(items)) if !items.is_empty() => items,
        _ => return Err(rejected("ERR flow items must be a non-empty list")),
    };
    if items.len() > MAX_ITEMS {
        return Err(rejected("ERR flow items exceeds maximum 100000"));
    }
    let shared: Vec<(Vec<u8>, Term)> = entries
        .iter()
        .filter(|(key, _)| !matches!(key.as_slice(), b"independent" | b"return" | b"items"))
        .cloned()
        .collect();
    let mut parsed = Vec::with_capacity(items.len());
    for item in items {
        let mut options = shared.clone();
        for (key, value) in item_entries(item)? {
            options.retain(|(existing, _)| *existing != key);
            options.push((key, value));
        }
        parsed.push(create::parse(&Term::Map(options))?);
    }
    if !independent {
        let mut seen: Vec<(&[u8], Option<&[u8]>)> = Vec::new();
        for create in &parsed {
            let key = (create.id.as_slice(), create.partition_key.as_deref());
            if seen.contains(&key) {
                return Err(rejected("ERR flow duplicate id in batch"));
            }
            seen.push(key);
        }
    }
    Ok(CreateMany {
        partition_key,
        independent,
        ok_on_success,
        items: parsed,
    })
}

/// The reply from the per-item outcomes, as for the other batches.
///
/// # Errors
/// An atomic batch's first error.
pub fn reply(
    batch: &CreateMany,
    outcomes: Vec<Result<Term, ServiceError>>,
) -> Result<Term, ServiceError> {
    if batch.atomic() {
        return match outcomes.into_iter().find_map(Result::err) {
            Some(error) => Err(error),
            None => Ok(Term::text("OK")),
        };
    }
    if outcomes.iter().all(Result::is_ok) && (batch.ok_on_success || !batch.independent) {
        return Ok(Term::text("OK"));
    }
    Ok(Term::List(
        outcomes
            .into_iter()
            .map(|outcome| match outcome {
                Ok(_) => Term::text("ok"),
                Err(error) => Term::List(vec![
                    Term::text("error"),
                    Term::Bin(error.to_string().into_bytes()),
                ]),
            })
            .collect(),
    ))
}
