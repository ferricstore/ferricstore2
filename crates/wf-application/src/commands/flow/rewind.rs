//! `FLOW.REWIND` (ADR-014 step 10), as `MutationAttrs.rewind_attrs/2`
//! reads it: the Flow, the history event to restore and optional guards; a
//! `reason` becomes the rewound Flow's error.

use super::FlowError;
use super::create::partition_key;
use super::opts::Opts;
use super::term::Term;

/// One rewind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rewind {
    /// Flow id.
    pub id: Vec<u8>,
    /// Partition, if sent.
    pub partition_key: Option<Vec<u8>>,
    /// The history event to restore, `<ms>-<version>`.
    pub to_event: Vec<u8>,
    /// The state the Flow must be in.
    pub expect_state: Option<Vec<u8>>,
    /// Due time instead of the event's.
    pub run_at_ms: Option<u64>,
    /// Command time.
    pub now_ms: Option<u64>,
    /// The rewound Flow's error, when given.
    pub reason: Option<Vec<u8>>,
}

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

/// Reads `FLOW.REWIND`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse(args: &Term) -> Result<Rewind, FlowError> {
    let opts = Opts::new(
        "FLOW.REWIND",
        args,
        &[
            "id",
            "partition_key",
            "to_event",
            "expect_state",
            "run_at_ms",
            "now_ms",
            "reason",
            "reason_ref",
        ],
    )?;
    if opts.has("reason_ref") {
        return Err(rejected(
            "ERR flow reason_ref input is not supported; use reason",
        ));
    }
    let id = match opts.get("id") {
        Some(Term::Bin(id)) if !id.is_empty() => id.clone(),
        _ => return Err(rejected("ERR flow id must be a non-empty string")),
    };
    let to_event = opts.required_binary("to_event")?;
    let expect_state = opts.binary_or_nil("expect_state")?;
    let run_at_ms = opts.non_neg_integer("run_at_ms")?;
    let now_ms = opts.non_neg_integer("now_ms")?;
    let reason = match opts.get("reason") {
        None | Some(Term::Nil) => None,
        Some(Term::Bin(reason)) => Some(reason.clone()),
        Some(_) => return Err(rejected("ERR flow reason must be a string")),
    };
    Ok(Rewind {
        id,
        partition_key: partition_key(&opts)?,
        to_event,
        expect_state,
        run_at_ms,
        now_ms,
        reason,
    })
}

/// The version an event id names, as `HistoryEvent.version/1`: 0 for an id
/// this server did not generate.
#[must_use]
pub fn event_version(event_id: &[u8]) -> (u64, u64) {
    let text = std::str::from_utf8(event_id).unwrap_or_default();
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    match text.split_once('-') {
        Some((at, version)) if digits(at) && digits(version) => {
            (at.parse().unwrap_or(0), version.parse().unwrap_or(0))
        }
        _ => (0, 0),
    }
}
