//! Bounded continuations traverse compound values without native recursion.

use std::cmp::Ordering;
use std::ops::Range;

use crate::value::original_flow::OriginalValueNode as N;
use crate::value::original_flow::prepare::OriginalValueEncodeError as E;
use crate::value::original_flow::prepare::budget::{Budget, add};
use crate::value::original_flow::prepare::layout::TermView;
use crate::value::original_flow::prepare::order::{CompareFrame, SeqSource, push, scalar};

#[derive(Clone, Copy)]
pub(in crate::value::original_flow::prepare) struct ListCursor {
    node: usize,
    index: usize,
}

pub(in crate::value::original_flow::prepare) enum ListStep {
    Head(usize, ListCursor),
    Tail(usize),
}

impl ListCursor {
    pub(in crate::value::original_flow::prepare) const fn new(node: usize) -> Self {
        Self { node, index: 0 }
    }

    pub(in crate::value::original_flow::prepare) fn step(
        mut self,
        view: TermView<'_, '_>,
        budget: &mut Budget,
    ) -> Result<ListStep, E> {
        loop {
            budget.spend(1)?;
            self.node = view.canonical(self.node)?;
            let N::List { heads, tail } = view.node(self.node)? else {
                return Ok(ListStep::Tail(self.node));
            };
            if self.index < heads.len() {
                let id = *view
                    .input
                    .edges
                    .get(add(heads.start, self.index)?)
                    .ok_or(E::InvalidInput)?;
                self.index = add(self.index, 1)?;
                return Ok(ListStep::Head(id, self));
            }
            self.node = *tail;
            self.index = 0;
        }
    }
}

pub(in crate::value::original_flow::prepare) fn item(
    view: TermView<'_, '_>,
    source: SeqSource,
    index: usize,
) -> Result<usize, E> {
    let position = match source {
        SeqSource::Input => index,
        SeqSource::MapKeys => *view.order.get(index).ok_or(E::InvalidInput)?,
        SeqSource::MapValues => add(*view.order.get(index).ok_or(E::InvalidInput)?, 1)?,
    };
    view.input
        .edges
        .get(position)
        .copied()
        .ok_or(E::InvalidInput)
}

pub(in crate::value::original_flow::prepare) fn edges(
    view: TermView<'_, '_>,
    source: SeqSource,
    mut left: Range<usize>,
    mut right: Range<usize>,
    stack: &mut Vec<CompareFrame>,
    budget: &mut Budget,
) -> Result<(), E> {
    if left.len() != right.len() {
        return Err(E::InvalidInput);
    }
    if left.is_empty() {
        return Ok(());
    }
    let a = item(view, source, left.start)?;
    let b = item(view, source, right.start)?;
    left.start = add(left.start, 1)?;
    right.start = add(right.start, 1)?;
    push(
        view,
        stack,
        CompareFrame::Sequence {
            source,
            left,
            right,
        },
        budget,
    )?;
    push(view, stack, CompareFrame::Pair(a, b), budget)
}

pub(in crate::value::original_flow::prepare) fn lists(
    view: TermView<'_, '_>,
    left: ListCursor,
    right: ListCursor,
    stack: &mut Vec<CompareFrame>,
    budget: &mut Budget,
) -> Result<Option<Ordering>, E> {
    match (left.step(view, budget)?, right.step(view, budget)?) {
        (ListStep::Head(a, left), ListStep::Head(b, right)) => {
            push(view, stack, CompareFrame::Lists { left, right }, budget)?;
            push(view, stack, CompareFrame::Pair(a, b), budget)?;
            Ok(None)
        }
        (ListStep::Tail(a), ListStep::Tail(b)) => {
            push(view, stack, CompareFrame::Pair(a, b), budget)?;
            Ok(None)
        }
        (ListStep::Head(..), ListStep::Tail(b)) => {
            Ok(Some(11_u8.cmp(&scalar::rank(view.node(b)?))))
        }
        (ListStep::Tail(a), ListStep::Head(..)) => Ok(Some(scalar::rank(view.node(a)?).cmp(&11))),
    }
}
