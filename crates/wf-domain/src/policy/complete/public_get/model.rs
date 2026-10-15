//! Original GET routing facts and exact caller diagnoses.

use std::fmt;

use super::PolicySchemaLimits;

/// Prepared original GET arguments, without an executed-read or origin proof.
///
/// The key is owned; type and state borrow the original input independently.
/// This owner does not admit the sum of other requests retained by its caller.
pub struct OriginalPolicyGetRequest<'input> {
    pub(super) policy_key: Vec<u8>,
    pub(super) workflow_type: &'input [u8],
    pub(super) response_state: Option<&'input [u8]>,
    pub(super) profile: PolicySchemaLimits,
}

impl<'input> OriginalPolicyGetRequest<'input> {
    /// Borrows the exact independently owned prefixed allocator key.
    #[must_use]
    pub fn policy_key(&self) -> &[u8] {
        &self.policy_key
    }

    /// Borrows the original input type, independently of this key owner.
    #[must_use]
    pub const fn workflow_type(&self) -> &'input [u8] {
        self.workflow_type
    }

    /// Preserves nil versus every original binary selector, including empty.
    #[must_use]
    pub const fn response_state(&self) -> Option<&'input [u8]> {
        self.response_state
    }

    /// Reads immutable profile facts, without payment or reader authority.
    #[must_use]
    pub const fn schema_limits(&self) -> PolicySchemaLimits {
        self.profile
    }
}

/// Exact original GET validation and supplied-reader diagnoses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalPolicyGetRejection {
    /// The function head or complete keyword shape is invalid.
    Options,
    /// The binary workflow type is empty.
    Type,
    /// The first selector is neither nil nor a binary.
    State,
    /// The complete prefixed key exceeds the original byte bound.
    KeyBytes,
    /// Stored data is nonbinary, undecodable or bound to another type.
    Corrupt,
    /// The supplied policy read originated from an unavailable shard.
    ShardUnavailable,
    /// The supplied policy reader failed or returned an invalid result.
    ReadFailed,
}

impl OriginalPolicyGetRejection {
    /// Borrows the source caller body without constructing an output owner.
    #[must_use]
    pub const fn as_bytes(self) -> &'static [u8] {
        self.text().as_bytes()
    }

    const fn text(self) -> &'static str {
        match self {
            Self::Options => "ERR flow opts must be a keyword list",
            Self::Type => "ERR flow type must be a non-empty string",
            Self::State => "ERR flow state must be a string",
            Self::KeyBytes => "ERR key too large (max 65535 bytes)",
            Self::Corrupt => "ERR flow policy is corrupt",
            Self::ShardUnavailable => "ERR flow policy shard not available",
            Self::ReadFailed => "ERR flow policy read failed",
        }
    }
}

impl fmt::Display for OriginalPolicyGetRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.text())
    }
}

impl std::error::Error for OriginalPolicyGetRejection {}
