//! Reading request options as the Elixir server does (ADR-014).
//!
//! Each reader matches one `Ferricstore.Flow.MutationAttrs` helper: the same
//! defaults, the same accepted values and the same `ERR flow <key> ...` text.
//! An explicit nil is a present value, as in an Elixir keyword list.

use super::FlowError;
use super::term::Term;

/// Largest exact integer the Elixir server accepts for times and counts.
pub const MAX_EXACT: u64 = 9_007_199_254_740_991;

/// The options of one request.
pub struct Opts<'a> {
    entries: &'a [(Vec<u8>, Term)],
}

impl<'a> Opts<'a> {
    /// Reads a request map, refusing keys outside `allowed`.
    pub fn new(command: &str, args: &'a Term, allowed: &[&str]) -> Result<Self, FlowError> {
        let Term::Map(entries) = args else {
            return Err(FlowError::rejected(format!(
                "ERR native {command} payload must be a map"
            )));
        };
        if let Some((key, _)) = entries
            .iter()
            .find(|(key, _)| !allowed.iter().any(|name| name.as_bytes() == key.as_slice()))
        {
            return Err(FlowError::rejected(format!(
                "ERR {command} option {} is not supported by this server",
                String::from_utf8_lossy(key)
            )));
        }
        Ok(Self { entries })
    }

    /// The value under `key`, nil included.
    pub fn get(&self, key: &str) -> Option<&'a Term> {
        self.entries
            .iter()
            .find(|(name, _)| name.as_slice() == key.as_bytes())
            .map(|(_, value)| value)
    }

    /// Whether `key` was sent at all.
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// `required_binary/2`: a non-empty binary.
    pub fn required_binary(&self, key: &str) -> Result<Vec<u8>, FlowError> {
        match self.get(key) {
            Some(Term::Bin(value)) if !value.is_empty() => Ok(value.clone()),
            Some(_) => Err(FlowError::flow(key, "must be a non-empty string")),
            None => Err(FlowError::flow(key, "is required")),
        }
    }

    /// `optional_binary/3`: a non-empty binary, `default` when absent.
    pub fn optional_binary(&self, key: &str, default: &[u8]) -> Result<Vec<u8>, FlowError> {
        match self.get(key) {
            None => Ok(default.to_vec()),
            Some(Term::Bin(value)) if !value.is_empty() => Ok(value.clone()),
            Some(_) => Err(FlowError::flow(key, "must be a non-empty string")),
        }
    }

    /// `optional_binary_or_nil/3` with a nil default.
    pub fn binary_or_nil(&self, key: &str) -> Result<Option<Vec<u8>>, FlowError> {
        match self.get(key) {
            None | Some(Term::Nil) => Ok(None),
            Some(Term::Bin(value)) => Ok(Some(value.clone())),
            Some(_) => Err(FlowError::flow(key, "must be a string")),
        }
    }

    /// `optional_boolean/3`.
    pub fn boolean(&self, key: &str, default: bool) -> Result<bool, FlowError> {
        match self.get(key) {
            None => Ok(default),
            Some(Term::Bool(value)) => Ok(*value),
            Some(_) => Err(FlowError::flow(key, "must be a boolean")),
        }
    }

    /// `optional_non_neg_integer/3`: 0 through [`MAX_EXACT`].
    pub fn non_neg_integer(&self, key: &str) -> Result<Option<u64>, FlowError> {
        match self.get(key) {
            None => Ok(None),
            Some(Term::Int(value)) => match u64::try_from(*value) {
                Ok(value) if value <= MAX_EXACT => Ok(Some(value)),
                Ok(_) => Err(FlowError::flow(
                    key,
                    &format!("exceeds maximum {MAX_EXACT}"),
                )),
                Err(_) => Err(FlowError::flow(key, "must be a non-negative integer")),
            },
            Some(_) => Err(FlowError::flow(key, "must be a non-negative integer")),
        }
    }

    /// `optional_pos_integer`: 1 through [`MAX_EXACT`].
    pub fn positive_integer(&self, key: &str) -> Result<Option<u64>, FlowError> {
        match self.get(key) {
            None => Ok(None),
            Some(Term::Int(value)) => match u64::try_from(*value) {
                Ok(0) | Err(_) => Err(FlowError::flow(key, "must be a positive integer")),
                Ok(value) if value <= MAX_EXACT => Ok(Some(value)),
                Ok(_) => Err(FlowError::flow(
                    key,
                    &format!("exceeds maximum {MAX_EXACT}"),
                )),
            },
            Some(_) => Err(FlowError::flow(key, "must be a positive integer")),
        }
    }

    /// `optional_priority/2`: 0 through 2.
    pub fn priority(&self, default: u8) -> Result<u8, FlowError> {
        match self.get("priority") {
            None => Ok(default),
            Some(Term::Int(value)) if (0..=2).contains(value) => {
                Ok(u8::try_from(*value).unwrap_or(default))
            }
            Some(_) => Err(FlowError::flow("priority", "must be between 0 and 2")),
        }
    }

    /// A list of non-empty binaries, such as value names to read.
    pub fn binary_list(&self, key: &str) -> Result<Vec<Vec<u8>>, FlowError> {
        match self.get(key) {
            None | Some(Term::Nil) => Ok(Vec::new()),
            Some(Term::List(items)) => items
                .iter()
                .map(|item| match item {
                    Term::Bin(name) if !name.is_empty() => Ok(name.clone()),
                    _ => Err(FlowError::flow(key, "must be a list of strings")),
                })
                .collect(),
            Some(Term::Bin(name)) if !name.is_empty() => Ok(vec![name.clone()]),
            Some(_) => Err(FlowError::flow(key, "must be a list of strings")),
        }
    }
}
