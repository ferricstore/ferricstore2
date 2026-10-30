//! Exact private owner point; key construction grants no state authority.

use super::{StateValueError, encode_components, key_size, key_start, put_component};
use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;
use wf_domain::workflow::request::RequestScope;

pub(crate) fn encode_credit_owner_key(
    scope: &RequestScope,
    credit_scope: &[u8],
    maximum: usize,
) -> Result<Vec<u8>, StateValueError> {
    if credit_scope.is_empty() || credit_scope.len() > 65_535 {
        return Err(StateValueError::InvalidValue);
    }
    encode_components(
        12,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            credit_scope,
        ],
        maximum,
    )
}

pub(crate) fn encode_credit_reservation_key(
    scope: &RequestScope,
    credit_scope: &[u8],
    shard: u64,
    epoch: u64,
    id: &[u8],
    maximum: usize,
) -> Result<Vec<u8>, StateValueError> {
    validate(credit_scope, shard, epoch)?;
    if id.is_empty() || id.len() > 256 {
        return Err(StateValueError::InvalidValue);
    }
    encode_components(
        13,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            credit_scope,
            &shard.to_be_bytes(),
            &epoch.to_be_bytes(),
            id,
        ],
        maximum,
    )
}

pub(crate) fn encode_credit_page_key(
    scope: &RequestScope,
    credit_scope: &[u8],
    shard: u64,
    epoch: u64,
    page: u16,
    maximum: usize,
) -> Result<Vec<u8>, StateValueError> {
    validate(credit_scope, shard, epoch)?;
    if !(1..=256).contains(&page) {
        return Err(StateValueError::InvalidValue);
    }
    numeric(
        14,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            credit_scope,
            &shard.to_be_bytes(),
            &epoch.to_be_bytes(),
        ],
        u64::from(page),
        maximum,
    )
}

pub(crate) fn encode_credit_cleanup_key(
    scope: &RequestScope,
    credit_scope: &[u8],
    sequence: u64,
    maximum: usize,
) -> Result<Vec<u8>, StateValueError> {
    validate(credit_scope, 0, 1)?;
    if !(1..=MAX).contains(&sequence) {
        return Err(StateValueError::InvalidValue);
    }
    numeric(
        15,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            credit_scope,
        ],
        sequence,
        maximum,
    )
}

fn numeric(
    family: u8,
    fields: &[&[u8]],
    number: u64,
    maximum: usize,
) -> Result<Vec<u8>, StateValueError> {
    let mut out = key_start(family, key_size(fields, 8)?, maximum)?;
    for field in fields {
        put_component(&mut out, field);
    }
    out.extend_from_slice(&number.to_be_bytes());
    Ok(out)
}

fn validate(scope: &[u8], shard: u64, epoch: u64) -> Result<(), StateValueError> {
    if scope.is_empty() || scope.len() > 65_535 || shard > MAX || !(1..=MAX).contains(&epoch) {
        return Err(StateValueError::InvalidValue);
    }
    Ok(())
}

pub(super) fn validate_decoded(key: &super::DecodedStateKey) -> Result<(), StateValueError> {
    let family = key.family();
    if key.part(2).is_empty() || key.part(2).len() > 256 {
        return Err(StateValueError::InvalidValue);
    }
    let (shard, epoch) = if matches!(family, 13 | 14) {
        (scalar(key.part(4))?, scalar(key.part(5))?)
    } else {
        (0, 1)
    };
    validate(key.part(3), shard, epoch)?;
    if (family == 13 && (key.part(6).is_empty() || key.part(6).len() > 256))
        || (family == 14 && !(1..=256).contains(&key.number()))
        || (family == 15 && !(1..=u128::from(MAX)).contains(&key.number()))
    {
        return Err(StateValueError::InvalidValue);
    }
    Ok(())
}

fn scalar(bytes: &[u8]) -> Result<u64, StateValueError> {
    Ok(u64::from_be_bytes(
        bytes
            .try_into()
            .map_err(|_| StateValueError::InvalidValue)?,
    ))
}
