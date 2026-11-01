//! Mathematical source integer comparisons, retaining arbitrary magnitudes.
use super::budget::{Budget, Failure, add, source};
use super::options::get;
use super::{I, OriginalCreateError as E};
use std::cmp::Ordering;

fn digits<'a>(value: I<'_, 'a>, scratch: &'a mut [u8; 16]) -> Option<&'a [u8]> {
    let (negative, bytes) = match value {
        I::Integer(n) => {
            *scratch = n.unsigned_abs().to_le_bytes();
            (n < 0, &scratch[..])
        }
        I::BigInteger {
            negative,
            little_endian,
        } => (negative, little_endian),
        _ => return None,
    };
    let end = bytes.iter().rposition(|b| *b != 0).map_or(0, |i| i + 1);
    if negative && end != 0 {
        None
    } else {
        Some(&bytes[..end])
    }
}
pub(super) fn compare(a: I<'_, '_>, b: I<'_, '_>) -> Option<Ordering> {
    let mut left = [0; 16];
    let mut right = [0; 16];
    let a = digits(a, &mut left)?;
    let b = digits(b, &mut right)?;
    Some(
        a.len()
            .cmp(&b.len())
            .then_with(|| a.iter().rev().cmp(b.iter().rev())),
    )
}
pub(super) fn positive(v: I<'_, '_>) -> bool {
    compare(v, I::Integer(0)) == Some(Ordering::Greater)
}
pub(super) fn nonnegative(v: I<'_, '_>) -> bool {
    compare(v, I::Integer(0)).is_some()
}
pub(super) fn uint(v: I<'_, '_>) -> Option<u64> {
    let mut scratch = [0; 16];
    let bytes = digits(v, &mut scratch)?;
    if bytes.len() > 8 {
        return None;
    }
    let mut raw = [0; 8];
    raw[..bytes.len()].copy_from_slice(bytes);
    Some(u64::from_le_bytes(raw))
}
pub(super) fn time<'r, 'a>(
    rows: &[I<'r, 'a>],
    key: &'static str,
    default: Option<I<'r, 'a>>,
) -> Result<Option<I<'r, 'a>>, Failure> {
    let selected = get(rows, key).or(default);
    let Some(value) = selected else {
        return Ok(None);
    };
    if compare(value, I::Integer(9_007_199_254_740_991)) == Some(Ordering::Greater) {
        return Err(source(format!(
            "ERR flow {key} exceeds maximum 9007199254740991"
        )));
    }
    if !nonnegative(value) {
        return Err(source(format!(
            "ERR flow {key} must be a non-negative integer"
        )));
    }
    Ok(Some(value))
}
pub(super) fn history<'r, 'a>(
    rows: &[I<'r, 'a>],
    key: &'static str,
    maximum: I<'r, 'a>,
    allow_zero: bool,
    budget: &mut Budget,
) -> Result<Option<I<'r, 'a>>, Failure> {
    let Some(value) = get(rows, key) else {
        return Ok(None);
    };
    if !(if allow_zero {
        nonnegative(value)
    } else {
        positive(value)
    }) {
        return Err(source(format!(
            "ERR flow {key} must be a {} integer",
            if allow_zero {
                "non-negative"
            } else {
                "positive"
            }
        )));
    }
    if compare(value, maximum) == Some(Ordering::Greater) {
        let maximum = decimal(maximum, budget)?;
        budget.admit(add(maximum.len(), 128)?)?;
        return Err(source(format!("ERR flow {key} exceeds maximum {maximum}")));
    }
    Ok(Some(value))
}
fn decimal(value: I<'_, '_>, budget: &mut Budget) -> Result<String, Failure> {
    use crate::{
        history::metadata::{
            OriginalHistoryMetadataLimits as L, prepare_original_history_metadata,
        },
        value::{OriginalValueInput, OriginalValueNode as N},
    };
    let mut scratch = [0; 16];
    let bytes = digits(value, &mut scratch).ok_or(E::Limit)?;
    let nodes = [
        N::Binary { bytes: 0..0 },
        N::BigInteger {
            negative: false,
            digits: 0..bytes.len(),
        },
        N::Map { entries: 0..2 },
    ];
    let edges = [0, 1];
    let limits = L {
        max_input_bytes: add(bytes.len(), 4096)?,
        max_pairs: 1,
        max_output_bytes: budget.limits.maximum_output_bytes,
        max_work_units: budget
            .limits
            .maximum_work_units
            .checked_sub(budget.work)
            .filter(|n| *n > 0)
            .ok_or(E::Limit)?,
        max_temporary_bytes: budget
            .limits
            .maximum_temporary_bytes
            .checked_sub(budget.live)
            .filter(|n| *n > 0)
            .ok_or(E::Limit)?,
    };
    let prepared = prepare_original_history_metadata(
        OriginalValueInput {
            bytes,
            nodes: &nodes,
            edges: &edges,
            root: 2,
        },
        limits,
    )
    .map_err(|error| match error {
        crate::history::metadata::OriginalHistoryMetadataError::ReserveFailed(error) => {
            E::ReserveFailed(error)
        }
        _ => E::Limit,
    })?;
    budget.spend(prepared.work_units())?;
    budget.temporary(add(budget.live, prepared.temporary_bytes())?)?;
    let text = prepared
        .into_fields()
        .pop()
        .ok_or(E::Limit)?
        .value
        .into_owned();
    budget.admit(text.len())?;
    String::from_utf8(text).map_err(|_| E::Limit.into())
}
