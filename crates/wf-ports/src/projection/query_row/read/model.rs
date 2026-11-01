//! Borrowed decoded data and measured results; these grant no native authority.
use crate::projection::query_row::I;
use std::borrow::Cow;
use wf_domain::metadata::OriginalAttributeValue;

/// Positive independent limits for borrowed reads and transformations.
#[derive(Clone, Copy, Debug)]
pub struct OriginalQueryReadLimits {
    /// Maximum combined encoded-row and state-key input bytes.
    pub maximum_input_bytes: usize,
    /// Maximum emitted bytes for relocation or physical-partition transformation.
    pub maximum_output_bytes: usize,
    /// Maximum continuous validation, selection and checksum work.
    pub maximum_work_units: usize,
    /// Conservative requested owners/control, excluding borrowed caller donors.
    pub maximum_temporary_bytes: usize,
}

/// Source none/all or a captured unique nonempty `MapSet` of binary names.
#[derive(Clone, Copy, Debug)]
pub enum OriginalQuerySelection<'a> {
    /// Omit the dynamic section.
    None,
    /// Materialize its complete nested map.
    All,
    /// Materialize only these unique named tuple fields.
    Names(&'a [&'a [u8]]),
}
/// Source none/all or a captured unique map of nonempty name sets.
#[derive(Clone, Copy, Debug)]
pub enum OriginalQueryStateSelection<'a> {
    /// Omit state metadata.
    None,
    /// Materialize its complete nested map.
    All,
    /// Unique state keys and unique names in each nonempty set.
    Names(&'a [(&'a [u8], &'a [&'a [u8]])]),
}
/// Original selected public-metadata read plan as ordinary captured data.
#[derive(Clone, Copy, Debug)]
pub struct OriginalQueryDecodePlan<'a> {
    /// Attribute selection.
    pub attributes: OriginalQuerySelection<'a>,
    /// State-metadata selection.
    pub state_meta: OriginalQueryStateSelection<'a>,
}

/// A decoded row or source expiry, with continuous accounting in either case.
pub enum OriginalQueryRead<T> {
    /// Successfully decoded ordinary data.
    Row {
        /// The returned data, without native availability/currentness proof.
        value: T,
        /// Continuous charged work.
        work_units: usize,
        /// Conservative requested logical peak, not allocator/RSS measurement.
        temporary_bytes: usize,
    },
    /// Source expiry after validation required by this read mode.
    Expired {
        /// Continuous charged work, including validation before expiry.
        work_units: usize,
        /// Conservative requested logical peak.
        temporary_bytes: usize,
    },
}
impl<T> OriginalQueryRead<T> {
    /// Move decoded data; None specifically means source expiry, never missing.
    #[must_use]
    pub fn into_row(self) -> Option<T> {
        match self {
            Self::Row { value, .. } => Some(value),
            Self::Expired { .. } => None,
        }
    }
    /// Whether this is the source expired result.
    #[must_use]
    pub const fn is_expired(&self) -> bool {
        matches!(self, Self::Expired { .. })
    }
    /// Continuous charged work on either outcome.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        match self {
            Self::Row { work_units, .. } | Self::Expired { work_units, .. } => *work_units,
        }
    }
    /// Conservative requested logical peak on either outcome.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        match self {
            Self::Row {
                temporary_bytes, ..
            }
            | Self::Expired {
                temporary_bytes, ..
            } => *temporary_bytes,
        }
    }
}
impl<T> std::fmt::Debug for OriginalQueryRead<T> {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("OriginalQueryRead(<redacted>)")
    }
}

/// Ordinary state-location address data; state kind is fixed by this profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginalQueryLocation<'a> {
    /// Original workflow ID.
    pub flow_id: &'a [u8],
    /// Original logical version.
    pub version: u64,
    /// Original committed generation index.
    pub raft_index: u64,
    /// Source file-family tag0..3, without runtime availability proof.
    pub source_tag: u8,
    /// Original source file/segment ID.
    pub source_id: u64,
    /// Physical byte offset.
    pub offset: u64,
    /// Positive stored value size.
    pub value_size: u64,
    /// Optional complete physical frame size.
    pub frame_size: Option<u64>,
    /// Advertised original checksum bytes, not a verified body digest.
    pub checksum: &'a [u8],
    /// Original source lifetime, independent of row retention.
    pub expire_at_ms: Option<u64>,
    /// Optional segment generation/ordinal.
    pub segment_generation: Option<u64>,
}

pub(super) type Configuration<'a> = (Vec<&'a [u8]>, Option<&'a [u8]>);

pub(super) type Entries<'a> = Vec<(&'a [u8], OriginalAttributeValue<'a>)>;
pub(super) type States<'a> = Vec<(&'a [u8], Entries<'a>)>;

/// Original public record shape, borrowing binary values from caller donors.
pub struct OriginalQueryRecord<'a> {
    pub(super) fields: [Option<I<'a, 'a>>; 20],
    pub(super) attributes: Entries<'a>,
    pub(super) states: States<'a>,
    pub(super) attributes_nested: bool,
    pub(super) states_nested: bool,
    pub(super) indexed_attributes: Vec<&'a [u8]>,
    pub(super) indexed_state: Option<&'a [u8]>,
}
impl<'a> OriginalQueryRecord<'a> {
    /// A scalar original builtin/internal field; missing differs from nil.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<I<'a, 'a>> {
        let index = super::super::record::NAMES
            .iter()
            .position(|n| *n == name)?;
        self.fields.get(index).copied().flatten()
    }
    /// The nested attributes field, omitted when empty or tuple-selected.
    #[must_use]
    pub fn attributes(&self) -> Option<&[(&'a [u8], OriginalAttributeValue<'a>)]> {
        (self.attributes_nested && !self.attributes.is_empty())
            .then_some(self.attributes.as_slice())
    }
    /// The nested state-metadata field, omitted when empty or tuple-selected.
    #[must_use]
    pub fn state_meta(&self) -> Option<&[(&'a [u8], Entries<'a>)]> {
        (self.states_nested && !self.states.is_empty()).then_some(self.states.as_slice())
    }
    /// Source Field-style attribute lookup, including nested or selected fields.
    #[must_use]
    pub fn attribute(&self, name: &[u8]) -> Option<&OriginalAttributeValue<'a>> {
        self.attributes.iter().find(|r| r.0 == name).map(|r| &r.1)
    }
    /// Source tuple-key map lookup; all/nested mode has no such tuple field.
    #[must_use]
    pub fn selected_attribute(&self, name: &[u8]) -> Option<&OriginalAttributeValue<'a>> {
        if self.attributes_nested {
            None
        } else {
            self.attribute(name)
        }
    }
    /// Source Field-style state metadata lookup.
    #[must_use]
    pub fn state_field(&self, state: &[u8], name: &[u8]) -> Option<&OriginalAttributeValue<'a>> {
        self.states
            .iter()
            .find(|r| r.0 == state)?
            .1
            .iter()
            .find(|r| r.0 == name)
            .map(|r| &r.1)
    }
    /// Source tuple-key state lookup; nested mode has no such tuple field.
    #[must_use]
    pub fn selected_state_field(
        &self,
        state: &[u8],
        name: &[u8],
    ) -> Option<&OriginalAttributeValue<'a>> {
        if self.states_nested {
            None
        } else {
            self.state_field(state, name)
        }
    }
    /// Full-read indexed names, omitted for query mode or empty configuration.
    #[must_use]
    pub fn indexed_attributes(&self) -> Option<&[&'a [u8]]> {
        (!self.indexed_attributes.is_empty()).then_some(self.indexed_attributes.as_slice())
    }
    /// Full-read indexed state key, omitted for query mode or empty configuration.
    #[must_use]
    pub const fn indexed_state_meta(&self) -> Option<&'a [u8]> {
        self.indexed_state
    }
    /// Count of present scalar, nested, selected and configuration fields.
    #[must_use]
    pub fn field_count(&self) -> usize {
        self.fields.iter().flatten().count()
            + usize::from(self.attributes().is_some())
            + usize::from(self.state_meta().is_some())
            + if self.attributes_nested {
                0
            } else {
                self.attributes.len()
            }
            + if self.states_nested {
                0
            } else {
                self.states.iter().map(|s| s.1.len()).sum()
            }
            + usize::from(!self.indexed_attributes.is_empty())
            + usize::from(self.indexed_state.is_some())
    }
}

/// Scope state is explicit: reference omission is not an unscoped record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalQueryScope<'a> {
    /// Reference mode did not interpret the scope.
    NotRead,
    /// Full/query mode validated an empty original scope.
    Unscoped,
    /// Full/query mode validated these opaque scope bytes and key association.
    Scoped(&'a [u8]),
}

/// Borrowed row/header data; reference mode has no record or exposed scope.
pub struct DecodedOriginalQueryRow<'a> {
    pub(super) encoded: &'a [u8],
    pub(super) key: &'a [u8],
    pub(super) id: &'a [u8],
    pub(super) version: u64,
    pub(super) expiry: u64,
    pub(super) location: OriginalQueryLocation<'a>,
    pub(super) scope: OriginalQueryScope<'a>,
    pub(super) record: Option<OriginalQueryRecord<'a>>,
    pub(super) sections: [&'a [u8]; 5],
}
impl<'a> DecodedOriginalQueryRow<'a> {
    /// Original state-key donor, without copying.
    #[must_use]
    pub const fn state_key(&self) -> &'a [u8] {
        self.key
    }
    /// Original identity derived from the state key.
    #[must_use]
    pub const fn flow_id(&self) -> &'a [u8] {
        self.id
    }
    /// Original logical version.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
    /// Original row retention expiry.
    #[must_use]
    pub const fn expire_at_ms(&self) -> u64 {
        self.expiry
    }
    /// Ordinary decoded physical state-location data.
    #[must_use]
    pub const fn locator(&self) -> &OriginalQueryLocation<'a> {
        &self.location
    }
    /// Reference omission, validated unscoped data, or opaque scoped bytes.
    #[must_use]
    pub const fn scope_prefix(&self) -> OriginalQueryScope<'a> {
        self.scope
    }
    /// Materialized original record; absent specifically in reference mode.
    #[must_use]
    pub const fn record(&self) -> Option<&OriginalQueryRecord<'a>> {
        self.record.as_ref()
    }
}

/// Borrowed original internal-record view with two explicit overrides.
pub struct OriginalQueryInternalRecord<'r, 'a> {
    pub(super) record: &'r OriginalQueryRecord<'a>,
    pub(super) partition: Option<Cow<'a, [u8]>>,
    pub(super) retention: Option<u64>,
}
impl OriginalQueryInternalRecord<'_, '_> {
    /// Scalar lookup with original physical partition and retention restored.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<I<'_, '_>> {
        if name == "partition_key" {
            return Some(self.partition.as_deref().map_or(I::Nil, I::Binary));
        }
        if name == "terminal_retention_until_ms" {
            return self.retention.map(|v| I::Integer(i128::from(v)));
        }
        self.record.field(name)
    }
    /// Borrow the original public record for all other metadata lookups.
    #[must_use]
    pub const fn public_record(&self) -> &OriginalQueryRecord<'_> {
        self.record
    }
}
