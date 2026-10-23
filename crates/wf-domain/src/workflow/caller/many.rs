//! Source item-shape capture with borrowed IDs, leases and option descriptors.
//!
//! Inputs require canonical proper-list descriptors and source-unique map keys.
//! Source capture, list fragments and opaque-value correspondence remain caller work.

use std::fmt;

use crate::policy::complete::RawFlowPolicyValue as Raw;

mod map;
mod shape;

#[derive(Clone, Copy)]
enum ManyItemKind {
    Create,
    Complete,
    Retry,
    Fail,
    Cancel,
    Transition,
}

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
/// Returns a source shape or lease diagnosis, or an operational reservation failure.
pub fn create<'a>(item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    shape::parse(item, ManyItemKind::Create)
}

/// Captures a source complete item.
///
/// # Errors
/// Returns a source shape or lease diagnosis, or an operational reservation failure.
pub fn complete<'a>(item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    shape::parse(item, ManyItemKind::Complete)
}

/// Captures a source retry item.
///
/// # Errors
/// Returns a source shape or lease diagnosis, or an operational reservation failure.
pub fn retry<'a>(item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    shape::parse(item, ManyItemKind::Retry)
}

/// Captures a source fail item.
///
/// # Errors
/// Returns a source shape or lease diagnosis, or an operational reservation failure.
pub fn fail<'a>(item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    shape::parse(item, ManyItemKind::Fail)
}

/// Captures a source cancel item.
///
/// # Errors
/// Returns a source shape or lease diagnosis, or an operational reservation failure.
pub fn cancel<'a>(item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    shape::parse(item, ManyItemKind::Cancel)
}

/// Captures a source transition item.
///
/// # Errors
/// Returns a source shape or lease diagnosis, or an operational reservation failure.
pub fn transition<'a>(item: &'a Raw<'a>) -> Result<ParsedManyItem<'a>, ManyItemOptionsError> {
    shape::parse(item, ManyItemKind::Transition)
}

/// Merges borrowed option rows and pins the caller's partition descriptor.
///
/// # Errors
/// Returns the operational error if reserving the merged output rows fails.
pub fn merge<'a>(
    base: &[(&'a str, &'a Raw<'a>)],
    item: &[(&'a str, &'a Raw<'a>)],
    partition: &'a Raw<'a>,
) -> Result<Vec<(&'a str, &'a Raw<'a>)>, ManyItemOptionsError> {
    let mut count = 1;
    for &(key, _) in base {
        if key != "partition_key" && !item.iter().any(|(item_key, _)| *item_key == key) {
            count += 1;
        }
    }
    for &(key, _) in item {
        if key != "partition_key" {
            count += 1;
        }
    }
    let mut rows = reserve_rows(count)?;
    rows.push(("partition_key", partition));
    for &(key, value) in base {
        if key != "partition_key" && !item.iter().any(|(item_key, _)| *item_key == key) {
            rows.push((key, value));
        }
    }
    for &(key, value) in item {
        if key != "partition_key" {
            rows.push((key, value));
        }
    }
    Ok(rows)
}

fn validate_id(bytes: &[u8]) -> Result<(), ManyItemOptionsError> {
    if bytes.is_empty() {
        Err(ManyItemOptionsError::Id)
    } else {
        Ok(())
    }
}

fn required_lease<'a>(value: &Raw<'a>) -> Result<&'a [u8], ManyItemOptionsError> {
    match value {
        Raw::Binary(bytes) if !bytes.is_empty() => Ok(bytes),
        _ => Err(ManyItemOptionsError::LeaseToken),
    }
}

fn optional_lease(value: Option<&Raw<'_>>) -> Result<(), ManyItemOptionsError> {
    match value {
        None | Some(Raw::Nil | Raw::Atom("nil")) => Ok(()),
        Some(value) => required_lease(value).map(|_| ()),
    }
}

fn reserve_rows<'a>(count: usize) -> Result<Vec<(&'a str, &'a Raw<'a>)>, ManyItemOptionsError> {
    let mut rows = Vec::new();
    rows.try_reserve_exact(count)
        .map_err(ManyItemOptionsError::Allocation)?;
    Ok(rows)
}
