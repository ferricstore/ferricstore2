//! Publication keys bind actual group/recovery/tenant, never lease shard IDs.

use super::{StateValueError as E, encode_components, key_size, key_start, put_component};
use wf_domain::workflow::{MAX_EXACT_MILLIS as MAX, request::RequestScope};

pub(crate) fn meta(scope: &RequestScope, maximum: usize) -> Result<Vec<u8>, E> {
    encode_components(
        16,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
        ],
        maximum,
    )
}

pub(crate) fn intent(scope: &RequestScope, sequence: u64, maximum: usize) -> Result<Vec<u8>, E> {
    if !(1..=MAX).contains(&sequence) {
        return Err(E::InvalidValue);
    }
    let fields = [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
    ];
    let mut out = key_start(17, key_size(&fields, 8)?, maximum)?;
    for field in fields {
        put_component(&mut out, field);
    }
    out.extend_from_slice(&sequence.to_be_bytes());
    Ok(out)
}

pub(super) fn validate(key: &super::DecodedStateKey) -> Result<(), E> {
    if key.part(2).is_empty()
        || key.part(2).len() > 256
        || (key.family() == 17 && !(1..=u128::from(MAX)).contains(&key.number()))
    {
        return Err(E::InvalidValue);
    }
    Ok(())
}
