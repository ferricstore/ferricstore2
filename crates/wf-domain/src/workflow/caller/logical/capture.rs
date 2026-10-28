//! Bound original occurrences before traversing terms or selecting overrides.

use std::mem::size_of;

use crate::value::{OriginalValueInput, OriginalValueNode};
use crate::workflow::caller::logical::budget::{add, mul};
use crate::workflow::caller::logical::{
    LogicalMutationError as E, LogicalMutationFieldValue as V, LogicalMutationInput as Input,
};
use crate::workflow::caller::{MutationCallerError as C, MutationFieldValue as S};

pub(super) fn measure(
    input: Input<'_>,
    common: &[(&str, V<'_>)],
    base: Option<&[u8]>,
    maximum: usize,
) -> Result<usize, E> {
    if input.fields.len() > 8 || common.len() > 8 {
        return Err(C::InvalidOptions.into());
    }
    let mut bytes = add(
        add(input.id.len(), input.operation.len())?,
        base.map_or(0, <[u8]>::len),
    )?;
    for (name, value) in common.iter().chain(input.fields) {
        let width = match value {
            V::Scalar(value) => match value {
                S::Nil | S::Unsupported => 0,
                S::Bool(_) => 1,
                S::Integer(_) => 16,
                S::Bytes(bytes) => bytes.len(),
                S::Retry(retry) => add(32, retry.exhausted_to.as_ref().map_or(0, Vec::len))?,
            },
            V::Value { input, .. } => {
                let pools = add(
                    input.bytes.len(),
                    mul(input.nodes.len(), size_of::<OriginalValueNode>())?,
                )?;
                add(
                    add(pools, mul(input.edges.len(), size_of::<usize>())?)?,
                    add(
                        size_of::<OriginalValueInput<'_>>(),
                        size_of::<Option<usize>>(),
                    )?,
                )?
            }
        };
        bytes = add(add(add(bytes, 1)?, name.len())?, width)?;
    }
    if bytes > maximum {
        return Err(C::RawBytes.into());
    }
    Ok(bytes)
}

pub(super) fn project<'a>(operation: &str, source: &[(&'a str, V<'a>)]) -> [(&'a str, S<'a>); 8] {
    let mut result = [("", S::Nil); 8];
    for ((name, value), slot) in source.iter().zip(&mut result) {
        let projected = match value {
            V::Value { .. } => S::Bytes(b""),
            V::Scalar(value) if opaque(operation, name) => match value {
                S::Retry(_) | S::Unsupported => S::Unsupported,
                _ => S::Bytes(b""),
            },
            V::Scalar(value) => *value,
        };
        *slot = (name, projected);
    }
    result
}

fn opaque(operation: &str, name: &str) -> bool {
    match name {
        "payload" => matches!(
            operation,
            "create" | "transition" | "complete" | "fail" | "retry"
        ),
        "result" => operation == "complete",
        "error" => matches!(operation, "fail" | "retry"),
        "reason" => operation == "cancel",
        _ => false,
    }
}

pub(super) fn selected<'a>(
    input: Input<'a>,
    common: &[(&str, V<'a>)],
    name: &str,
) -> Option<V<'a>> {
    input
        .fields
        .iter()
        .chain(common)
        .find_map(|(key, value)| (*key == name).then_some(*value))
}
