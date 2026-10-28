//! One exact stored output owner and bounded iterative traversal continuations.

mod foreign;
mod scalar;

use std::ops::Range;

use crate::value::original_flow::OriginalValueNode as N;
use crate::value::original_flow::prepare::OriginalValueEncodeError as E;
use crate::value::original_flow::prepare::budget::{Budget, add, frames, wire_length};
use crate::value::original_flow::prepare::layout::{Layout, TermView};
use crate::value::original_flow::prepare::order::{
    SeqSource,
    sequence::{self, ListCursor, ListStep},
};

pub(in crate::value::original_flow::prepare) struct Output {
    bytes: Vec<u8>,
    expected: usize,
}

impl Output {
    pub(in crate::value::original_flow::prepare) fn put(
        &mut self,
        bytes: &[u8],
        budget: &mut Budget,
    ) -> Result<(), E> {
        if add(self.bytes.len(), bytes.len())? > self.expected {
            return Err(E::InvalidInput);
        }
        budget.spend(add(bytes.len(), 1)?)?;
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }
    pub(in crate::value::original_flow::prepare) fn byte(
        &mut self,
        value: u8,
        budget: &mut Budget,
    ) -> Result<(), E> {
        self.put(&[value], budget)
    }
    pub(in crate::value::original_flow::prepare) fn u32(
        &mut self,
        value: u32,
        budget: &mut Budget,
    ) -> Result<(), E> {
        self.put(&value.to_be_bytes(), budget)
    }
}

pub(in crate::value::original_flow::prepare) enum EmitFrame {
    Node(usize),
    Edges(Range<usize>),
    Map {
        positions: Range<usize>,
        value: bool,
    },
    List {
        cursor: ListCursor,
        remaining: usize,
        tail: usize,
    },
}

pub(in crate::value::original_flow::prepare) fn run(
    layout: &Layout<'_>,
    budget: &mut Budget,
) -> Result<Vec<u8>, E> {
    let view = layout.view();
    if let Some(bytes) = scalar::raw(view, view.input.root)? {
        return raw(bytes, budget);
    }
    term(layout, b"FSV2\x02\x83", budget)
}

pub(in crate::value::original_flow::prepare) fn run_external(
    layout: &Layout<'_>,
    budget: &mut Budget,
) -> Result<Vec<u8>, E> {
    term(layout, b"\x83", budget)
}

fn term(layout: &Layout<'_>, header: &[u8], budget: &mut Budget) -> Result<Vec<u8>, E> {
    let view = layout.view();
    let size = budget.encoded(add(header.len(), view.plan(view.input.root)?.size)?)?;
    let mut output = Output {
        bytes: budget.reserve(size)?,
        expected: size,
    };
    let mut stack = budget.reserve::<EmitFrame>(frames(view.input.nodes.len())?)?;
    output.put(header, budget)?;
    push(view, &mut stack, EmitFrame::Node(view.input.root), budget)?;
    while let Some(frame) = stack.pop() {
        budget.spend(1)?;
        match frame {
            EmitFrame::Node(id) => node(view, id, &mut output, &mut stack, budget)?,
            EmitFrame::Edges(mut range) => {
                if let Some(id) = range.next() {
                    let child = *view.input.edges.get(id).ok_or(E::InvalidInput)?;
                    push(view, &mut stack, EmitFrame::Edges(range), budget)?;
                    push(view, &mut stack, EmitFrame::Node(child), budget)?;
                }
            }
            EmitFrame::Map { positions, value } => map(view, positions, value, &mut stack, budget)?,
            EmitFrame::List {
                cursor,
                remaining,
                tail,
            } => list(view, cursor, remaining, tail, &mut stack, budget)?,
        }
    }
    if output.bytes.len() != size {
        return Err(E::InvalidInput);
    }
    Ok(output.bytes)
}

fn raw(bytes: &[u8], budget: &mut Budget) -> Result<Vec<u8>, E> {
    budget.spend(add(bytes.len().min(4), 1)?)?;
    let escaped = bytes.starts_with(b"FSV2");
    let size = budget.encoded(add(bytes.len(), if escaped { 5 } else { 0 })?)?;
    let mut output = Output {
        bytes: budget.reserve(size)?,
        expected: size,
    };
    if escaped {
        output.put(b"FSV2\x01", budget)?;
    }
    output.put(bytes, budget)?;
    Ok(output.bytes)
}

fn push(
    view: TermView<'_, '_>,
    stack: &mut Vec<EmitFrame>,
    frame: EmitFrame,
    budget: &mut Budget,
) -> Result<(), E> {
    budget.spend(1)?;
    if stack.len() >= frames(view.input.nodes.len())? {
        return Err(E::Limit);
    }
    stack.push(frame);
    Ok(())
}

fn node(
    view: TermView<'_, '_>,
    id: usize,
    output: &mut Output,
    stack: &mut Vec<EmitFrame>,
    budget: &mut Budget,
) -> Result<(), E> {
    let id = view.canonical(id)?;
    if scalar::node(view, id, output, budget)? {
        return Ok(());
    }
    if foreign::node(view, id, output, budget)? {
        if let N::ForeignLocalFun { free, .. } = view.node(id)? {
            push(view, stack, EmitFrame::Edges(free.clone()), budget)?;
        }
        return Ok(());
    }
    match view.node(id)? {
        N::Tuple { children } => {
            if let Ok(length) = u8::try_from(children.len()) {
                output.put(&[104, length], budget)?;
            } else {
                output.byte(105, budget)?;
                output.u32(wire_length(children.len())?, budget)?;
            }
            push(view, stack, EmitFrame::Edges(children.clone()), budget)?;
        }
        N::Map { .. } => {
            let positions = view.plan(id)?.order.clone();
            output.byte(116, budget)?;
            output.u32(wire_length(positions.len())?, budget)?;
            push(
                view,
                stack,
                EmitFrame::Map {
                    positions,
                    value: false,
                },
                budget,
            )?;
        }
        N::List { .. } => start_list(view, id, output, stack, budget)?,
        _ => return Err(E::InvalidInput),
    }
    Ok(())
}

fn start_list(
    view: TermView<'_, '_>,
    id: usize,
    output: &mut Output,
    stack: &mut Vec<EmitFrame>,
    budget: &mut Budget,
) -> Result<(), E> {
    let list = view.plan(id)?.list;
    if list.byte_values && list.heads <= 65_535 && matches!(view.node(list.tail)?, N::EmptyList) {
        output.byte(107, budget)?;
        output.put(
            &u16::try_from(list.heads)
                .map_err(|_| E::Limit)?
                .to_be_bytes(),
            budget,
        )?;
        let mut cursor = ListCursor::new(id);
        for _ in 0..list.heads {
            let ListStep::Head(head, next) = cursor.step(view, budget)? else {
                return Err(E::InvalidInput);
            };
            let N::Integer(value) = view.node(head)? else {
                return Err(E::InvalidInput);
            };
            output.byte(u8::try_from(*value).map_err(|_| E::InvalidInput)?, budget)?;
            cursor = next;
        }
    } else {
        output.byte(108, budget)?;
        output.u32(wire_length(list.heads)?, budget)?;
        push(
            view,
            stack,
            EmitFrame::List {
                cursor: ListCursor::new(id),
                remaining: list.heads,
                tail: list.tail,
            },
            budget,
        )?;
    }
    Ok(())
}

fn map(
    view: TermView<'_, '_>,
    mut positions: Range<usize>,
    value: bool,
    stack: &mut Vec<EmitFrame>,
    budget: &mut Budget,
) -> Result<(), E> {
    if positions.is_empty() {
        return Ok(());
    }
    let source = if value {
        SeqSource::MapValues
    } else {
        SeqSource::MapKeys
    };
    let id = sequence::item(view, source, positions.start)?;
    if value {
        positions.start = add(positions.start, 1)?;
    }
    push(
        view,
        stack,
        EmitFrame::Map {
            positions,
            value: !value,
        },
        budget,
    )?;
    push(view, stack, EmitFrame::Node(id), budget)
}

fn list(
    view: TermView<'_, '_>,
    cursor: ListCursor,
    remaining: usize,
    tail: usize,
    stack: &mut Vec<EmitFrame>,
    budget: &mut Budget,
) -> Result<(), E> {
    if remaining == 0 {
        return push(view, stack, EmitFrame::Node(tail), budget);
    }
    let ListStep::Head(id, next) = cursor.step(view, budget)? else {
        return Err(E::InvalidInput);
    };
    push(
        view,
        stack,
        EmitFrame::List {
            cursor: next,
            remaining: remaining - 1,
            tail,
        },
        budget,
    )?;
    push(view, stack, EmitFrame::Node(id), budget)
}
