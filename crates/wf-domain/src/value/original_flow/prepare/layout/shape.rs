//! Graph checks precede every child observation; atom identities stay borrowed.

use std::cmp::Ordering;
use std::ops::Range;

use crate::value::original_flow::OriginalValueNode as N;
use crate::value::original_flow::prepare::OriginalValueEncodeError as E;
use crate::value::original_flow::prepare::budget::{Budget, add};
use crate::value::original_flow::prepare::layout::{NodePlan, TermView};
use crate::value::original_flow::prepare::order;

pub(in crate::value::original_flow::prepare) fn child<'b>(
    view: TermView<'_, 'b>,
    id: usize,
    parent: usize,
) -> Result<&'b NodePlan, E> {
    if id >= parent {
        return Err(E::InvalidInput);
    }
    view.plan(id)
}

pub(in crate::value::original_flow::prepare) fn edges(
    view: TermView<'_, '_>,
    range: Range<usize>,
    parent: usize,
    budget: &mut Budget,
) -> Result<(usize, usize), E> {
    let ids = view.edges(range)?;
    budget.spend(add(ids.len(), 1)?)?;
    let mut size = 0;
    let mut depth = 0;
    for id in ids {
        let plan = child(view, *id, parent)?;
        size = add(size, plan.size)?;
        depth = depth.max(plan.depth);
    }
    Ok((size, depth))
}

pub(in crate::value::original_flow::prepare) fn atom(
    view: TermView<'_, '_>,
    id: usize,
    parent: usize,
) -> Result<(usize, usize), E> {
    let plan = child(view, id, parent)?;
    if !matches!(view.node(id)?, N::Atom { .. }) {
        return Err(E::InvalidInput);
    }
    Ok((plan.size, plan.depth))
}

pub(in crate::value::original_flow::prepare) fn fun_coherent(
    view: TermView<'_, '_>,
    id: usize,
    canonical: i32,
    budget: &mut Budget,
) -> Result<(), E> {
    let N::ForeignLocalFun {
        module,
        index,
        arity,
        uniq,
        old_index,
        ..
    } = view.input.nodes.get(id).ok_or(E::InvalidInput)?
    else {
        return Err(E::InvalidInput);
    };
    for (prior, node) in view
        .input
        .nodes
        .get(..id)
        .ok_or(E::InvalidInput)?
        .iter()
        .enumerate()
    {
        budget.spend(1)?;
        if let N::ForeignLocalFun {
            module: pm,
            index: pi,
            arity: pa,
            uniq: pu,
            old_index: po,
            ..
        } = node
            && index == pi
            && order::atoms(view, *module, *pm, budget)? == Ordering::Equal
        {
            budget.spend(33)?;
            let same_entry = arity == pa && uniq == pu;
            if same_entry != (canonical == view.plan(prior)?.fun_uniq)
                || (same_entry && old_index != po)
            {
                return Err(E::InvalidInput);
            }
        }
    }
    Ok(())
}
