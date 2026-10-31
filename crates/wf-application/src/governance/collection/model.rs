//! Bounded collection ownership; no backend handle or current-state certificate.

use std::fmt;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{GovernanceCatalogReadLimits, LoadedGovernanceRecord, StateTransactionError};

/// Complete configured target plus a positive number of globally ranked results.
#[derive(Clone, Copy)]
pub struct GovernanceCatalogCollectionRequest<'a> {
    /// Separately authorized target namespace.
    pub scope: &'a RequestScope,
    /// Original catalog identity.
    pub catalog: &'a [u8],
    /// Result count; does not truncate traversal.
    pub limit: usize,
}

/// Independent read, result and enclosing call logical ownership ceilings.
#[derive(Clone, Copy, Debug)]
pub struct GovernanceCatalogCollectionLimits {
    /// Each coherent catalog read, including its original member owners.
    pub page: GovernanceCatalogReadLimits,
    /// Each configured loader, including its original complete value.
    pub load: GovernanceCatalogReadLimits,
    /// Complete retained result values, independent of count.
    pub maximum_result_bytes: usize,
    /// Result slots, result values, two pages, one loader and both adapters.
    pub workspace_bytes: usize,
}
impl Default for GovernanceCatalogCollectionLimits {
    fn default() -> Self {
        Self {
            page: GovernanceCatalogReadLimits::default(),
            load: GovernanceCatalogReadLimits {
                maximum_bytes: 64 * 1_048_576,
                maximum_temporary_bytes: 32 * 1_048_576,
                ..GovernanceCatalogReadLimits::default()
            },
            maximum_result_bytes: 256 * 1_048_576,
            workspace_bytes: 512 * 1_048_576,
        }
    }
}

/// Original moved values in global result order, with no payload conversion copy.
pub struct GovernanceCatalogCollection<T> {
    pub(super) entries: Vec<LoadedGovernanceRecord<T>>,
    pub(super) owned_bytes: usize,
}
impl<T> GovernanceCatalogCollection<T> {
    /// Complete retained result count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    /// Whether all loaded records were skipped or the catalog was empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    /// Borrows the original globally ranked values.
    pub fn records(&self) -> impl ExactSizeIterator<Item = &T> + DoubleEndedIterator {
        self.entries.iter().map(|entry| &entry.record)
    }
    /// Moves the original values out, without another result allocation.
    pub fn into_records(self) -> impl ExactSizeIterator<Item = T> + DoubleEndedIterator {
        self.entries.into_iter().map(|entry| entry.record)
    }
    /// Complete logical retained record bytes, excluding separately reserved slots.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned_bytes
    }
}

/// Whole-call refusal; no partial collection escapes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GovernanceCatalogCollectionError {
    /// Invalid count, identity or positive resource profile.
    InvalidOptions,
    /// Checked prospective or actual ownership bound failed.
    Limit,
    /// A prospectively admitted host reservation failed.
    Allocation,
    /// Page shape/order/anchor contract was invalid.
    InvalidPage,
    /// Loader ownership shape/charge contract was invalid.
    InvalidLoader,
    /// Original configured read/loader diagnosis.
    Source(StateTransactionError),
}
impl fmt::Display for GovernanceCatalogCollectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions => {
                f.write_str("ERR invalid flow governance catalog collection options")
            }
            Self::Limit => f.write_str("catalog collection ownership limit"),
            Self::Allocation => f.write_str("catalog collection allocation failed"),
            Self::InvalidPage => f.write_str("ERR flow governance catalog is corrupt"),
            Self::InvalidLoader => f.write_str("ERR flow governance catalog loader is invalid"),
            Self::Source(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for GovernanceCatalogCollectionError {}
