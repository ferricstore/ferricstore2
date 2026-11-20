//! `FLOW.GET`: one Flow's record, or nil.

use super::FlowError;
use super::create::partition_key;
use super::opts::Opts;
use super::record::{Include, Values};
use super::term::Term;

const ALLOWED: &[&str] = &[
    "id",
    "partition_key",
    "values",
    "value_max_bytes",
    "payload",
];

/// Read one Flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetFlow {
    /// Flow id.
    pub id: Vec<u8>,
    /// Partition, if sent; the auto bucket of the id when absent.
    pub partition_key: Option<Vec<u8>>,
    /// What to include besides the record.
    pub include: Include,
}

/// Reads `FLOW.GET`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid value.
pub fn parse(args: &Term) -> Result<GetFlow, FlowError> {
    let opts = Opts::new("FLOW.GET", args, ALLOWED)?;
    let id = match opts.get("id") {
        Some(Term::Bin(id)) if !id.is_empty() => id.clone(),
        _ => {
            return Err(FlowError::rejected(
                "ERR flow id must be a non-empty string".to_owned(),
            ));
        }
    };
    opts.non_neg_integer("value_max_bytes")?;
    Ok(GetFlow {
        id,
        partition_key: partition_key(&opts)?,
        include: Include {
            // Elixir accepts the flag on FLOW.GET and still returns only the
            // payload reference.
            payload: false,
            values: Values::Names(opts.binary_list("values")?),
            ..Include::default()
        },
    })
}
