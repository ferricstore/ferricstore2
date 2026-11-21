//! Reading `FLOW.GOVERNANCE.LEDGER`, as `Governance.Ledger.list/3` checks
//! its options.

use super::super::super::FlowError;
use super::super::super::keys;
use super::super::super::opts::{MAX_EXACT, Opts};
use super::super::super::term::Term;
use super::super::{LEDGER_MAX_EVENTS, MAX_KEY_BYTES};
use super::{key_size, native, partition_key, rejected};
use crate::routing::Partition;

/// `FLOW.GOVERNANCE.LEDGER`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LedgerList {
    /// The Flow.
    pub id: Vec<u8>,
    /// Its explicit partition.
    pub partition_key: Option<Vec<u8>>,
    /// Most events returned.
    pub limit: usize,
    /// Newest first.
    pub rev: bool,
    /// Events at or after.
    pub from_ms: u64,
    /// Events at or before.
    pub to_ms: Option<u64>,
}

fn ledger_integer(opts: &Opts<'_>, key: &str) -> Result<Option<u64>, FlowError> {
    match opts.get(key) {
        None => Ok(None),
        Some(Term::Int(value)) => u64::try_from(*value)
            .ok()
            .filter(|value| *value <= MAX_EXACT)
            .map(Some)
            .ok_or_else(|| {
                rejected(format!(
                    "ERR flow governance ledger {key} must be a non-negative integer"
                ))
            }),
        Some(_) => Err(rejected(format!(
            "ERR flow governance ledger {key} must be a non-negative integer"
        ))),
    }
}

/// Reads `FLOW.GOVERNANCE.LEDGER`.
///
/// # Errors
/// Elixir's text for an invalid option.
pub fn ledger(args: &Term) -> Result<LedgerList, FlowError> {
    let id = native(args, "id")?;
    let opts = Opts::new(
        "FLOW.GOVERNANCE.LEDGER",
        args,
        &[
            "id",
            "partition_key",
            "limit",
            "rev",
            "from_ms",
            "to_ms",
            "deadline_ms",
        ],
    )?;
    if id.is_empty() || id.len() > MAX_KEY_BYTES {
        return Err(rejected(
            "ERR flow id must be a non-empty string".to_owned(),
        ));
    }
    let partition_key = partition_key(&opts)?;
    let partition = Partition::for_new_flow(&id, partition_key.as_deref());
    let mut index = keys::governance_prefix(&partition, "li");
    index.extend_from_slice(&id);
    key_size(&index).map_err(rejected)?;
    let limit = match opts.get("limit") {
        None => 100,
        Some(Term::Int(limit)) if *limit > 0 => usize::try_from(*limit)
            .unwrap_or(usize::MAX)
            .min(LEDGER_MAX_EVENTS),
        Some(_) => {
            return Err(rejected(
                "ERR flow governance ledger limit must be a positive integer".to_owned(),
            ));
        }
    };
    let rev = match opts.get("rev") {
        None => false,
        Some(Term::Bool(rev)) => *rev,
        Some(_) => {
            return Err(rejected(
                "ERR flow governance ledger rev must be boolean".to_owned(),
            ));
        }
    };
    Ok(LedgerList {
        from_ms: ledger_integer(&opts, "from_ms")?.unwrap_or(0),
        to_ms: ledger_integer(&opts, "to_ms")?,
        id,
        partition_key,
        limit,
        rev,
    })
}
