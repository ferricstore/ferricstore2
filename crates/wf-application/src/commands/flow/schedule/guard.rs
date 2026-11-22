//! What a schedule command may carry: only its own options (Elixir's
//! `validate_option_fields/3`), and the `expected_state` and
//! `expected_version` guards that keep an operator's change to the
//! schedule they read.
//!
//! Elixir reaches these guards through its API and dashboard; this node's
//! only API is the wire, so the wire takes them.

use super::super::FlowError;
use super::super::opts::Opts;
use super::super::term::Term;
use super::rejected;

/// The command's options, refusing a Flow option it does not take by name;
/// a name no command knows is the wire's own refusal (`Opts::new`).
///
/// # Errors
/// `ERR unsupported flow schedule option <key>`, or `Opts::new`'s text.
pub fn options<'a>(name: &str, args: &'a Term, allowed: &[&str]) -> Result<Opts<'a>, FlowError> {
    let opts = Opts::new(name, args, allowed)?;
    if let Term::Map(entries) = args
        && let Some((key, _)) = entries.iter().find(|(key, _)| {
            !allowed
                .iter()
                .any(|known| known.as_bytes() == key.as_slice())
        })
    {
        return Err(rejected(format!(
            "ERR unsupported flow schedule option {}",
            String::from_utf8_lossy(key)
        )));
    }
    Ok(opts)
}

/// The guards of an operator change: the state and version the caller
/// read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Expect {
    /// The state the caller read.
    pub state: Option<Vec<u8>>,
    /// The version the caller read.
    pub version: Option<u64>,
}

impl Expect {
    /// The option names.
    pub const KEYS: [&'static str; 2] = ["expected_state", "expected_version"];

    /// Reads the guards; nil is no guard.
    ///
    /// # Errors
    /// Elixir's text for a guard of the wrong shape.
    pub fn read(opts: &Opts<'_>) -> Result<Self, FlowError> {
        let version = match opts.get("expected_version") {
            None | Some(Term::Nil) => None,
            Some(Term::Int(version)) if *version >= 0 => u64::try_from(*version).ok(),
            Some(_) => {
                return Err(rejected(
                    "ERR flow schedule expected_version must be a non-negative integer",
                ));
            }
        };
        let state = match opts.get("expected_state") {
            None | Some(Term::Nil) => None,
            Some(Term::Bin(state)) if !state.is_empty() => Some(state.clone()),
            Some(_) => {
                return Err(rejected(
                    "ERR flow schedule expected_state must be a non-empty string",
                ));
            }
        };
        Ok(Self { state, version })
    }

    /// Whether any guard was given.
    #[must_use]
    pub const fn any(&self) -> bool {
        self.state.is_some() || self.version.is_some()
    }

    /// Checks the schedule's current `state` and `version`.
    ///
    /// # Errors
    /// `ERR flow schedule changed concurrently` when it moved on.
    pub fn check(&self, state: &[u8], version: u64) -> Result<(), FlowError> {
        let moved = self.version.is_some_and(|expected| expected != version)
            || self
                .state
                .as_deref()
                .is_some_and(|expected| expected != state);
        if moved {
            return Err(rejected("ERR flow schedule changed concurrently"));
        }
        Ok(())
    }
}
