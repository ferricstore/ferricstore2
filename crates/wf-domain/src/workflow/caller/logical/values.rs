//! Selected opaque values use the real source encoder and remaining budgets.

use crate::value::{
    OriginalValueEncodeError, OriginalValueEncodeLimits, OriginalValueInput as I,
    OriginalValueNode as N, PreparedOriginalFlowValue as P, prepare_original_flow_value,
    prepare_original_flow_value_with_init_pid,
};
use crate::workflow::caller::MutationFieldValue as S;
use crate::workflow::caller::logical::{
    LogicalMutationError as E, LogicalMutationFieldValue as V, LogicalMutationInput as Input,
};
use crate::workflow::caller::logical::{budget::Budget, capture};

pub(super) fn prepare(
    input: Input<'_>,
    common: &[(&str, V<'_>)],
    budget: &mut Budget,
) -> Result<[Option<Vec<u8>>; 2], E> {
    let names = match input.operation {
        "create" | "transition" => [Some("payload"), None],
        "complete" => [Some("payload"), Some("result")],
        "fail" | "retry" => [Some("payload"), Some("error")],
        "cancel" => [None, Some("reason")],
        _ => [None, None],
    };
    let mut output = [None, None];
    for (name, slot) in names.into_iter().zip(&mut output) {
        let Some(name) = name else { continue };
        let Some(value) = capture::selected(input, common, name) else {
            continue;
        };
        let limits = budget.value_limits()?;
        let prepared = encode(value, limits).map_err(|source| E::Value {
            field: name,
            source,
        })?;
        budget.value_done(&prepared)?;
        *slot = Some(prepared.into_stored_bytes());
    }
    Ok(output)
}

fn encode(value: V<'_>, limits: OriginalValueEncodeLimits) -> Result<P, OriginalValueEncodeError> {
    match value {
        V::Value {
            input,
            init_pid: Some(pid),
        } => prepare_original_flow_value_with_init_pid(input, pid, limits),
        V::Value {
            input,
            init_pid: None,
        } => prepare_original_flow_value(input, limits),
        V::Scalar(S::Bytes(bytes)) => primitive(
            bytes,
            N::Binary {
                bytes: 0..bytes.len(),
            },
            limits,
        ),
        V::Scalar(S::Nil) => atom(b"nil", limits),
        V::Scalar(S::Bool(true)) => atom(b"true", limits),
        V::Scalar(S::Bool(false)) => atom(b"false", limits),
        V::Scalar(S::Integer(integer)) => {
            if let Ok(value) = i64::try_from(integer) {
                primitive(&[], N::Integer(value), limits)
            } else {
                let digits = integer.unsigned_abs().to_le_bytes();
                primitive(
                    &digits,
                    N::BigInteger {
                        negative: integer < 0,
                        digits: 0..digits.len(),
                    },
                    limits,
                )
            }
        }
        V::Scalar(S::Retry(_) | S::Unsupported) => Err(OriginalValueEncodeError::InvalidInput),
    }
}

fn atom(bytes: &[u8], limits: OriginalValueEncodeLimits) -> Result<P, OriginalValueEncodeError> {
    primitive(
        bytes,
        N::Atom {
            bytes: 0..bytes.len(),
            utf8: true,
        },
        limits,
    )
}

fn primitive(
    bytes: &[u8],
    node: N,
    limits: OriginalValueEncodeLimits,
) -> Result<P, OriginalValueEncodeError> {
    prepare_original_flow_value(
        I {
            bytes,
            nodes: &[node],
            edges: &[],
            root: 0,
        },
        limits,
    )
}
