//! `FLOW.GET`: one Flow's record, or nil.

use super::FlowError;
use super::create::partition_key;
use super::opts::Opts;
use super::record::Include;
use super::term::Term;

const ALLOWED: &[&str] = &[
    "id",
    "partition_key",
    "values",
    "value_max_bytes",
    "payload",
    "full",
    "payload_max_bytes",
    "include_cold",
    "consistent_projection",
];

/// Elixir's `flow_payload_return_max_bytes`: 64 KiB.
const PAYLOAD_MAX_BYTES: u64 = 65_536;

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
    // `PayloadReturn.options/2`: `payload` defaults to `full`; the cold and
    // projection hints are checked and have no use on a single node.
    let full = opts.boolean("full", false)?;
    let payload = opts.boolean("payload", full)?;
    for flag in ["include_cold", "consistent_projection"] {
        opts.boolean(flag, false)?;
    }
    let max_bytes = opts.non_neg_integer("payload_max_bytes")?;
    if max_bytes.is_some_and(|max| max > PAYLOAD_MAX_BYTES) {
        return Err(FlowError::rejected(format!(
            "ERR flow payload_max_bytes exceeds maximum {PAYLOAD_MAX_BYTES}"
        )));
    }
    Ok(GetFlow {
        id,
        partition_key: partition_key(&opts)?,
        include: Include {
            payload,
            max_bytes: max_bytes.unwrap_or(PAYLOAD_MAX_BYTES),
            values: super::claim::values(&opts)?,
        },
    })
}
