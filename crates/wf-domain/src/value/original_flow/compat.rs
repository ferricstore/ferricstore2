//! Explicit legacy fallback, independent of strict history-value decoding.

use std::mem::size_of;

use crate::value::original_flow::budget::Budget;
use crate::value::original_flow::{
    OriginalFlowValueError as E, OriginalValueContext as C, OriginalValueDocument as D,
    OriginalValueLimits as L, ValueEncoding, decode, decode_fixed_bytes, decode_wrapper,
};

/// Decode with the pinned legacy fallback and exact stored user-byte size.
///
/// Invalid typed ETF becomes a binary over its original suffix. The returned
/// size excludes a recognized five-byte FSV2 wrapper and includes ETF version.
/// The input Vec is retained without cloning or shrinking. A failed typed parse
/// and its fallback share one continuous budget; failed work is not refunded.
/// This API grants neither storage nor runtime authority.
///
/// # Errors
///
/// Compatibility/profile and operational resource failures remain errors.
/// Only malformed typed values and known-absent safe entries become binary
/// suffixes. Actual reservation failure retains its distinct error variant.
pub fn decode_original_flow_value_compat(
    encoded: Vec<u8>,
    context: &C<'_>,
    limits: L,
) -> Result<(D, usize), E> {
    let mut budget = Budget::new(limits, (encoded.len(), encoded.capacity()))?;
    budget.spend(fixed_bytes()?)?;
    let encoding = decode_wrapper(&encoded, &mut budget)?;
    let (start, typed) = match &encoding {
        ValueEncoding::Raw { start } => (*start, false),
        ValueEncoding::Etf { version_offset } => (*version_offset, true),
    };
    let user_size = encoded
        .len()
        .checked_sub(start)
        .ok_or(E::InvalidFlowValue)?;
    let mut document = D {
        encoded,
        nodes: Vec::new(),
        edges: Vec::new(),
        root: 0,
    };
    match decode::parse(&mut document, &encoding, context, &mut budget) {
        Ok(()) => {}
        Err(E::InvalidFlowValue) if typed => fallback(&mut document, start, &mut budget)?,
        Err(error) => return Err(error),
    }
    Ok((document, user_size))
}

fn fallback(document: &mut D, start: usize, budget: &mut Budget) -> Result<(), E> {
    let cleanup = document
        .nodes
        .len()
        .checked_add(document.edges.len())
        .and_then(|count| count.checked_add(1))
        .ok_or(E::Limit)?;
    budget.spend(cleanup)?;
    document.nodes = Vec::new();
    document.edges = Vec::new();
    decode::binary(document, start, budget)?;
    decode::validate(document, budget)
}

fn fixed_bytes() -> Result<usize, E> {
    // Conservative logical headers, not a native frame/capacity or RSS claim.
    decode_fixed_bytes()?
        .checked_add(size_of::<Result<(D, usize), E>>())
        .and_then(|total| total.checked_add(size_of::<(D, usize)>()))
        .and_then(|total| total.checked_add(4 * size_of::<usize>()))
        .ok_or(E::Limit)
}
