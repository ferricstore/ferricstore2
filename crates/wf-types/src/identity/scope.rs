//! Distinct opaque identities for admission, routing and retained outcomes.

use crate::error::ValueError;

/// Nonempty binary tenant isolation identity, bounded to 8,192 bytes.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TenantId(Box<[u8]>);

impl TenantId {
    /// Preserves opaque bytes without text normalization.
    ///
    /// # Errors
    /// Rejects empty bytes or a value above the documented identity bound.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ValueError> {
        validate(&bytes, 8_192)?;
        Ok(Self(bytes.into_boxed_slice()))
    }

    /// Borrows the original validated identity bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Nonempty binary consensus group identity, bounded to 256 bytes.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GroupId(Box<[u8]>);

impl GroupId {
    /// Preserves opaque bytes without text normalization.
    ///
    /// # Errors
    /// Rejects empty bytes or a value above the documented identity bound.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ValueError> {
        validate(&bytes, 256)?;
        Ok(Self(bytes.into_boxed_slice()))
    }

    /// Borrows the original validated identity bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Nonempty binary disaster recovery namespace identity, bounded to 256 bytes.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RecoveryId(Box<[u8]>);

impl RecoveryId {
    /// Preserves opaque bytes without text normalization.
    ///
    /// # Errors
    /// Rejects empty bytes or a value above the documented identity bound.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ValueError> {
        validate(&bytes, 256)?;
        Ok(Self(bytes.into_boxed_slice()))
    }

    /// Borrows the original validated identity bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Nonempty binary client request identity, bounded to 256 bytes.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(Box<[u8]>);

impl RequestId {
    /// Preserves opaque bytes without text normalization.
    ///
    /// # Errors
    /// Rejects empty bytes or a value above the documented identity bound.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ValueError> {
        validate(&bytes, 256)?;
        Ok(Self(bytes.into_boxed_slice()))
    }

    /// Borrows the original validated identity bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Nonempty binary logical partition identity, bounded to 65,535 bytes.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PartitionId(Box<[u8]>);

impl PartitionId {
    /// Preserves opaque bytes without text normalization.
    ///
    /// # Errors
    /// Rejects empty bytes or a value above the documented identity bound.
    pub fn new(bytes: Vec<u8>) -> Result<Self, ValueError> {
        validate(&bytes, 65_535)?;
        Ok(Self(bytes.into_boxed_slice()))
    }

    /// Borrows the original validated identity bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

fn validate(bytes: &[u8], limit: usize) -> Result<(), ValueError> {
    if bytes.is_empty() {
        Err(ValueError::EmptyScopeId)
    } else if bytes.len() > limit {
        Err(ValueError::ScopeIdTooLong {
            bytes: bytes.len(),
            limit,
        })
    } else {
        Ok(())
    }
}
