//! `FLOW.POLICY.SET` and `FLOW.POLICY.GET` (ADR-014 step 9), as
//! `Ferricstore.Flow.Policy`, `PolicyPatch` and `RetryPolicy` read, merge,
//! normalize and present a Flow type's policy.
//!
//! A policy is stored in its normalized form; a set merges its patch into
//! that form (or replaces it), normalizes the result again and raises the
//! type's generation. Top-level `retention`, `version`, `mode` and
//! `governance` are not options of the typed protocol, so they never reach a
//! policy; state-level governance is refused until governance is built.

use std::collections::BTreeMap;

use wf_domain::policy::{Backoff, BackoffKind, InstalledRetryPolicy, RetryOverride};

use super::FlowError;
use super::opts::{MAX_EXACT, Opts};
use super::term::Term;

mod merge;
mod normalize;
mod resolve;

pub use merge::merge;
pub use normalize::normalize;
pub use resolve::{Lifecycle, lifecycle, reply};

/// Largest serialized policy, as Elixir's 256 KiB snapshot bound.
const MAX_SNAPSHOT_BYTES: usize = 262_144;
/// Elixir's largest key; a policy key is `f:{f}:policy:` and the type.
const MAX_KEY_BYTES: usize = 65_535;
const POLICY_KEY_PREFIX_BYTES: usize = 13;
/// The keys of a set that belong to the policy itself.
const POLICY_KEYS: [&str; 5] = [
    "retry",
    "states",
    "max_active_ms",
    "indexed_attributes",
    "indexed_state_meta",
];

/// How Flows in one state are claimed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Any due Flow may run (the default).
    Parallel,
    /// One Flow at a time per partition, oldest first.
    Fifo,
}

impl Mode {
    /// The reply text.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Parallel => "parallel",
            Self::Fifo => "fifo",
        }
    }
}

/// A retention override; absent fields inherit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Retention {
    /// How long a terminal Flow is kept.
    pub ttl_ms: Option<u64>,
    /// How many history events a Flow keeps.
    pub history_max_events: Option<u64>,
}

/// One state's policy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatePolicy {
    /// Claim mode.
    pub mode: Option<Mode>,
    /// Retry override for Flows retried from this state.
    pub retry: Option<RetryOverride>,
    /// Retention for Flows created in this state.
    pub retention: Option<Retention>,
}

/// A Flow type's normalized policy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FlowPolicy {
    /// Type-wide active-time limit.
    pub max_active_ms: Option<u64>,
    /// Type-wide retry override.
    pub retry: Option<RetryOverride>,
    /// Per-state policies.
    pub states: BTreeMap<Vec<u8>, StatePolicy>,
    /// Attribute names queries index.
    pub indexed_attributes: Option<Vec<String>>,
    /// The `state_meta` key queries index.
    pub indexed_state_meta: Option<String>,
}

impl FlowPolicy {
    /// The claim mode of `state`.
    #[must_use]
    pub fn mode(&self, state: &[u8]) -> Mode {
        self.states
            .get(state)
            .and_then(|policy| policy.mode)
            .unwrap_or(Mode::Parallel)
    }

    /// The stored form: the map a later set merges its patch into.
    #[must_use]
    pub fn term(&self) -> Term {
        let mut entries = Vec::new();
        if let Some(max) = self.max_active_ms {
            entries.push((b"max_active_ms".to_vec(), Term::int(max)));
        }
        if let Some(retry) = &self.retry {
            entries.push((b"retry".to_vec(), override_term(retry)));
        }
        let states = self
            .states
            .iter()
            .map(|(state, policy)| (state.clone(), state_term(policy)))
            .collect();
        entries.push((b"states".to_vec(), Term::Map(states)));
        if let Some(names) = &self.indexed_attributes {
            let names = names.iter().map(|name| Term::text(name)).collect();
            entries.push((b"indexed_attributes".to_vec(), Term::List(names)));
        }
        if let Some(key) = &self.indexed_state_meta {
            entries.push((b"indexed_state_meta".to_vec(), Term::text(key)));
        }
        Term::Map(entries)
    }
}

/// A state's policy with only its given fields.
fn state_term(policy: &StatePolicy) -> Term {
    let mut fields = Vec::new();
    if let Some(mode) = policy.mode {
        fields.push((b"mode".to_vec(), Term::text(mode.as_str())));
    }
    if let Some(retry) = &policy.retry {
        fields.push((b"retry".to_vec(), override_term(retry)));
    }
    if let Some(retention) = &policy.retention {
        let mut kept = Vec::new();
        if let Some(ttl) = retention.ttl_ms {
            kept.push((b"ttl_ms".to_vec(), Term::int(ttl)));
        }
        if let Some(max) = retention.history_max_events {
            kept.push((b"history_max_events".to_vec(), Term::int(max)));
        }
        fields.push((b"retention".to_vec(), Term::Map(kept)));
    }
    Term::Map(fields)
}

/// A full backoff as a map.
fn backoff_term(backoff: &Backoff) -> Term {
    Term::Map(vec![
        (b"kind".to_vec(), Term::text(backoff.kind.as_str())),
        (b"base_ms".to_vec(), Term::int(backoff.base_ms)),
        (b"max_ms".to_vec(), Term::int(backoff.max_ms)),
        (
            b"jitter_pct".to_vec(),
            Term::int(u64::from(backoff.jitter_pct)),
        ),
    ])
}

/// A retry override with only its given fields.
fn override_term(retry: &RetryOverride) -> Term {
    let mut entries = Vec::new();
    if let Some(max) = retry.max_retries {
        entries.push((b"max_retries".to_vec(), Term::int(u64::from(max))));
    }
    if let Some(backoff) = &retry.backoff {
        entries.push((b"backoff".to_vec(), backoff_term(backoff)));
    }
    if let Some(target) = &retry.exhausted_to {
        entries.push((b"exhausted_to".to_vec(), Term::Bin(target.clone())));
    }
    Term::Map(entries)
}

/// The policy a shard holds for one type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    /// The type.
    pub flow_type: Vec<u8>,
    /// Raised by every set.
    pub generation: u64,
    /// The policy.
    pub policy: FlowPolicy,
    /// Its retry view, as the domain reducer reads it.
    pub retry: InstalledRetryPolicy,
}

impl Installed {
    /// Builds the entry for a normalized policy.
    ///
    /// # Errors
    /// Refuses a retry the domain does not accept.
    pub fn new(flow_type: Vec<u8>, generation: u64, policy: FlowPolicy) -> Result<Self, FlowError> {
        let states = policy
            .states
            .iter()
            .filter_map(|(state, entry)| Some((state.clone(), entry.retry.clone()?)))
            .collect();
        let retry = InstalledRetryPolicy::new(
            flow_type.clone(),
            policy.retry.clone().unwrap_or_default(),
            states,
        )
        .map_err(FlowError::Domain)?;
        Ok(Self {
            flow_type,
            generation,
            policy,
            retry,
        })
    }
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

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
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
    let mut allowed = vec!["type", "state", "expected_generation", "replace"];
    allowed.extend(POLICY_KEYS);
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
            .filter(|(key, _)| POLICY_KEYS.iter().any(|name| name.as_bytes() == key))
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

/// Refuses a policy larger than the snapshot bound.
///
/// # Errors
/// Elixir's snapshot-size refusal.
pub fn check_size(policy: &Term) -> Result<(), FlowError> {
    let mut encoded = Vec::new();
    let fits = policy.encode(&mut encoded).is_some() && encoded.len() <= MAX_SNAPSHOT_BYTES;
    if fits {
        Ok(())
    } else {
        Err(rejected(&format!(
            "ERR flow policy snapshot exceeds {MAX_SNAPSHOT_BYTES} bytes"
        )))
    }
}

/// The next generation after `stored`, as `PolicyMigration.next_generation/1`.
///
/// # Errors
/// Refuses a stale expected generation or an exhausted one.
pub fn next_generation(stored: u64, expected: Option<u64>) -> Result<u64, FlowError> {
    if expected.is_some_and(|expected| expected != stored) {
        return Err(rejected("ERR stale flow policy generation"));
    }
    stored
        .checked_add(1)
        .filter(|next| *next <= MAX_EXACT)
        .ok_or_else(|| rejected("ERR flow policy generation exhausted"))
}

/// The domain's backoff kind for a text.
pub(crate) fn backoff_kind(text: &[u8]) -> Option<BackoffKind> {
    match text {
        b"none" => Some(BackoffKind::None),
        b"fixed" => Some(BackoffKind::Fixed),
        b"linear" => Some(BackoffKind::Linear),
        b"exponential" => Some(BackoffKind::Exponential),
        _ => None,
    }
}
