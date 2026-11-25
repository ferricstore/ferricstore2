//! A claim's or read's state and value filters, as Elixir reads them.

use super::super::FlowError;
use super::super::opts::Opts;
use super::super::record::Values;
use super::super::term::Term;

pub(super) fn states(opts: &Opts<'_>) -> Result<Vec<Vec<u8>>, FlowError> {
    let names = match (opts.get("state"), opts.get("states")) {
        (Some(_), Some(_)) => {
            return Err(FlowError::rejected(
                "ERR flow state and states are mutually exclusive".to_owned(),
            ));
        }
        (Some(value), None) | (None, Some(value)) => value,
        (None, None) => return Ok(Vec::new()),
    };
    let list = match names {
        Term::List(items) => items.as_slice(),
        single => std::slice::from_ref(single),
    };
    // `any` is every state, as Elixir reads it (audit gap G20), and cannot
    // be named with other states (G68).
    let any = |state: &Term| matches!(state, Term::Bin(name) if name.eq_ignore_ascii_case(b"any"));
    if list.iter().any(any) {
        return if list.iter().all(any) {
            Ok(Vec::new())
        } else {
            Err(FlowError::rejected(
                "ERR flow STATE ANY cannot be mixed with explicit states".to_owned(),
            ))
        };
    }
    let mut states = Vec::new();
    for state in list {
        match state {
            Term::Bin(name) if !name.is_empty() => {
                if !states.contains(name) {
                    states.push(name.clone());
                }
            }
            _ => return Err(FlowError::flow("state", "must be a non-empty string")),
        }
    }
    Ok(states)
}

/// A claim's or read's `values`: `true`, `false`, a name or a name list.
pub(in crate::commands::flow) fn values(opts: &Opts<'_>) -> Result<Values, FlowError> {
    match opts.get("values") {
        None => Ok(Values::None),
        Some(Term::Bool(true)) => Ok(Values::All),
        Some(Term::Bool(false)) => Ok(Values::Names(Vec::new())),
        Some(Term::Bin(name)) if !name.is_empty() => Ok(Values::Names(vec![name.clone()])),
        Some(Term::List(items)) => {
            let mut names: Vec<Vec<u8>> = Vec::new();
            for item in items {
                let Term::Bin(name) = item else {
                    return Err(FlowError::flow("value name", "must be a non-empty string"));
                };
                if name.is_empty() {
                    return Err(FlowError::flow("value name", "must be a non-empty string"));
                }
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
            Ok(Values::Names(names))
        }
        Some(_) => Err(FlowError::rejected(
            "ERR flow values must be true, false, a name, or a name list".to_owned(),
        )),
    }
}
