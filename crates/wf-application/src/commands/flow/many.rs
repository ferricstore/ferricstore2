//! Batches of single-Flow mutations (ADR-014 step 3): `FLOW.COMPLETE_MANY`
//! (typed), `TRANSITION_MANY`, `RETRY_MANY`, `FAIL_MANY` and `CANCEL_MANY`.
//!
//! Each item becomes the options of one single-Flow mutation: the batch's
//! shared options, then the item's id, partition, lease token and fence. A
//! batch in one named partition that is not independent is all or nothing,
//! as in Elixir: it replies `OK` or the first item's error and changes
//! nothing. An independent batch, or one whose items name their own
//! partitions (`MIXED`), decides each item alone.

use super::FlowError;
use super::mutate::{self, Mutation};
use super::term::Term;
use crate::commands::queue::ServiceError;

/// One batch.
#[derive(Clone, Debug, PartialEq)]
pub struct ManyMutation {
    /// The batch's partition; `None` when each item names its own (`MIXED`).
    pub partition_key: Option<Vec<u8>>,
    /// Items succeed or fail on their own.
    pub independent: bool,
    /// Reply with a single `OK` when every item succeeds.
    pub ok_on_success: bool,
    /// The mutations, in request order.
    pub items: Vec<Mutation>,
}

impl ManyMutation {
    /// Whether the batch is decided all or nothing.
    #[must_use]
    pub fn atomic(&self) -> bool {
        !self.independent && self.partition_key.is_some()
    }
}

/// The single-Flow command of a batch command.
#[must_use]
pub fn single(name: &str) -> Option<&'static str> {
    Some(match name {
        "FLOW.COMPLETE_MANY" => "FLOW.COMPLETE",
        "FLOW.TRANSITION_MANY" => "FLOW.TRANSITION",
        "FLOW.RETRY_MANY" => "FLOW.RETRY",
        "FLOW.FAIL_MANY" => "FLOW.FAIL",
        "FLOW.CANCEL_MANY" => "FLOW.CANCEL",
        _ => return None,
    })
}

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

/// The item's own options, as Elixir's native server reads them: a map of
/// any options of the single command (audit gap G3), or a list whose order
/// depends on the command (G4): `[id, token, fence]` or `[id, partition,
/// token, fence]` for complete, retry and fail; `[id, fence, token]` or
/// `[id, partition, fence, token]` for a transition; `[id, fence]` or
/// `[id, partition, fence]` for a cancel.
fn item_entries(single: &str, item: &Term) -> Result<Vec<(Vec<u8>, Term)>, FlowError> {
    let order: (&[&str], &[&str]) = match single {
        "FLOW.TRANSITION" => (
            &["id", "fencing_token", "lease_token"],
            &["id", "partition_key", "fencing_token", "lease_token"],
        ),
        "FLOW.CANCEL" => (
            &["id", "fencing_token"],
            &["id", "partition_key", "fencing_token"],
        ),
        _ => (
            &["id", "lease_token", "fencing_token"],
            &["id", "partition_key", "lease_token", "fencing_token"],
        ),
    };
    match item {
        Term::List(fields) => {
            let keys = if fields.len() == order.0.len() {
                order.0
            } else if fields.len() == order.1.len() {
                order.1
            } else {
                return Err(rejected("ERR flow many item has wrong width"));
            };
            Ok(keys
                .iter()
                .zip(fields)
                .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
                .collect())
        }
        Term::Map(entries) => Ok(entries.clone()),
        _ => Err(rejected("ERR flow many item must be a list or a map")),
    }
}

/// Reads one batch command.
///
/// # Errors
/// The Elixir server's refusal text for invalid options or items.
pub fn parse(name: &str, args: &Term) -> Result<ManyMutation, FlowError> {
    let single = single(name).ok_or_else(|| {
        FlowError::rejected(format!("ERR {name} is not supported by this server"))
    })?;
    let Term::Map(entries) = args else {
        return Err(FlowError::rejected(format!(
            "ERR native {name} payload must be a map"
        )));
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
    if items.len() > 100_000 {
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
        for (key, value) in item_entries(single, item)? {
            options.retain(|(existing, _)| *existing != key);
            options.push((key, value));
        }
        parsed.push(mutate::parse(single, &Term::Map(options))?);
    }
    if !independent {
        let mut seen: Vec<(&[u8], Option<&[u8]>)> = Vec::new();
        for mutation in &parsed {
            let key = (mutation.id.as_slice(), mutation.partition_key.as_deref());
            if seen.contains(&key) {
                return Err(rejected("ERR flow duplicate id in batch"));
            }
            seen.push(key);
        }
    }
    Ok(ManyMutation {
        partition_key,
        independent,
        ok_on_success,
        items: parsed,
    })
}

/// The reply to a batch from its per-item outcomes, as Elixir replies.
///
/// # Errors
/// An atomic batch's first error.
pub fn reply(
    batch: &ManyMutation,
    outcomes: Vec<Result<Term, ServiceError>>,
) -> Result<Term, ServiceError> {
    let all_ok = outcomes.iter().all(Result::is_ok);
    if batch.atomic() {
        return match outcomes.into_iter().find_map(Result::err) {
            Some(error) => Err(error),
            None => Ok(Term::text("OK")),
        };
    }
    if all_ok {
        return Ok(if batch.ok_on_success || !batch.independent {
            Term::text("OK")
        } else {
            Term::List(outcomes.iter().map(|_| Term::text("OK")).collect())
        });
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
