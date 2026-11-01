//! Borrowed original operands and explicit cumulative preparation ceilings.
use crate::log::CodecLimits;

/// One verified stored9 frame plus optional opaque metadata in source order.
#[derive(Clone, Copy)]
pub struct MetadataBatchInput<'a> {
    /// Complete actually encoded stored9 frame, including its footer.
    pub stored_batch: &'a [u8],
    /// Original caller operands; None carries actual prior captured metadata.
    /// A successful birth requires Some; Some(empty bytes) is invalid.
    pub metadata: &'a [Option<&'a [u8]>],
}

/// The nested preparation and metadata copies share these explicit accounts.
#[derive(Clone, Copy)]
pub struct MetadataBatchLimits {
    /// Existing replicated frame/item/value profile.
    pub codec: CodecLimits,
    /// Simultaneously requested complete logical ownership, including slots.
    pub maximum_owned_bytes: usize,
    /// Cumulative outer and nested codec work.
    pub maximum_work_units: usize,
}
