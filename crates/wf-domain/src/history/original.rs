//! Borrowed flat history views from the original pure helper.
//!
//! Capture and proper-list acquisition belong to the caller. These views do not
//! decode terms, flatten aliases, validate event IDs or authorize history reads.
//! Opaque descriptors retain no hidden-payload correspondence proof.

use std::collections::BTreeMap;

use crate::policy::complete::RawFlowPolicyValue as Raw;

/// Already captured tuple or flat proper-list entry, with borrowed backing.
#[derive(Clone, Copy, Debug)]
pub enum OriginalHistoryInput<'a> {
    /// A tuple whose second element was captured as a proper field list.
    Tuple {
        /// Any original event-ID descriptor.
        event_id: &'a Raw<'a>,
        /// Flat field positions, including invalid keys or a dangling position.
        fields: &'a [Raw<'a>],
    },
    /// The first item is the event ID; the entire remaining flat tail is fields.
    List(&'a [Raw<'a>]),
}

/// An owned map of borrowed descriptors, distinct from mutation history state.
///
/// Sorted iteration is a Rust representation choice. Equal byte keys may retain
/// their first key backing; the last value descriptor is the selected value.
/// Standard B-tree nodes allocate infallibly through the global allocator.
/// Caller-owned capture, bounds and allocation failure remain caller concerns;
/// this view provides no paid admission or total-allocation guarantee.
#[derive(Debug)]
pub struct RawHistoryEntry<'a> {
    /// The actual original event-ID descriptor, without format validation.
    pub event_id: &'a Raw<'a>,
    /// Binary key bytes and actual value descriptors remain borrowed.
    pub fields: BTreeMap<&'a [u8], &'a Raw<'a>>,
}

/// Callable empty-map baseline before the source-derived regression body.
#[must_use]
pub fn fields_to_map<'a>(_fields: &'a [Raw<'a>]) -> BTreeMap<&'a [u8], &'a Raw<'a>> {
    BTreeMap::new()
}

/// Callable absent-entry baseline before the source-derived regression body.
///
/// The implemented boundary uses `None` only for an empty captured list. That
/// Rust sentinel represents an unmatched source clause, not returned Elixir nil.
#[must_use]
pub fn to_tuple(_entry: OriginalHistoryInput<'_>) -> Option<RawHistoryEntry<'_>> {
    None
}
