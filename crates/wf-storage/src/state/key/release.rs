//! Exact workflow-binding points and tenant-scoped release queue identities.
use super::{StateValueError as E, encode_components, key_size, key_start, put_component};
use wf_domain::workflow::{MAX_EXACT_MILLIS as MAX, request::RequestScope};

pub(crate) fn binding(
    scope: &RequestScope,
    partition: &[u8],
    id: &[u8],
    maximum: usize,
) -> Result<Vec<u8>, E> {
    if [partition, id]
        .iter()
        .any(|v| v.is_empty() || v.len() > 65_535)
    {
        return Err(E::InvalidValue);
    }
    encode_components(
        20,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            partition,
            id,
        ],
        maximum,
    )
}
pub(crate) fn queue(
    scope: &RequestScope,
    family: u8,
    sequence: Option<u64>,
    maximum: usize,
) -> Result<Vec<u8>, E> {
    if !matches!((family, sequence), (21, None) | (22 | 23, Some(1..=MAX))) {
        return Err(E::InvalidValue);
    }
    let parts = [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
    ];
    let mut out = key_start(
        family,
        key_size(&parts, if sequence.is_some() { 8 } else { 0 })?,
        maximum,
    )?;
    for part in parts {
        put_component(&mut out, part);
    }
    if let Some(n) = sequence {
        out.extend_from_slice(&n.to_be_bytes());
    }
    Ok(out)
}
pub(super) fn validate(key: &super::DecodedStateKey) -> Result<(), E> {
    if key.part(2).is_empty() || key.part(2).len() > 256 {
        return Err(E::InvalidValue);
    }
    if key.family() == 20
        && [key.part(3), key.part(4)]
            .iter()
            .any(|v| v.is_empty() || v.len() > 65_535)
    {
        return Err(E::InvalidValue);
    }
    if matches!(key.family(), 22 | 23) && !(1..=u128::from(MAX)).contains(&key.number()) {
        return Err(E::InvalidValue);
    }
    Ok(())
}
