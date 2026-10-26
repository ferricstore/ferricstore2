//! Closed scoped value identities; layout capability is checked by the backend.

use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{OriginalValueRef, OriginalValueRefKind, parse_original_value_ref};
use wf_types::identity::{PartitionId, RecordId};

use super::{StateValueError, encode_components, key_size, key_start, put_component};
use crate::state::value::canonical_history_position;

pub(crate) fn encode_immutable_key(
    scope: &RequestScope,
    public_ref: &[u8],
    incarnation: u128,
    maximum: usize,
) -> Result<Vec<u8>, StateValueError> {
    validate_reference(public_ref)?;
    let fields = [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        public_ref,
    ];
    let mut output = key_start(8, key_size(&fields, 16)?, maximum)?;
    for field in fields {
        put_component(&mut output, field);
    }
    output.extend_from_slice(&incarnation.to_be_bytes());
    Ok(output)
}

pub(crate) fn encode_alias_key(
    scope: &RequestScope,
    public_ref: &[u8],
    maximum: usize,
) -> Result<Vec<u8>, StateValueError> {
    validate_reference(public_ref)?;
    encode_components(
        9,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            public_ref,
        ],
        maximum,
    )
}

pub(crate) fn encode_binding_key(
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    history: Option<&str>,
    maximum: usize,
) -> Result<Vec<u8>, StateValueError> {
    let fields = [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        partition.as_bytes(),
        id.as_bytes(),
    ];
    let Some(history) = history else {
        return encode_components(10, &fields, maximum);
    };
    let (milliseconds, _) = canonical_history_position(history.as_bytes())?;
    let all_fields = [
        fields[0],
        fields[1],
        fields[2],
        fields[3],
        fields[4],
        history.as_bytes(),
    ];
    let mut output = key_start(11, key_size(&all_fields, 8)?, maximum)?;
    for field in fields {
        put_component(&mut output, field);
    }
    output.extend_from_slice(&milliseconds.to_be_bytes());
    put_component(&mut output, history.as_bytes());
    Ok(output)
}

pub(super) fn validate_reference(bytes: &[u8]) -> Result<(), StateValueError> {
    if bytes.is_empty() || bytes.len() > 65_535 {
        return Err(StateValueError::InvalidValue);
    }
    match parse_original_value_ref(bytes) {
        Some(OriginalValueRef::Regular {
            kind:
                OriginalValueRefKind::Payload
                | OriginalValueRefKind::Result
                | OriginalValueRefKind::Error,
            version,
            ..
        }) => {
            if version == 0 {
                Err(StateValueError::InvalidValue)
            } else {
                Ok(())
            }
        }
        Some(_) => Err(StateValueError::UnsupportedTag),
        None => Err(StateValueError::InvalidValue),
    }
}
