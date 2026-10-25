//! Pure source value-reference and logical-binary preparation.
//!
//! Borrowed shapes and prepared bytes grant no storage, placement, publication
//! or admission authority. Logical binaries are never implicitly typed ETF.

mod encoding;
mod publication;
mod reference;

pub use encoding::{original_binary_stored_len, original_binary_stored_parts};
pub use publication::{
    RecordBatchValueEffect, RecordBatchValueEffects, RecordBatchValueEffectsError,
    inspect_record_batch_value_effects,
};
pub use reference::{
    parse_original_value_ref, render_original_named_value_ref, render_original_value_ref,
};

/// A regular source value-reference family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalValueRefKind {
    /// Workflow payload bytes.
    Payload,
    /// Workflow result bytes.
    Result,
    /// Workflow error bytes.
    Error,
    /// Regular shared bytes, distinct from a named shared reference.
    Shared,
}

/// Source partition selection as ordinary data, without placement authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalValuePartition<'a> {
    /// Source omitted/nil partition: automatic bucket from the workflow owner ID.
    Automatic,
    /// Explicit source global partition.
    Global,
    /// Opaque partition bytes, including the source automatic-key spelling.
    Explicit(&'a [u8]),
}

/// A borrowed source-shaped reference, without durable availability authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalValueRef<'a> {
    /// A regular p/r/e/s reference with an opaque nonempty ID.
    Regular {
        /// Complete source `f:{...}` prefix, including its closing brace.
        tag_prefix: &'a [u8],
        /// The regular source value family.
        kind: OriginalValueRefKind,
        /// Original ID bytes; colons, NUL and non-UTF8 bytes are meaningful.
        id: &'a [u8],
        /// Canonical nonnegative mathematical version.
        version: u64,
    },
    /// A named shared reference with original encoded component spans.
    Named {
        /// Complete source `f:{...}` prefix, including its closing brace.
        tag_prefix: &'a [u8],
        /// Original canonical unpadded base64url owner component, not decoded bytes.
        owner_component: &'a [u8],
        /// Original canonical unpadded base64url name component, not decoded bytes.
        name_component: &'a [u8],
        /// Canonical nonnegative mathematical version.
        version: u64,
    },
}

/// Semantic input failure is distinct from output limits and actual allocation.
#[derive(Debug)]
pub enum OriginalValueBuildError {
    /// The supplied components are outside canonical eligible construction.
    InvalidInput,
    /// Checked arithmetic or a caller/source output ceiling was exceeded.
    Limit,
    /// A real fallible reservation failed.
    ReserveFailed(std::collections::TryReserveError),
}
