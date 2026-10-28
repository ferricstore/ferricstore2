//! Owned source FSV2/ETF value compatibility, independent of policy envelopes.

mod budget;
mod compat;
mod context;
mod decode;
mod equality;
mod model;
mod prepare;
mod scan;

use crate::value::original_flow::budget::Budget;
use crate::value::original_flow::context::ContextState;
use crate::value::original_flow::scan::{ScanTarget, Scanner};

pub use compat::decode_original_flow_value_compat;
pub use model::{
    OriginalFlowValueError, OriginalValueContext, OriginalValueDocument,
    OriginalValueFunctionIdentity, OriginalValueLimits, OriginalValueNode,
};
pub use prepare::{
    OriginalValueEncodeError, OriginalValueEncodeLimits, OriginalValueInput,
    PreparedOriginalFlowValue, prepare_original_flow_value,
    prepare_original_flow_value_with_init_pid,
};

/// Conservative fixed decoder metadata, excluding separately charged backing.
pub(crate) fn decode_fixed_bytes() -> Result<usize, OriginalFlowValueError> {
    use std::mem::size_of;

    let parts = [
        (size_of::<OriginalValueDocument>(), 1),
        (
            size_of::<Result<OriginalValueDocument, OriginalFlowValueError>>(),
            1,
        ),
        (size_of::<Budget>(), 1),
        (size_of::<OriginalValueLimits>(), 1),
        (size_of::<ContextState>(), 1),
        (size_of::<Scanner<'static, 'static, 'static, 'static>>(), 2),
        (size_of::<ScanTarget<'static>>(), 2),
        (size_of::<scan::Census>(), 2),
        (size_of::<scan::TermSummary>(), 8),
        (size_of::<Option<scan::TermSummary>>(), 4),
        (size_of::<scan::frame::ParseFrame>(), 4),
        (size_of::<OriginalValueNode>(), 4),
        (size_of::<ValueEncoding>(), 1),
        (equality::fixed_owner_bytes()?, 1),
    ];
    parts
        .into_iter()
        .try_fold(0_usize, |total, (width, count)| {
            width
                .checked_mul(count)
                .and_then(|part| total.checked_add(part))
                .ok_or(OriginalFlowValueError::Limit)
        })
}

/// Decode stored source value bytes into an owned typed document.
///
/// The unchanged input owner and flat postorder arenas retain typed values.
/// Captured safe tables are explicit compatibility inputs, never runtime or
/// storage authority. Foreign identities are data without BEAM execution.
/// Operational work is continuous across census, build and exact map checks.
///
/// # Errors
///
/// Malformed bytes and closed-table absence return `InvalidFlowValue`.
/// Unavailable or unsupported contexts/profiles, limits and actual reservation
/// failure keep their distinct operational error variants.
pub fn decode_original_flow_value(
    encoded: Vec<u8>,
    context: &OriginalValueContext<'_>,
    limits: OriginalValueLimits,
) -> Result<OriginalValueDocument, OriginalFlowValueError> {
    let mut budget = Budget::new(limits, (encoded.len(), encoded.capacity()))?;
    let encoding = decode_wrapper(&encoded, &mut budget)?;
    let mut document = OriginalValueDocument {
        encoded,
        nodes: Vec::new(),
        edges: Vec::new(),
        root: 0,
    };
    decode::parse(&mut document, &encoding, context, &mut budget)?;
    Ok(document)
}

enum ValueEncoding {
    Raw { start: usize },
    Etf { version_offset: usize },
}

fn decode_wrapper(
    input: &[u8],
    budget: &mut Budget,
) -> Result<ValueEncoding, OriginalFlowValueError> {
    budget.spend(input.len().min(4))?;
    if input.get(..4) != Some(b"FSV2") {
        return Ok(ValueEncoding::Raw { start: 0 });
    }
    let Some(mode) = input.get(4) else {
        return Ok(ValueEncoding::Raw { start: 0 });
    };
    budget.spend(1)?;
    Ok(match *mode {
        1 => ValueEncoding::Raw { start: 5 },
        2 => ValueEncoding::Etf { version_offset: 5 },
        _ => ValueEncoding::Raw { start: 0 },
    })
}
