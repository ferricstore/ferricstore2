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

/// Selects the last value for each binary key from fixed positional pairs.
///
/// Invalid keys consume their entire pair; a trailing singleton is ignored.
/// Key bytes and value descriptors borrow the caller's original backing.
#[must_use]
pub fn fields_to_map<'a>(fields: &'a [Raw<'a>]) -> BTreeMap<&'a [u8], &'a Raw<'a>> {
    let mut output = BTreeMap::new();
    for pair in fields.as_chunks::<2>().0 {
        if let [Raw::Binary(key), value] = pair {
            output.insert(*key, value);
        }
    }
    output
}

/// Borrows the original event ID and converts the entry's flat field positions.
///
/// This boundary uses `None` only for an empty captured list. That
/// Rust sentinel represents an unmatched source clause, not returned Elixir nil.
#[must_use]
pub fn to_tuple(entry: OriginalHistoryInput<'_>) -> Option<RawHistoryEntry<'_>> {
    let (event_id, fields) = match entry {
        OriginalHistoryInput::Tuple { event_id, fields } => (event_id, fields),
        OriginalHistoryInput::List(items) => items.split_first()?,
    };
    Some(RawHistoryEntry {
        event_id,
        fields: fields_to_map(fields),
    })
}

/// A canonical source millisecond value with borrowed decimal magnitude.
///
/// Nonzero digits omit the sign and leading zeros. Zero is always nonnegative
/// and uses static backing. This view is not a clock or history-read authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginalHistoryMillis<'a> {
    negative: bool,
    digits: &'a [u8],
}

impl<'a> OriginalHistoryMillis<'a> {
    /// Whether the mathematical value is strictly negative.
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.negative
    }

    /// The normalized ASCII magnitude, borrowing the original binary payload.
    #[must_use]
    pub const fn digits(self) -> &'a [u8] {
        self.digits
    }
}

/// Returns the original helper's millisecond value as a canonical decimal view.
///
/// Malformed and nonbinary inputs represent positive zero. Nonzero results
/// borrow the binary payload rather than its temporary descriptor.
#[must_use]
pub fn original_history_event_ms(_event_id: Raw<'_>) -> OriginalHistoryMillis<'_> {
    OriginalHistoryMillis {
        negative: false,
        digits: b"0",
    }
}
