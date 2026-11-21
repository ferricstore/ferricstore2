//! Named limit, budget and circuit rules in a governance policy, as
//! `normalize_limit_rule/1`, `normalize_budget_rule/1`,
//! `normalize_circuit_rule/1` and `optional_rule_map/4` check them.

use super::super::super::FlowError;
use super::super::super::term::Term;
use super::{
    Entries, MAX_STRING_BYTES, Normalizer, Pairs, as_map, choice, get, integer, put, rejected,
    section, strings,
};

const MAX_RULES: usize = 1_000;
const MAX_ERROR_CLASS_BYTES: usize = 256;
const MAX_TRACKED: i64 = 64;

fn enforcement(rule: &Pairs) -> Result<Option<Term>, FlowError> {
    choice(
        get(rule, "enforcement"),
        &["strict_global", "approximate_global"],
        "ERR invalid flow governance enforcement mode",
    )
}

pub(super) fn limit_rule(rule: &Pairs) -> Result<Entries, FlowError> {
    let mut out = Vec::new();
    put(
        &mut out,
        "limit",
        integer(get(rule, "limit"), "limit", 0, true)?,
    );
    put(&mut out, "enforcement", enforcement(rule)?);
    put(
        &mut out,
        "lease_size",
        integer(get(rule, "lease_size"), "lease_size", 1, false)?,
    );
    Ok(out)
}

pub(super) fn budget_rule(rule: &Pairs) -> Result<Entries, FlowError> {
    let mut out = Vec::new();
    put(
        &mut out,
        "limit",
        integer(get(rule, "limit"), "limit", 0, true)?,
    );
    put(
        &mut out,
        "window_ms",
        integer(get(rule, "window_ms"), "window_ms", 1, true)?,
    );
    put(&mut out, "enforcement", enforcement(rule)?);
    put(
        &mut out,
        "lease_size",
        integer(get(rule, "lease_size"), "lease_size", 1, false)?,
    );
    let unit = match get(rule, "unit") {
        None => None,
        Some(Term::Bin(unit)) if !unit.is_empty() && unit.len() <= MAX_STRING_BYTES => {
            Some(Term::Bin(unit.clone()))
        }
        Some(_) => {
            return Err(rejected(format!(
                "ERR flow governance unit must be a non-empty string of at most {MAX_STRING_BYTES} bytes"
            )));
        }
    };
    put(&mut out, "unit", unit);
    Ok(out)
}

pub(super) fn circuit_rule(rule: &Pairs) -> Result<Entries, FlowError> {
    let mut out = Vec::new();
    let threshold = integer(get(rule, "failure_threshold"), "failure_threshold", 1, true)?;
    let threshold_value = match threshold {
        Some(Term::Int(value)) => value,
        _ => 0,
    };
    put(&mut out, "failure_threshold", threshold);
    put(
        &mut out,
        "open_ms",
        integer(get(rule, "open_ms"), "open_ms", 1, true)?,
    );
    put(
        &mut out,
        "window_ms",
        integer(get(rule, "window_ms"), "window_ms", 1, false)?,
    );
    let min_calls = integer(get(rule, "min_calls"), "min_calls", 1, false)?;
    let rate = match get(rule, "failure_rate_pct") {
        None => None,
        Some(Term::Int(rate)) if (1..=100).contains(rate) => Some(Term::Int(*rate)),
        Some(_) => {
            return Err(rejected(
                "ERR flow governance failure_rate_pct must be an integer from 1 to 100",
            ));
        }
    };
    put(
        &mut out,
        "latency_threshold_ms",
        integer(
            get(rule, "latency_threshold_ms"),
            "latency_threshold_ms",
            1,
            false,
        )?,
    );
    put(
        &mut out,
        "error_classes",
        strings(
            get(rule, "error_classes"),
            "error_classes",
            MAX_ERROR_CLASS_BYTES,
        )?,
    );
    put(
        &mut out,
        "half_open_max_probes",
        integer(
            get(rule, "half_open_max_probes"),
            "half_open_max_probes",
            1,
            false,
        )?,
    );
    put(
        &mut out,
        "half_open_success_threshold",
        integer(
            get(rule, "half_open_success_threshold"),
            "half_open_success_threshold",
            1,
            false,
        )?,
    );
    if matches!(min_calls, Some(Term::Int(calls)) if calls > MAX_TRACKED) {
        return Err(rejected(
            "ERR flow governance circuit min_calls must be <= 64",
        ));
    }
    if threshold_value > MAX_TRACKED && rate.is_none() {
        return Err(rejected(
            "ERR flow governance circuit failure_threshold must be <= 64 without failure_rate_pct",
        ));
    }
    if threshold_value > MAX_TRACKED && min_calls.is_none() {
        return Err(rejected(
            "ERR flow governance circuit min_calls is required when failure_threshold exceeds 64",
        ));
    }
    put(&mut out, "min_calls", min_calls);
    put(&mut out, "failure_rate_pct", rate);
    Ok(out)
}

/// `optional_rule_map/4`: named rules, at most 1000, checked in name order.
pub(super) fn rules(
    value: Option<&Term>,
    name: &str,
    rule: &str,
    normalize: Normalizer,
) -> Result<Option<Term>, FlowError> {
    let Some(entries) = section(value, name)? else {
        return Ok(None);
    };
    if entries.len() > MAX_RULES {
        return Err(rejected(format!(
            "ERR flow governance {name} policy must contain at most {MAX_RULES} rules"
        )));
    }
    let mut sorted: Vec<&(Vec<u8>, Term)> = entries.iter().collect();
    sorted.sort_by(|left, right| left.0.cmp(&right.0));
    let mut out = Vec::new();
    for (key, body) in sorted {
        if key.is_empty() {
            return Err(rejected(format!(
                "ERR flow governance {name} names must be non-empty strings"
            )));
        }
        if key.len() > MAX_STRING_BYTES {
            return Err(rejected(format!(
                "ERR flow governance {name} names must be at most {MAX_STRING_BYTES} bytes"
            )));
        }
        let body = as_map(body).ok_or_else(|| {
            rejected(format!(
                "ERR flow governance {rule} rule must be a map or keyword list"
            ))
        })?;
        out.push((key.clone(), Term::Map(normalize(body)?)));
    }
    Ok(Some(Term::Map(out)))
}
