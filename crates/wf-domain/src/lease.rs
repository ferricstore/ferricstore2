//! Explicit opaque lease identity and lease-extension inputs.

use wf_types::version::{Fence, TimestampMillis};

use crate::value::{DomainError, InputField};

/// Caller-generated opaque bytes; this crate never generates or interprets them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseToken(Box<[u8]>);

impl LeaseToken {
    /// Checks a nonempty binary token, including non-UTF-8 bytes.
    ///
    /// # Errors
    /// Returns the exact source empty-token rejection for an empty input.
    pub fn new(bytes: Vec<u8>) -> Result<Self, DomainError> {
        if bytes.is_empty() {
            Err(DomainError::EmptyValue(InputField::LeaseToken))
        } else {
            Ok(Self(bytes.into_boxed_slice()))
        }
    }

    /// Borrows the opaque token bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// The exact current lease identity plus fencing generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LeaseGuard {
    /// Caller-observed opaque lease token.
    pub token: LeaseToken,
    /// Caller-observed fencing generation.
    pub fence: Fence,
}

/// An immutable authoritative lease observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lease {
    /// Worker that owns this lease.
    pub worker: Vec<u8>,
    /// Caller-generated opaque token.
    pub token: LeaseToken,
    /// Durable expiration deadline, used to enable explicit reclaim.
    pub deadline: TimestampMillis,
}

/// Extension replaces the deadline with committed time plus duration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtendLease {
    /// Current ownership proof.
    pub guard: LeaseGuard,
    /// Positive lease duration in milliseconds.
    pub lease_ms: u64,
}
