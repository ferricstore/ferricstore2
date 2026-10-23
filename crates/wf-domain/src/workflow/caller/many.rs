//! Source item-shape capture with borrowed IDs, leases and option descriptors.

use std::fmt;

use crate::policy::complete::RawFlowPolicyValue as Raw;

/// One source many-item result; this grants no mutation or admission authority.
#[derive(Debug)]
pub struct ParsedManyItem<'a> {
    /// The caller's original nonempty binary identity.
    pub id: &'a [u8],
    /// A separate required lease for complete, retry and fail results only.
    pub lease_token: Option<&'a [u8]>,
    /// Ordered atom names and borrowed values, including keyword duplicates.
    pub opts: Vec<(&'a str, &'a Raw<'a>)>,
}

/// Source boundary diagnoses, separate from an operational row allocation error.
#[derive(Debug)]
pub enum ManyItemOptionsError {
    /// The source item has no valid nonempty binary identity.
    Id,
    /// The supplied option list is not a keyword list.
    Options,
    /// A required or present optional lease is not a valid nonempty binary.
    LeaseToken,
    /// Reserving the borrowed output rows failed.
    Allocation(std::collections::TryReserveError),
}

impl fmt::Display for ManyItemOptionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id => formatter.write_str("ERR flow id must be a non-empty string"),
            Self::Options => formatter.write_str("ERR flow opts must be a keyword list"),
            Self::LeaseToken => {
                formatter.write_str("ERR flow lease_token must be a non-empty string")
            }
            Self::Allocation(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl std::error::Error for ManyItemOptionsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation(error) => Some(error),
            Self::Id | Self::Options | Self::LeaseToken => None,
        }
    }
}

/// Captures a source create item.
///
/// # Errors
/// Returns the source identity diagnosis in the deliberately rejecting R0 body.
pub fn create<'a>(_item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    Err(ManyItemOptionsError::Id)
}

/// Captures a source complete item.
///
/// # Errors
/// Returns the source identity diagnosis in the deliberately rejecting R0 body.
pub fn complete<'a>(_item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    Err(ManyItemOptionsError::Id)
}

/// Captures a source retry item.
///
/// # Errors
/// Returns the source identity diagnosis in the deliberately rejecting R0 body.
pub fn retry<'a>(_item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    Err(ManyItemOptionsError::Id)
}

/// Captures a source fail item.
///
/// # Errors
/// Returns the source identity diagnosis in the deliberately rejecting R0 body.
pub fn fail<'a>(_item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    Err(ManyItemOptionsError::Id)
}

/// Captures a source cancel item.
///
/// # Errors
/// Returns the source identity diagnosis in the deliberately rejecting R0 body.
pub fn cancel<'a>(_item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    Err(ManyItemOptionsError::Id)
}

/// Captures a source transition item.
///
/// # Errors
/// Returns the source identity diagnosis in the deliberately rejecting R0 body.
pub fn transition<'a>(_item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    Err(ManyItemOptionsError::Id)
}

/// Merges borrowed option rows and pins the caller's partition descriptor.
///
/// # Errors
/// The deliberate R0 body returns an empty result before row reservation exists.
pub fn merge<'a>(
    _base: &[(&'a str, &'a Raw<'a>)],
    _item: &[(&'a str, &'a Raw<'a>)],
    _partition: &'a Raw<'a>,
) -> Result<Vec<(&'a str, &'a Raw<'a>)>, ManyItemOptionsError> {
    Ok(Vec::new())
}
