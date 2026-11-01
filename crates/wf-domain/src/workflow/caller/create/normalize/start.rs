//! Original start-only presence rules and exact lease/deadline boundaries.
use super::super::budget::{Failure, source};
use super::{I, n, o};
use crate::workflow::caller::create::OriginalCreateError as E;
use std::cmp::Ordering;

pub(super) fn reject_create_options(rows: &[I<'_, '_>]) -> Result<(), Failure> {
    if o::get(rows, "idempotent").is_some() {
        return Err(source(
            "ERR flow idempotent start_and_claim is not supported",
        ));
    }
    if o::get(rows, "run_at_ms").is_some() {
        return Err(source(
            "ERR flow run_at_ms is not supported by start_and_claim",
        ));
    }
    Ok(())
}

pub(super) fn lease<'r, 'a>(rows: &[I<'r, 'a>]) -> Result<I<'r, 'a>, Failure> {
    let value = o::get(rows, "lease_ms").unwrap_or(I::Integer(30_000));
    if n::compare(value, I::Integer(9_007_199_254_740_991)) == Some(Ordering::Greater) {
        return Err(source("ERR flow lease_ms exceeds maximum 9007199254740991"));
    }
    if !n::positive(value) {
        return Err(source("ERR flow lease_ms must be a positive integer"));
    }
    Ok(value)
}

pub(super) fn deadline(now: Option<I<'_, '_>>, lease: Option<I<'_, '_>>) -> Result<(), Failure> {
    let Some(now) = now else {
        return Ok(());
    };
    let now = n::uint(now).ok_or(E::Limit)?;
    let lease = lease.and_then(n::uint).ok_or(E::Limit)?;
    if now > 9_007_199_254_740_991 - lease {
        return Err(source(
            "ERR flow lease_ms deadline exceeds maximum 9007199254740991",
        ));
    }
    Ok(())
}
