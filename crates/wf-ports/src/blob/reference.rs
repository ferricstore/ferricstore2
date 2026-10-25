//! Callable source-reference rejection baselines, without parser/rendering BODY.

use crate::blob::{
    OriginalValueBuildError, OriginalValuePartition, OriginalValueRef, OriginalValueRefKind,
};

/// Inspect the source binary reference grammar without granting availability.
///
/// This new R0 entry always returns `None`; recognition is not implemented yet.
#[must_use]
pub fn parse_original_value_ref(_input: &[u8]) -> Option<OriginalValueRef<'_>> {
    None
}

/// Callable rejection baseline for canonical eligible regular references.
///
/// # Errors
///
/// Always returns [`OriginalValueBuildError::InvalidInput`] in this R0 stage.
pub fn render_original_value_ref(
    _id: &[u8],
    _kind: OriginalValueRefKind,
    _version: u64,
    _partition: OriginalValuePartition<'_>,
    _maximum: usize,
) -> Result<Vec<u8>, OriginalValueBuildError> {
    Err(OriginalValueBuildError::InvalidInput)
}

/// Callable rejection baseline for canonical eligible named shared references.
///
/// # Errors
///
/// Always returns [`OriginalValueBuildError::InvalidInput`] in this R0 stage.
pub fn render_original_named_value_ref(
    _owner: &[u8],
    _name: &[u8],
    _version: u64,
    _partition: OriginalValuePartition<'_>,
    _maximum: usize,
) -> Result<Vec<u8>, OriginalValueBuildError> {
    Err(OriginalValueBuildError::InvalidInput)
}
