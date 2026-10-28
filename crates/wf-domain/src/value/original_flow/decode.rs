//! Shared strict parse; callers own the unchanged input and continuous budget.

use crate::value::original_flow::budget::{Budget, reserve_exact};
use crate::value::original_flow::context::ContextState;
use crate::value::original_flow::scan::{ScanTarget, Scanner};
use crate::value::original_flow::{
    OriginalFlowValueError as E, OriginalValueContext as C, OriginalValueDocument as D,
    OriginalValueNode as N, ValueEncoding, equality,
};

pub(in crate::value::original_flow) fn parse(
    document: &mut D,
    encoding: &ValueEncoding,
    context: &C<'_>,
    budget: &mut Budget,
) -> Result<(), E> {
    match encoding {
        ValueEncoding::Raw { start } => binary(document, *start, budget)?,
        ValueEncoding::Etf { version_offset } => typed(document, *version_offset, context, budget)?,
    }
    validate(document, budget)
}

pub(in crate::value::original_flow) fn binary(
    document: &mut D,
    start: usize,
    budget: &mut Budget,
) -> Result<(), E> {
    let D { encoded, nodes, .. } = document;
    budget.require_depth(1)?;
    budget.require_nodes(0, 1)?;
    budget.spend(
        encoded
            .len()
            .checked_sub(start)
            .ok_or(E::InvalidFlowValue)?,
    )?;
    reserve_exact(nodes, 1, budget)?;
    budget.spend(1)?;
    nodes.push(N::Binary {
        bytes: start..encoded.len(),
    });
    document.root = 0;
    Ok(())
}

fn typed(
    document: &mut D,
    version_offset: usize,
    context: &C<'_>,
    budget: &mut Budget,
) -> Result<(), E> {
    let D {
        encoded,
        nodes,
        edges,
        root,
    } = document;
    let version = encoded.get(version_offset).ok_or(E::InvalidFlowValue)?;
    budget.spend(1)?;
    if *version != 131 {
        return Err(E::InvalidFlowValue);
    }
    let start = version_offset.checked_add(1).ok_or(E::Limit)?;
    let mut state = ContextState::new();
    let (_, census) = Scanner::new(
        encoded,
        context,
        &mut state,
        budget,
        ScanTarget::Census,
        start,
    )
    .run()?;
    reserve_exact(nodes, census.finished, budget)?;
    reserve_exact(edges, census.edges, budget)?;
    let (parsed, _) = Scanner::new(
        encoded,
        context,
        &mut state,
        budget,
        ScanTarget::Build {
            nodes,
            edges,
            expected: census,
        },
        start,
    )
    .run()?;
    drop(state);
    *root = parsed.id;
    Ok(())
}

pub(in crate::value::original_flow) fn validate(
    document: &D,
    budget: &mut Budget,
) -> Result<(), E> {
    if document.root >= document.nodes.len() {
        return Err(E::InvalidFlowValue);
    }
    for id in &document.edges {
        budget.spend(1)?;
        if *id >= document.nodes.len() {
            return Err(E::InvalidFlowValue);
        }
    }
    equality::validate_maps(document, budget)
}
