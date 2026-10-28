//! Flat postorder facts; no encoded subterm cache or recursive value owner.

mod compound;
mod shape;

use std::mem::size_of;
use std::ops::Range;

use crate::value::original_flow::OriginalValueNode as N;
use crate::value::original_flow::prepare::budget::{Budget, add, frames, mul};
use crate::value::original_flow::prepare::{
    OriginalValueEncodeError as E, OriginalValueInput as I,
};
use crate::value::original_flow::prepare::{order, scalar};

#[derive(Default, Clone, Copy)]
pub(in crate::value::original_flow::prepare) struct ListInfo {
    pub(in crate::value::original_flow::prepare) heads: usize,
    pub(in crate::value::original_flow::prepare) bytes: usize,
    pub(in crate::value::original_flow::prepare) tail: usize,
    pub(in crate::value::original_flow::prepare) byte_values: bool,
}

#[derive(Default)]
pub(in crate::value::original_flow::prepare) struct NodePlan {
    pub(in crate::value::original_flow::prepare) canonical: usize,
    pub(in crate::value::original_flow::prepare) size: usize,
    pub(in crate::value::original_flow::prepare) depth: usize,
    pub(in crate::value::original_flow::prepare) atom_len: usize,
    pub(in crate::value::original_flow::prepare) list: ListInfo,
    pub(in crate::value::original_flow::prepare) order: Range<usize>,
    pub(in crate::value::original_flow::prepare) fun_uniq: i32,
}

pub(in crate::value::original_flow::prepare) struct Layout<'a> {
    pub(in crate::value::original_flow::prepare) input: I<'a>,
    pub(in crate::value::original_flow::prepare) init_pid: Option<usize>,
    pub(in crate::value::original_flow::prepare) plans: Vec<NodePlan>,
    pub(in crate::value::original_flow::prepare) order: Vec<usize>,
}

#[derive(Clone, Copy)]
pub(in crate::value::original_flow::prepare) struct TermView<'a, 'b> {
    pub(in crate::value::original_flow::prepare) input: I<'a>,
    pub(in crate::value::original_flow::prepare) init_pid: Option<usize>,
    pub(in crate::value::original_flow::prepare) plans: &'b [NodePlan],
    pub(in crate::value::original_flow::prepare) order: &'b [usize],
}

impl<'a, 'b> TermView<'a, 'b> {
    pub(in crate::value::original_flow::prepare) fn canonical(self, id: usize) -> Result<usize, E> {
        Ok(self.plans.get(id).ok_or(E::InvalidInput)?.canonical)
    }
    pub(in crate::value::original_flow::prepare) fn node(self, id: usize) -> Result<&'a N, E> {
        self.input
            .nodes
            .get(self.canonical(id)?)
            .ok_or(E::InvalidInput)
    }
    pub(in crate::value::original_flow::prepare) fn plan(
        self,
        id: usize,
    ) -> Result<&'b NodePlan, E> {
        self.plans.get(id).ok_or(E::InvalidInput)
    }
    pub(in crate::value::original_flow::prepare) fn edges(
        self,
        range: Range<usize>,
    ) -> Result<&'a [usize], E> {
        self.input.edges.get(range).ok_or(E::InvalidInput)
    }
}

impl Layout<'_> {
    pub(in crate::value::original_flow::prepare) fn view(&self) -> TermView<'_, '_> {
        TermView {
            input: self.input,
            init_pid: self.init_pid,
            plans: &self.plans,
            order: &self.order,
        }
    }
}

pub(in crate::value::original_flow::prepare) fn build<'a>(
    input: I<'a>,
    init_pid: Option<usize>,
    budget: &mut Budget,
) -> Result<Layout<'a>, E> {
    if input.bytes.len() > budget.limits.max_input_bytes
        || input.nodes.len() > budget.limits.max_nodes
        || input.edges.len() > budget.limits.max_edges
    {
        return Err(E::Limit);
    }
    if input.root >= input.nodes.len() {
        return Err(E::InvalidInput);
    }
    budget.spend(add(input.bytes.len(), input.nodes.len())?)?;
    let (slots, sorting) = map_slots(input)?;
    let mut layout = Layout {
        input,
        init_pid,
        plans: budget.reserve(input.nodes.len())?,
        order: budget.reserve(slots)?,
    };
    let count = if sorting {
        frames(input.nodes.len())?
    } else {
        0
    };
    let mut scratch = budget.reserve::<order::CompareFrame>(count)?;
    for (id, node) in input.nodes.iter().enumerate() {
        budget.spend(1)?;
        let mut plan = NodePlan {
            canonical: id,
            depth: 1,
            ..NodePlan::default()
        };
        if let Some((size, atom_len)) = scalar::size(input, node, budget)? {
            plan.size = size;
            plan.atom_len = atom_len;
        } else {
            compound::plan(&mut layout, id, node, &mut plan, &mut scratch, budget)?;
        }
        if plan.depth > budget.limits.max_depth {
            return Err(E::Limit);
        }
        layout.plans.push(plan);
    }
    drop(scratch);
    budget.release(mul(count, size_of::<order::CompareFrame>())?)?;
    Ok(layout)
}

fn map_slots(input: I<'_>) -> Result<(usize, bool), E> {
    let mut count = 0;
    let mut sorting = false;
    for node in input.nodes {
        if let N::Map { entries } = node {
            let length = input
                .edges
                .get(entries.clone())
                .ok_or(E::InvalidInput)?
                .len();
            if length % 2 != 0 {
                return Err(E::InvalidInput);
            }
            count = add(count, length / 2)?;
            sorting |= length > 2;
        }
    }
    Ok((count, sorting))
}
