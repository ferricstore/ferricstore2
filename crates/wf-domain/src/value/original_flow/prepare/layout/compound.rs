//! Exact compound sizes and postorder map preparation.

use crate::value::original_flow::OriginalValueNode as N;
use crate::value::original_flow::prepare::OriginalValueEncodeError as E;
use crate::value::original_flow::prepare::budget::{Budget, add, wire_length};
use crate::value::original_flow::prepare::layout::{Layout, ListInfo, NodePlan, TermView, shape};
use crate::value::original_flow::prepare::{order, scalar};

pub(in crate::value::original_flow::prepare) fn plan(
    layout: &mut Layout<'_>,
    id: usize,
    node: &N,
    plan: &mut NodePlan,
    scratch: &mut Vec<order::CompareFrame>,
    budget: &mut Budget,
) -> Result<(), E> {
    match node {
        N::Tuple { children } => {
            let length = layout.view().edges(children.clone())?.len();
            wire_length(length)?;
            let (size, depth) = shape::edges(layout.view(), children.clone(), id, budget)?;
            plan.size = add(if length <= 255 { 2 } else { 5 }, size)?;
            plan.depth = add(depth, 1)?;
        }
        N::List { heads, tail } => list(layout.view(), id, heads.clone(), *tail, plan, budget)?,
        N::Map { entries } => {
            let (size, depth) = shape::edges(layout.view(), entries.clone(), id, budget)?;
            let length = layout.view().edges(entries.clone())?.len();
            wire_length(length / 2)?;
            plan.size = add(5, size)?;
            plan.depth = add(depth, 1)?;
            let start = layout.order.len();
            for offset in (entries.start..entries.end).step_by(2) {
                budget.spend(1)?;
                layout.order.push(offset);
            }
            plan.order = start..layout.order.len();
            let (prefix, current) = layout.order.split_at_mut(start);
            let view = TermView {
                input: layout.input,
                init_pid: layout.init_pid,
                plans: &layout.plans,
                order: prefix,
            };
            order::sort_map(view, current, scratch, budget)?;
        }
        _ => foreign(layout.view(), id, node, plan, budget)?,
    }
    Ok(())
}

fn list(
    view: TermView<'_, '_>,
    id: usize,
    heads: std::ops::Range<usize>,
    tail: usize,
    plan: &mut NodePlan,
    budget: &mut Budget,
) -> Result<(), E> {
    let (size, depth) = shape::edges(view, heads.clone(), id, budget)?;
    let tail_plan = shape::child(view, tail, id)?;
    plan.depth = add(depth.max(tail_plan.depth), 1)?;
    if heads.is_empty() {
        plan.canonical = view.canonical(tail)?;
        plan.size = tail_plan.size;
        plan.list = view.plan(plan.canonical)?.list;
        return Ok(());
    }
    let mut byte_values = true;
    for child in view.edges(heads.clone())? {
        budget.spend(1)?;
        byte_values &= matches!(view.node(*child)?, N::Integer(0..=255));
    }
    let inherited = if matches!(view.node(tail)?, N::List { .. }) {
        view.plan(view.canonical(tail)?)?.list
    } else {
        ListInfo {
            tail: view.canonical(tail)?,
            byte_values: true,
            ..ListInfo::default()
        }
    };
    let list = ListInfo {
        heads: add(heads.len(), inherited.heads)?,
        bytes: add(size, inherited.bytes)?,
        tail: inherited.tail,
        byte_values: byte_values && inherited.byte_values,
    };
    let proper = matches!(view.node(list.tail)?, N::EmptyList);
    plan.size = if proper && list.byte_values && list.heads <= 65_535 {
        add(3, list.heads)?
    } else {
        wire_length(list.heads)?;
        add(5, add(list.bytes, view.plan(list.tail)?.size)?)?
    };
    plan.list = list;
    Ok(())
}

fn foreign(
    view: TermView<'_, '_>,
    id: usize,
    node: &N,
    plan: &mut NodePlan,
    budget: &mut Budget,
) -> Result<(), E> {
    match node {
        N::ForeignPid { node, .. } | N::ForeignPort { node, .. } => {
            let (size, depth) = shape::atom(view, *node, id)?;
            plan.size = add(13, size)?;
            plan.depth = add(depth, 1)?;
        }
        N::ForeignReference { node, words, .. } => {
            let (size, depth) = shape::atom(view, *node, id)?;
            let length = scalar::bytes(view.input, words)?.len();
            if length == 0 || length % 4 != 0 || length > 20 {
                return Err(E::InvalidInput);
            }
            plan.size = add(7, add(size, length)?)?;
            plan.depth = add(depth, 1)?;
        }
        N::ForeignExport {
            module,
            function,
            arity,
        } => {
            if *arity > 255 {
                return Err(E::InvalidInput);
            }
            let (ms, md) = shape::atom(view, *module, id)?;
            let (fs, fd) = shape::atom(view, *function, id)?;
            plan.size = add(3, add(ms, fs)?)?;
            plan.depth = add(1, md.max(fd))?;
        }
        N::ForeignLocalFun { .. } => local_fun(view, id, node, plan, budget)?,
        _ => return Err(E::InvalidInput),
    }
    Ok(())
}

fn local_fun(
    view: TermView<'_, '_>,
    id: usize,
    node: &N,
    plan: &mut NodePlan,
    budget: &mut Budget,
) -> Result<(), E> {
    let N::ForeignLocalFun {
        module,
        old_index,
        old_uniq,
        creator,
        free,
        ..
    } = node
    else {
        return Err(E::InvalidInput);
    };
    if !(-(1_i64 << 59)..(1_i64 << 59)).contains(old_index)
        || !(-(1_i64 << 59)..(1_i64 << 59)).contains(old_uniq)
    {
        return Err(E::InvalidInput);
    }
    let (ms, md) = shape::atom(view, *module, id)?;
    let creator_plan = shape::child(view, *creator, id)?;
    if !matches!(view.node(*creator)?, N::ForeignPid { .. }) {
        return Err(E::InvalidInput);
    }
    let (fs, fd) = shape::edges(view, free.clone(), id, budget)?;
    wire_length(free.len())?;
    plan.fun_uniq = i32::from_le_bytes(
        old_uniq.to_le_bytes()[..4]
            .try_into()
            .map_err(|_| E::InvalidInput)?,
    );
    shape::fun_coherent(view, id, plan.fun_uniq, budget)?;
    let init_pid = view.init_pid.ok_or(E::UnsupportedContext { node: id })?;
    let init_plan = shape::child(view, init_pid, id)?;
    if !matches!(view.node(init_pid)?, N::ForeignPid { .. }) {
        return Err(E::InvalidInput);
    }
    let old_size = add(
        scalar::integer_size(*old_index),
        scalar::integer_size(i64::from(plan.fun_uniq)),
    )?;
    plan.size = add(30, add(ms, add(old_size, add(init_plan.size, fs)?)?)?)?;
    wire_length(plan.size.checked_sub(1).ok_or(E::Limit)?)?;
    plan.depth = add(1, md.max(fd).max(creator_plan.depth).max(init_plan.depth))?;
    Ok(())
}
