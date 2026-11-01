//! Owned routing bytes plus borrowed normalized original caller data.
use super::super::OriginalQueryRecordError;
use std::{borrow::Cow, fmt};
use wf_domain::{
    metadata::OriginalMetadataInput as I,
    workflow::caller::create::{OriginalCreateCaller, OriginalCreateError},
};

/// Ordinary normalized source attributes and their admitted original state key.
pub struct PreparedOriginalCreate<'r, 'a> {
    pub(super) caller: OriginalCreateCaller<'r, 'a>,
    pub(super) partition: Cow<'a, [u8]>,
    pub(super) key: Vec<u8>,
    pub(super) metadata_bytes: usize,
    pub(super) owned: usize,
}
impl<'r, 'a> PreparedOriginalCreate<'r, 'a> {
    /// Complete normalized scalar/opaque source field, including resolved partition.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<I<'_, '_>> {
        if name == "partition_key" {
            Some(I::Binary(&self.partition))
        } else {
            self.caller.field(name)
        }
    }
    /// All normalized borrowed caller data and bounded dynamic spines.
    #[must_use]
    pub const fn caller(&self) -> &OriginalCreateCaller<'r, 'a> {
        &self.caller
    }
    /// Complete original attribute count, including resolved automatic partition.
    #[must_use]
    pub fn field_count(&self) -> usize {
        self.caller.field_count_without_partition() + 1
    }
    /// Exact original source state key, without physical native availability authority.
    #[must_use]
    pub fn state_key(&self) -> &[u8] {
        &self.key
    }
    /// Measured admission metadata bytes; reserved index names are not installed policy.
    #[must_use]
    pub const fn admission_metadata_bytes(&self) -> usize {
        self.metadata_bytes
    }
    /// Conservative requested final ownership, excluding borrowed leaf backing.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned
    }
}
impl fmt::Debug for PreparedOriginalCreate<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreparedOriginalCreate(<redacted>)")
    }
}

/// Operational normalization or original codec failures; never business rejection.
#[derive(Debug)]
pub enum OriginalCreatePreparationError {
    /// Domain capture or resource refusal.
    Create(OriginalCreateError),
    /// Query resource/codec failure outside original source invalid/size classes.
    Query(OriginalQueryRecordError),
}
impl fmt::Display for OriginalCreatePreparationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Create(e) => e.fmt(f),
            Self::Query(e) => fmt::Debug::fmt(e, f),
        }
    }
}
impl std::error::Error for OriginalCreatePreparationError {}
