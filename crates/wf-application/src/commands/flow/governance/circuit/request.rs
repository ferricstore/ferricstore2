//! Reading circuit requests in the order `CircuitStore` checks them.

use super::super::super::opts::{MAX_EXACT, Opts};
use super::super::super::term::Term;
use super::super::super::{FlowError, keys};
use super::super::{MAX_FIELD_BYTES, key_size, native};
use super::{Command, Rule};
use crate::routing::Partition;

const MAX_ERROR_CLASSES: usize = 1_000;
const MAX_ERROR_CLASS_BYTES: usize = 256;
const MAX_TRACKED: u64 = 64;

fn rejected(text: String) -> FlowError {
    FlowError::rejected(text)
}

fn positive(opts: &Opts<'_>, key: &str) -> Result<Option<u64>, FlowError> {
    match opts.get(key) {
        None | Some(Term::Nil) => Ok(None),
        Some(Term::Int(value))
            if *value > 0 && u64::try_from(*value).is_ok_and(|value| value <= MAX_EXACT) =>
        {
            Ok(u64::try_from(*value).ok())
        }
        Some(_) => Err(rejected(format!(
            "ERR flow circuit {key} must be a positive integer"
        ))),
    }
}

fn error_classes(opts: &Opts<'_>) -> Result<Option<Vec<Vec<u8>>>, FlowError> {
    let items = match opts.get("error_classes") {
        None | Some(Term::Nil) => return Ok(None),
        Some(Term::List(items)) => items,
        Some(_) => {
            return Err(rejected(
                "ERR flow circuit error_classes must be a list of non-empty strings".to_owned(),
            ));
        }
    };
    let mut kept: Vec<Vec<u8>> = Vec::new();
    for (count, item) in items.iter().enumerate() {
        if count >= MAX_ERROR_CLASSES {
            return Err(rejected(
                "ERR flow circuit error_classes must contain at most 1000 values".to_owned(),
            ));
        }
        match item {
            Term::Bin(class) if !class.is_empty() && class.len() <= MAX_ERROR_CLASS_BYTES => {
                if !kept.contains(class) {
                    kept.push(class.clone());
                }
            }
            Term::Bin(class) if !class.is_empty() => {
                return Err(rejected(format!(
                    "ERR flow circuit error_classes must contain non-empty strings of at most {MAX_ERROR_CLASS_BYTES} bytes"
                )));
            }
            _ => {
                return Err(rejected(
                    "ERR flow circuit error_classes must be a list of non-empty strings".to_owned(),
                ));
            }
        }
    }
    Ok(Some(kept))
}

/// `optional_rule_opts/1`, then `validate_tracking_bounds/3`. The call
/// details a manual open accepts are checked and unused.
fn rule(opts: &Opts<'_>) -> Result<Rule, FlowError> {
    let open_ms = positive(opts, "open_ms")?;
    let failure_threshold = positive(opts, "failure_threshold")?;
    let window_ms = positive(opts, "window_ms")?;
    let min_calls = positive(opts, "min_calls")?;
    let failure_rate_pct = match opts.get("failure_rate_pct") {
        None | Some(Term::Nil) => None,
        Some(Term::Int(value)) if (1..=100).contains(value) => u64::try_from(*value).ok(),
        Some(_) => {
            return Err(rejected(
                "ERR flow circuit failure_rate_pct must be an integer from 1 to 100".to_owned(),
            ));
        }
    };
    let latency_threshold_ms = positive(opts, "latency_threshold_ms")?;
    let error_classes = error_classes(opts)?;
    let half_open_max_probes = positive(opts, "half_open_max_probes")?;
    let half_open_success_threshold = positive(opts, "half_open_success_threshold")?;
    match opts.get("latency_ms") {
        None | Some(Term::Nil) => {}
        Some(Term::Int(value)) if u64::try_from(*value).is_ok_and(|value| value <= MAX_EXACT) => {}
        Some(_) => {
            return Err(rejected(
                "ERR flow circuit latency_ms must be a non-negative integer".to_owned(),
            ));
        }
    }
    match opts.get("error_class") {
        None | Some(Term::Nil) => {}
        Some(Term::Bin(class)) if !class.is_empty() && class.len() <= MAX_FIELD_BYTES => {}
        Some(_) => {
            return Err(rejected(format!(
                "ERR flow circuit error_class must be a non-empty string of at most {MAX_FIELD_BYTES} bytes"
            )));
        }
    }
    let threshold = failure_threshold.unwrap_or(5);
    if min_calls.is_some_and(|calls| calls > MAX_TRACKED) {
        return Err(rejected(
            "ERR flow circuit min_calls must be <= 64".to_owned(),
        ));
    }
    if threshold > MAX_TRACKED {
        if failure_rate_pct.is_none() {
            return Err(rejected(
                "ERR flow circuit failure_threshold must be <= 64 without failure_rate_pct"
                    .to_owned(),
            ));
        }
        if min_calls.is_none() {
            return Err(rejected(
                "ERR flow circuit min_calls is required when failure_threshold exceeds 64"
                    .to_owned(),
            ));
        }
    }
    Ok(Rule {
        failure_threshold,
        open_ms,
        window_ms,
        min_calls,
        failure_rate_pct,
        latency_threshold_ms,
        error_classes,
        half_open_max_probes,
        half_open_success_threshold,
    })
}

/// Reads `FLOW.CIRCUIT.OPEN`, `CLOSE` or `GET`.
///
/// # Errors
/// Elixir's text for an invalid option, or an unknown command.
pub fn parse(name: &str, args: &Term) -> Result<Command, FlowError> {
    let scope = native(args, "scope")?;
    let allowed: &[&str] = match name {
        "FLOW.CIRCUIT.OPEN" => &[
            "scope",
            "now_ms",
            "open_ms",
            "failure_threshold",
            "window_ms",
            "min_calls",
            "failure_rate_pct",
            "latency_threshold_ms",
            "error_classes",
            "half_open_max_probes",
            "half_open_success_threshold",
            "latency_ms",
            "error_class",
            "deadline_ms",
        ],
        "FLOW.CIRCUIT.CLOSE" => &["scope", "now_ms", "deadline_ms"],
        "FLOW.CIRCUIT.GET" => &["scope", "deadline_ms"],
        _ => {
            return Err(rejected(format!(
                "ERR {name} is not supported by this server"
            )));
        }
    };
    let opts = Opts::new(name, args, allowed)?;
    if scope.is_empty() {
        return Err(rejected(
            "ERR flow circuit scope must be a non-empty string".to_owned(),
        ));
    }
    let prefix = keys::governance_prefix(&Partition::parse(&scope), "c").len();
    key_size(prefix.saturating_add(scope.len()))?;
    if name == "FLOW.CIRCUIT.GET" {
        return Ok(Command::Get(scope));
    }
    let now_ms = match opts.get("now_ms") {
        None => None,
        Some(Term::Int(now)) if u64::try_from(*now).is_ok_and(|now| now <= MAX_EXACT) => {
            u64::try_from(*now).ok()
        }
        Some(_) => {
            return Err(rejected(
                "ERR flow circuit now_ms must be a non-negative integer".to_owned(),
            ));
        }
    };
    if name == "FLOW.CIRCUIT.CLOSE" {
        return Ok(Command::Close { scope, now_ms });
    }
    Ok(Command::Open {
        rule: Box::new(rule(&opts)?),
        scope,
        now_ms,
    })
}
