//! Orthogonal catalog identity; owner/member bytes never imply placement.

use super::{StateValueError as E, encode_components};
use wf_domain::workflow::request::RequestScope;

pub(crate) fn prefix(scope: &RequestScope, catalog: &[u8], maximum: usize) -> Result<Vec<u8>, E> {
    if catalog.len() > 65_535 {
        return Err(E::InvalidValue);
    }
    encode_components(
        18,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            catalog,
        ],
        maximum,
    )
}
pub(crate) fn member(
    scope: &RequestScope,
    catalog: &[u8],
    member: &[u8],
    maximum: usize,
) -> Result<Vec<u8>, E> {
    if catalog.len() > 65_535 || member.len() > 65_535 {
        return Err(E::InvalidValue);
    }
    encode_components(
        18,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            catalog,
            member,
        ],
        maximum,
    )
}
pub(super) fn validate(key: &super::DecodedStateKey) -> Result<(), E> {
    if key.part(2).is_empty()
        || key.part(2).len() > 256
        || key.part(3).len() > 65_535
        || key.part(4).is_empty()
        || key.part(4).len() > 65_535
    {
        return Err(E::InvalidValue);
    }
    Ok(())
}

pub(crate) fn progress(scope: &RequestScope, catalog: &[u8], maximum: usize) -> Result<Vec<u8>, E> {
    if catalog.len() > 65_535 {
        return Err(E::InvalidValue);
    }
    encode_components(
        19,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            catalog,
        ],
        maximum,
    )
}
pub(super) fn validate_progress(key: &super::DecodedStateKey) -> Result<(), E> {
    if key.part(2).is_empty() || key.part(2).len() > 256 || key.part(3).len() > 65_535 {
        return Err(E::InvalidValue);
    }
    Ok(())
}
