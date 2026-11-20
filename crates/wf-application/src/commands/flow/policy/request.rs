//! Reading `FLOW.POLICY.SET` and `FLOW.POLICY.GET` requests, as
//! `Ferricstore.Flow.Policy.set/3` and `get/3` check their options.

use super::super::FlowError;
use super::super::opts::{MAX_EXACT, Opts};
use super::super::term::Term;
use super::{POLICY_KEYS, TEXT_POLICY_KEYS, check_size};

const MAX_KEY_BYTES: usize = 65_535;
const POLICY_KEY_PREFIX_BYTES: usize = 13;

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

/// `FLOW.POLICY.SET`.
#[derive(Clone, Debug, PartialEq)]
pub struct PolicySet {
    /// The type.
    pub flow_type: Vec<u8>,
    /// The policy keys given.
    pub patch: Vec<(Vec<u8>, Term)>,
    /// Replace the stored policy instead of merging into it.
    pub replace: bool,
    /// The generation the stored policy must have.
    pub expected_generation: Option<u64>,
    /// Reply for this state only.
    pub state: Option<Vec<u8>>,
}

/// `FLOW.POLICY.GET`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolicyGet {
    /// The type.
    pub flow_type: Vec<u8>,
    /// Reply for this state only.
    pub state: Option<Vec<u8>>,
}

/// The type and reply state every policy command names.
fn target(opts: &Opts<'_>) -> Result<(Vec<u8>, Option<Vec<u8>>), FlowError> {
    let flow_type = match opts.get("type") {
        Some(Term::Bin(flow_type)) => flow_type.clone(),
        _ => return Err(rejected("ERR native field type must be binary")),
    };
    if flow_type.is_empty() {
        return Err(rejected("ERR flow type must be a non-empty string"));
    }
    if flow_type.len().saturating_add(POLICY_KEY_PREFIX_BYTES) > MAX_KEY_BYTES {
        return Err(rejected(&format!(
            "ERR key too large (max {MAX_KEY_BYTES} bytes)"
        )));
    }
    let state = match opts.get("state") {
        None | Some(Term::Nil) => None,
        Some(Term::Bin(state)) => Some(state.clone()),
        Some(_) => return Err(rejected("ERR flow state must be a string")),
    };
    Ok((flow_type, state))
}

/// Reads `FLOW.POLICY.SET`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse_set(args: &Term) -> Result<PolicySet, FlowError> {
    parse_set_with(args, &POLICY_KEYS)
}

/// Reads `FLOW.POLICY.SET` sent as text, which may also carry a type-level
/// retention and (refused later) a type-level mode.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse_set_text(args: &Term) -> Result<PolicySet, FlowError> {
    parse_set_with(args, &TEXT_POLICY_KEYS)
}

fn parse_set_with(args: &Term, keys: &[&str]) -> Result<PolicySet, FlowError> {
    let mut allowed = vec!["type", "state", "expected_generation", "replace"];
    allowed.extend(keys);
    let opts = Opts::new("FLOW.POLICY.SET", args, &allowed)?;
    let (flow_type, state) = target(&opts)?;
    let expected_generation = match opts.get("expected_generation") {
        None | Some(Term::Nil) => None,
        Some(Term::Int(value)) => match u64::try_from(*value) {
            Ok(value) if value <= MAX_EXACT => Some(value),
            _ => {
                return Err(rejected(
                    "ERR flow expected_generation must be a non-negative integer",
                ));
            }
        },
        Some(_) => {
            return Err(rejected(
                "ERR flow expected_generation must be a non-negative integer",
            ));
        }
    };
    let replace = match opts.get("replace") {
        None => false,
        Some(Term::Bool(replace)) => *replace,
        Some(_) => return Err(rejected("ERR flow replace must be boolean")),
    };
    let patch: Vec<(Vec<u8>, Term)> = match args {
        Term::Map(entries) => entries
            .iter()
            .filter(|(key, _)| keys.iter().any(|name| name.as_bytes() == key))
            .cloned()
            .collect(),
        _ => Vec::new(),
    };
    check_size(&Term::Map(patch.clone()))?;
    Ok(PolicySet {
        flow_type,
        patch,
        replace,
        expected_generation,
        state,
    })
}

/// Reads `FLOW.POLICY.GET`.
///
/// # Errors
/// The Elixir server's refusal text for an invalid option.
pub fn parse_get(args: &Term) -> Result<PolicyGet, FlowError> {
    let opts = Opts::new("FLOW.POLICY.GET", args, &["type", "state"])?;
    let (flow_type, state) = target(&opts)?;
    Ok(PolicyGet { flow_type, state })
}
