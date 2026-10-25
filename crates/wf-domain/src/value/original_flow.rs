//! Owned source FSV2/ETF value compatibility, independent of policy envelopes.

mod budget;
mod context;
mod equality;
mod model;
mod scan;

use crate::value::original_flow::budget::{Budget, reserve_exact};
use crate::value::original_flow::context::ContextState;
use crate::value::original_flow::scan::{ScanTarget, Scanner};

pub use model::{
    OriginalFlowValueError, OriginalValueContext, OriginalValueDocument,
    OriginalValueFunctionIdentity, OriginalValueLimits, OriginalValueNode,
};

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
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let root = match decode_wrapper(&encoded, &mut budget)? {
        ValueEncoding::Raw { start } => {
            budget.require_depth(1)?;
            budget.require_nodes(0, 1)?;
            budget.spend(
                encoded
                    .len()
                    .checked_sub(start)
                    .ok_or(OriginalFlowValueError::InvalidFlowValue)?,
            )?;
            reserve_exact(&mut nodes, 1, &mut budget)?;
            budget.spend(1)?;
            nodes.push(OriginalValueNode::Binary {
                bytes: start..encoded.len(),
            });
            0
        }
        ValueEncoding::Etf { version_offset } => {
            let version = encoded
                .get(version_offset)
                .ok_or(OriginalFlowValueError::InvalidFlowValue)?;
            budget.spend(1)?;
            if *version != 131 {
                return Err(OriginalFlowValueError::InvalidFlowValue);
            }
            let start = version_offset
                .checked_add(1)
                .ok_or(OriginalFlowValueError::Limit)?;
            let mut state = ContextState::new();
            let (_, census) = Scanner::new(
                &encoded,
                context,
                &mut state,
                &mut budget,
                ScanTarget::Census,
                start,
            )
            .run()?;
            reserve_exact(&mut nodes, census.finished, &mut budget)?;
            reserve_exact(&mut edges, census.edges, &mut budget)?;
            let (root, _) = Scanner::new(
                &encoded,
                context,
                &mut state,
                &mut budget,
                ScanTarget::Build {
                    nodes: &mut nodes,
                    edges: &mut edges,
                    expected: census,
                },
                start,
            )
            .run()?;
            drop(state);
            root.id
        }
    };
    finish_document(encoded, nodes, edges, root, &mut budget)
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

fn finish_document(
    encoded: Vec<u8>,
    nodes: Vec<OriginalValueNode>,
    edges: Vec<usize>,
    root: usize,
    budget: &mut Budget,
) -> Result<OriginalValueDocument, OriginalFlowValueError> {
    if root >= nodes.len() {
        return Err(OriginalFlowValueError::InvalidFlowValue);
    }
    for id in &edges {
        budget.spend(1)?;
        if *id >= nodes.len() {
            return Err(OriginalFlowValueError::InvalidFlowValue);
        }
    }
    let document = OriginalValueDocument {
        encoded,
        nodes,
        edges,
        root,
    };
    equality::validate_maps(&document, budget)?;
    Ok(document)
}
