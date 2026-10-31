//! Borrowed derived traversal progress; it never grants workflow authority.
use std::fmt;

/// One explicitly scoped derived cursor set/clear command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogProgressChange<'a> {
    /// Original catalog identity, independent of member placement.
    pub catalog: &'a [u8],
    /// None clears; Some(empty) remains an original binary cursor.
    pub cursor: Option<&'a [u8]>,
}
impl CatalogProgressChange<'_> {
    /// Validates original bounded binary fields before effects/ownership.
    /// # Errors
    /// Refuses invalid caps and oversized fields.
    pub fn validate(self, maximum_key_bytes: usize) -> Result<(), CatalogProgressError> {
        if maximum_key_bytes == 0
            || self.catalog.len() > maximum_key_bytes
            || self
                .cursor
                .is_some_and(|cursor| cursor.len() > maximum_key_bytes)
        {
            return Err(CatalogProgressError);
        }
        Ok(())
    }
}
/// Invalid progress fields, distinct from an operational backend failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CatalogProgressError;
impl fmt::Display for CatalogProgressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ERR invalid flow limit cleanup cursor")
    }
}
impl std::error::Error for CatalogProgressError {}
