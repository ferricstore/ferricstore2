//! Closed capture field identity and one moved existing-value owner.
use crate::value::{
    OriginalFlowValueError, OriginalValueDocument, OriginalValueEncodeError, OriginalValueNode,
    PreparedOriginalFlowValue,
};
use std::fmt;

/// Original optional fields; no current value or provenance is inferred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalRecordMetadataField {
    /// Original logical partition, including explicit nil and empty binary.
    Partition,
    /// Original source scope prefix.
    ScopePrefix,
    /// Original parent identity.
    Parent,
    /// Original root identity.
    Root,
    /// Original correlation identity.
    Correlation,
    /// Original attributes value.
    Attributes,
    /// Original per-state metadata value.
    StateMeta,
    /// Original indexed attribute configuration.
    IndexedAttributes,
    /// Original indexed state metadata configuration.
    IndexedStateMeta,
    /// Original captured active duration.
    MaxActive,
    /// Original complete history cap.
    HistoryMax,
    /// Original hot history cap.
    HistoryHotMax,
}
impl OriginalRecordMetadataField {
    pub(super) const fn index(self) -> usize {
        self as usize
    }
    /// Exact binary key in this private versioned capture schema.
    #[must_use]
    pub const fn as_bytes(self) -> &'static [u8] {
        match self {
            Self::Partition => b"partition_key",
            Self::ScopePrefix => b"scope_prefix",
            Self::Parent => b"parent_flow_id",
            Self::Root => b"root_flow_id",
            Self::Correlation => b"correlation_id",
            Self::Attributes => b"attributes",
            Self::StateMeta => b"state_meta",
            Self::IndexedAttributes => b"indexed_attributes",
            Self::IndexedStateMeta => b"indexed_state_meta",
            Self::MaxActive => b"max_active_ms",
            Self::HistoryMax => b"history_max_events",
            Self::HistoryHotMax => b"history_hot_max_events",
        }
    }
    pub(super) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        [
            Self::Partition,
            Self::ScopePrefix,
            Self::Parent,
            Self::Root,
            Self::Correlation,
            Self::Attributes,
            Self::StateMeta,
            Self::IndexedAttributes,
            Self::IndexedStateMeta,
            Self::MaxActive,
            Self::HistoryMax,
            Self::HistoryHotMax,
        ]
        .into_iter()
        .find(|field| field.as_bytes() == bytes)
    }
}

/// One prepared data owner; publication and source validation remain external.
pub struct PreparedOriginalRecordMetadata {
    pub(super) value: PreparedOriginalFlowValue,
    pub(super) work: usize,
    pub(super) temporary: usize,
}
impl PreparedOriginalRecordMetadata {
    /// Borrows the complete private capture envelope.
    #[must_use]
    pub fn as_stored_bytes(&self) -> &[u8] {
        self.value.as_stored_bytes()
    }
    /// Moves the same complete byte allocation.
    #[must_use]
    pub fn into_stored_bytes(self) -> Vec<u8> {
        self.value.into_stored_bytes()
    }
    /// Continuous schema and codec work.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }
    /// Conservative simultaneous requested logical owners, not allocator/RSS.
    #[must_use]
    pub const fn temporary_bytes(&self) -> usize {
        self.temporary
    }
}

/// An immutable owned metadata document; no source or storage authority.
pub struct OriginalRecordMetadata {
    pub(super) document: OriginalValueDocument,
    pub(super) fields: [Option<usize>; 12],
    pub(super) work: usize,
}
impl OriginalRecordMetadata {
    /// Borrow a field root in the same verified arena, without cloning its data.
    #[must_use]
    pub fn field_input(
        &self,
        field: OriginalRecordMetadataField,
    ) -> Option<crate::value::OriginalValueInput<'_>> {
        let mut input = self.document.as_input();
        input.root = self.fields[field.index()]?;
        Some(input)
    }
    /// Requested retained descriptor bytes, excluding the unchanged input backing.
    #[must_use]
    pub fn descriptor_bytes(&self) -> Option<usize> {
        let input = self.document.as_input();
        input
            .nodes
            .len()
            .checked_mul(std::mem::size_of::<OriginalValueNode>())?
            .checked_add(
                input
                    .edges
                    .len()
                    .checked_mul(std::mem::size_of::<usize>())?,
            )
    }
    /// Borrows a field node; missing differs from explicit atom nil or empty data.
    #[must_use]
    pub fn field(&self, field: OriginalRecordMetadataField) -> Option<&OriginalValueNode> {
        self.fields[field.index()].and_then(|index| self.document.node(index))
    }
    /// Borrows the same complete backing and value spines.
    #[must_use]
    pub const fn document(&self) -> &OriginalValueDocument {
        &self.document
    }
    /// Moves the unchanged original complete encoded allocation.
    #[must_use]
    pub fn into_stored_bytes(self) -> Vec<u8> {
        self.document.into_encoded()
    }
    /// Actual continuous shared decoder and schema work.
    #[must_use]
    pub const fn work_units(&self) -> usize {
        self.work
    }
}
impl fmt::Debug for PreparedOriginalRecordMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PreparedOriginalRecordMetadata(<redacted>)")
    }
}
impl fmt::Debug for OriginalRecordMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OriginalRecordMetadata(<redacted>)")
    }
}

/// Schema errors stay distinct from actual context, resource and codec failures.
#[derive(Debug)]
pub enum OriginalRecordMetadataError {
    /// Envelope, field name, duplicate or graph reference is malformed.
    InvalidSchema,
    /// The capture's declared schema version is unsupported.
    UnsupportedVersion,
    /// An independent required operational limit is zero.
    InvalidLimits,
    /// Continuous work, bytes or logical control ownership exceeds its ceiling.
    Limit,
    /// Existing logical preparation failure, without reclassification.
    Encoding(OriginalValueEncodeError),
    /// Existing decoding/context failure, without reclassification.
    Decoding(OriginalFlowValueError),
}
impl fmt::Display for OriginalRecordMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSchema => f.write_str("invalid original record metadata capture"),
            Self::UnsupportedVersion => {
                f.write_str("unsupported original record metadata capture version")
            }
            Self::InvalidLimits => f.write_str("invalid record metadata limits"),
            Self::Limit => f.write_str("record metadata resource limit"),
            Self::Encoding(error) => error.fmt(f),
            Self::Decoding(error) => write!(f, "original value decode: {error:?}"),
        }
    }
}
impl std::error::Error for OriginalRecordMetadataError {}
