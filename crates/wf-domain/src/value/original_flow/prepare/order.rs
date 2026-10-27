//! Fallible heapsort and source exact term ordering over borrowed arenas.

pub(in crate::value::original_flow::prepare) mod scalar;
pub(in crate::value::original_flow::prepare) mod sequence;

use std::cmp::Ordering;
use std::ops::Range;

use crate::value::original_flow::OriginalValueNode as N;
use crate::value::original_flow::prepare::OriginalValueEncodeError as E;
use crate::value::original_flow::prepare::budget::{Budget, add, frames, mul};
use crate::value::original_flow::prepare::layout::TermView;
use crate::value::original_flow::prepare::order::sequence::ListCursor;

pub(in crate::value::original_flow::prepare) use scalar::atoms;

#[derive(Clone, Copy)]
pub(in crate::value::original_flow::prepare) enum SeqSource {
    Input,
    MapKeys,
    MapValues,
}

pub(in crate::value::original_flow::prepare) enum CompareFrame {
    Pair(usize, usize),
    Sequence {
        source: SeqSource,
        left: Range<usize>,
        right: Range<usize>,
    },
    Lists {
        left: ListCursor,
        right: ListCursor,
    },
}

pub(in crate::value::original_flow::prepare) fn push(
    view: TermView<'_, '_>,
    stack: &mut Vec<CompareFrame>,
    frame: CompareFrame,
    budget: &mut Budget,
) -> Result<(), E> {
    budget.spend(1)?;
    if stack.len() >= frames(view.input.nodes.len())? {
        return Err(E::Limit);
    }
    stack.push(frame);
    Ok(())
}

fn compare(
    view: TermView<'_, '_>,
    a: usize,
    b: usize,
    stack: &mut Vec<CompareFrame>,
    budget: &mut Budget,
) -> Result<Ordering, E> {
    budget.spend(add(stack.len(), 1)?)?;
    stack.clear();
    push(view, stack, CompareFrame::Pair(a, b), budget)?;
    while let Some(frame) = stack.pop() {
        budget.spend(1)?;
        let order = match frame {
            CompareFrame::Pair(a, b) => pair(view, a, b, stack, budget)?,
            CompareFrame::Sequence {
                source,
                left,
                right,
            } => {
                sequence::edges(view, source, left, right, stack, budget)?;
                None
            }
            CompareFrame::Lists { left, right } => {
                sequence::lists(view, left, right, stack, budget)?
            }
        };
        if let Some(value) = order
            && value != Ordering::Equal
        {
            return Ok(value);
        }
    }
    Ok(Ordering::Equal)
}

fn pair(
    view: TermView<'_, '_>,
    a: usize,
    b: usize,
    stack: &mut Vec<CompareFrame>,
    budget: &mut Budget,
) -> Result<Option<Ordering>, E> {
    budget.spend(128)?;
    let (a, b) = (view.canonical(a)?, view.canonical(b)?);
    if a == b {
        return Ok(None);
    }
    if let Some(order) = scalar::compare(view, a, b, budget)? {
        return Ok(Some(order));
    }
    match (view.node(a)?, view.node(b)?) {
        (N::Tuple { children: left }, N::Tuple { children: right })
        | (N::ForeignLocalFun { free: left, .. }, N::ForeignLocalFun { free: right, .. }) => {
            let order = left.len().cmp(&right.len());
            if order != Ordering::Equal {
                return Ok(Some(order));
            }
            push(
                view,
                stack,
                CompareFrame::Sequence {
                    source: SeqSource::Input,
                    left: left.clone(),
                    right: right.clone(),
                },
                budget,
            )?;
        }
        (N::Map { .. }, N::Map { .. }) => {
            let (left, right) = (&view.plan(a)?.order, &view.plan(b)?.order);
            let order = left.len().cmp(&right.len());
            if order != Ordering::Equal {
                return Ok(Some(order));
            }
            push(
                view,
                stack,
                CompareFrame::Sequence {
                    source: SeqSource::MapValues,
                    left: left.clone(),
                    right: right.clone(),
                },
                budget,
            )?;
            push(
                view,
                stack,
                CompareFrame::Sequence {
                    source: SeqSource::MapKeys,
                    left: left.clone(),
                    right: right.clone(),
                },
                budget,
            )?;
        }
        (N::List { .. }, N::List { .. }) => push(
            view,
            stack,
            CompareFrame::Lists {
                left: ListCursor::new(a),
                right: ListCursor::new(b),
            },
            budget,
        )?,
        _ => return Err(E::InvalidInput),
    }
    Ok(None)
}

fn key(view: TermView<'_, '_>, offset: usize) -> Result<usize, E> {
    view.input.edges.get(offset).copied().ok_or(E::InvalidInput)
}

pub(in crate::value::original_flow::prepare) fn sort_map(
    view: TermView<'_, '_>,
    offsets: &mut [usize],
    scratch: &mut Vec<CompareFrame>,
    budget: &mut Budget,
) -> Result<(), E> {
    if offsets.len() < 2 {
        return Ok(());
    }
    for root in (0..offsets.len() / 2).rev() {
        sift(view, offsets, root, offsets.len(), scratch, budget)?;
    }
    for end in (1..offsets.len()).rev() {
        budget.spend(1)?;
        offsets.swap(0, end);
        sift(view, offsets, 0, end, scratch, budget)?;
    }
    for pair in offsets.windows(2) {
        if compare(
            view,
            key(view, pair[0])?,
            key(view, pair[1])?,
            scratch,
            budget,
        )? == Ordering::Equal
        {
            return Err(E::InvalidInput);
        }
    }
    Ok(())
}

fn sift(
    view: TermView<'_, '_>,
    offsets: &mut [usize],
    mut root: usize,
    end: usize,
    scratch: &mut Vec<CompareFrame>,
    budget: &mut Budget,
) -> Result<(), E> {
    loop {
        budget.spend(1)?;
        let mut child = add(mul(root, 2)?, 1)?;
        if child >= end {
            return Ok(());
        }
        if child + 1 < end
            && compare(
                view,
                key(view, offsets[child])?,
                key(view, offsets[child + 1])?,
                scratch,
                budget,
            )? == Ordering::Less
        {
            child += 1;
        }
        if compare(
            view,
            key(view, offsets[root])?,
            key(view, offsets[child])?,
            scratch,
            budget,
        )? != Ordering::Less
        {
            return Ok(());
        }
        offsets.swap(root, child);
        root = child;
    }
}
