//! Iterative exact-key equality with bottom-up duplicate-map validation.

use std::ops::Range;

use crate::value::original_flow::budget::{Budget, grow_scratch};
use crate::value::original_flow::{
    OriginalFlowValueError as E, OriginalValueDocument as D, OriginalValueNode as N,
};

pub(in crate::value::original_flow) mod scalar;

enum CompareFrame {
    Sequence {
        left: Range<usize>,
        right: Range<usize>,
        next: usize,
    },
    List {
        left: ListCursor,
        right: ListCursor,
    },
    Map {
        left: Range<usize>,
        right: Range<usize>,
        row: usize,
        candidate: usize,
        value: bool,
    },
}

enum CompareStep {
    Done(bool),
    Pair {
        pair: (usize, usize),
        continuation: Option<CompareFrame>,
    },
}

#[derive(Clone, Copy)]
struct ListCursor {
    node: usize,
    position: usize,
}

pub(in crate::value::original_flow) fn validate_maps(
    document: &D,
    budget: &mut Budget,
) -> Result<(), E> {
    for node in &document.nodes {
        budget.spend(1)?;
        if let N::Map { entries } = node {
            let ids = document
                .edges
                .get(entries.clone())
                .ok_or(E::InvalidFlowValue)?;
            if ids.len() % 2 != 0 {
                return Err(E::InvalidFlowValue);
            }
            for row in (0..ids.len()).step_by(2) {
                for prior in (0..row).step_by(2) {
                    budget.spend(3)?;
                    if same_term(document, ids[row], ids[prior], budget)? {
                        return Err(E::InvalidFlowValue);
                    }
                }
            }
        }
    }
    Ok(())
}

fn same_term(document: &D, left: usize, right: usize, budget: &mut Budget) -> Result<bool, E> {
    let mut frames = Vec::new();
    let mut step = compare_pair(document, (left, right), budget)?;
    loop {
        step = match step {
            CompareStep::Done(equal) => {
                if frames.is_empty() {
                    return Ok(equal);
                }
                budget.spend(1)?;
                let frame = frames.pop().ok_or(E::InvalidFlowValue)?;
                resume(document, frame, equal, budget)?
            }
            CompareStep::Pair { pair, continuation } => {
                if let Some(frame) = continuation {
                    push_comparison(&mut frames, frame, budget)?;
                }
                compare_pair(document, pair, budget)?
            }
        };
    }
}

fn compare_pair(document: &D, pair: (usize, usize), budget: &mut Budget) -> Result<CompareStep, E> {
    budget.spend(2)?;
    let left = document.nodes.get(pair.0).ok_or(E::InvalidFlowValue)?;
    let right = document.nodes.get(pair.1).ok_or(E::InvalidFlowValue)?;
    if pair.0 == pair.1 {
        return Ok(CompareStep::Done(true));
    }
    let sequence = match (left, right) {
        (N::Tuple { children: a }, N::Tuple { children: b }) => Some((a.clone(), b.clone())),
        (N::ForeignLocalFun { free: a, .. }, N::ForeignLocalFun { free: b, .. }) => {
            if !scalar::functions_equal(document, left, right, budget)? {
                return Ok(CompareStep::Done(false));
            }
            Some((a.clone(), b.clone()))
        }
        _ => None,
    };
    if let Some((a, b)) = sequence {
        if a.len() != b.len() {
            return Ok(CompareStep::Done(false));
        }
        if a.is_empty() {
            return Ok(CompareStep::Done(true));
        }
        budget.spend(2)?;
        let pair = (
            *document.edges.get(a.start).ok_or(E::InvalidFlowValue)?,
            *document.edges.get(b.start).ok_or(E::InvalidFlowValue)?,
        );
        return Ok(CompareStep::Pair {
            pair,
            continuation: Some(CompareFrame::Sequence {
                left: a,
                right: b,
                next: 1,
            }),
        });
    }
    if matches!(left, N::List { .. }) || matches!(right, N::List { .. }) {
        return list_step(
            document,
            (
                ListCursor {
                    node: pair.0,
                    position: 0,
                },
                ListCursor {
                    node: pair.1,
                    position: 0,
                },
            ),
            budget,
        );
    }
    if let (N::Map { entries: a }, N::Map { entries: b }) = (left, right) {
        if a.len() != b.len() {
            return Ok(CompareStep::Done(false));
        }
        if a.is_empty() {
            return Ok(CompareStep::Done(true));
        }
        budget.spend(3)?;
        let pair = (
            *document.edges.get(a.start).ok_or(E::InvalidFlowValue)?,
            *document.edges.get(b.start).ok_or(E::InvalidFlowValue)?,
        );
        return Ok(CompareStep::Pair {
            pair,
            continuation: Some(CompareFrame::Map {
                left: a.clone(),
                right: b.clone(),
                row: 0,
                candidate: 0,
                value: false,
            }),
        });
    }
    Ok(CompareStep::Done(scalar::scalar_equal(
        document, left, right, budget,
    )?))
}

fn resume(
    document: &D,
    frame: CompareFrame,
    equal: bool,
    budget: &mut Budget,
) -> Result<CompareStep, E> {
    budget.spend(1)?;
    match frame {
        CompareFrame::Sequence { left, right, next } => {
            if !equal {
                return Ok(CompareStep::Done(false));
            }
            if next == left.len() {
                return Ok(CompareStep::Done(true));
            }
            budget.spend(2)?;
            let pair = (
                *document
                    .edges
                    .get(left.start + next)
                    .ok_or(E::InvalidFlowValue)?,
                *document
                    .edges
                    .get(right.start + next)
                    .ok_or(E::InvalidFlowValue)?,
            );
            Ok(CompareStep::Pair {
                pair,
                continuation: Some(CompareFrame::Sequence {
                    left,
                    right,
                    next: next.checked_add(1).ok_or(E::Limit)?,
                }),
            })
        }
        CompareFrame::List { left, right } => {
            if !equal {
                return Ok(CompareStep::Done(false));
            }
            list_step(document, (left, right), budget)
        }
        CompareFrame::Map {
            left,
            right,
            mut row,
            mut candidate,
            value,
        } => {
            if value && !equal {
                return Ok(CompareStep::Done(false));
            }
            let next_value = !value && equal;
            if value {
                row = row.checked_add(2).ok_or(E::Limit)?;
                if row == left.len() {
                    return Ok(CompareStep::Done(true));
                }
                candidate = 0;
            } else if !equal {
                candidate = candidate.checked_add(2).ok_or(E::Limit)?;
                if candidate == right.len() {
                    return Ok(CompareStep::Done(false));
                }
            }
            budget.spend(2 + usize::from(!next_value))?;
            let offset = usize::from(next_value);
            let pair = (
                *document
                    .edges
                    .get(left.start + row + offset)
                    .ok_or(E::InvalidFlowValue)?,
                *document
                    .edges
                    .get(right.start + candidate + offset)
                    .ok_or(E::InvalidFlowValue)?,
            );
            Ok(CompareStep::Pair {
                pair,
                continuation: Some(CompareFrame::Map {
                    left,
                    right,
                    row,
                    candidate,
                    value: next_value,
                }),
            })
        }
    }
}

fn list_step(
    document: &D,
    cursors: (ListCursor, ListCursor),
    budget: &mut Budget,
) -> Result<CompareStep, E> {
    let (mut left, mut right) = cursors;
    for cursor in [&mut left, &mut right] {
        loop {
            budget.spend(1)?;
            match document.nodes.get(cursor.node).ok_or(E::InvalidFlowValue)? {
                N::List { heads, tail } if cursor.position == heads.len() => {
                    cursor.node = *tail;
                    cursor.position = 0;
                }
                _ => break,
            }
        }
    }
    budget.spend(2)?;
    match (
        document.nodes.get(left.node),
        document.nodes.get(right.node),
    ) {
        (Some(N::List { heads: a, .. }), Some(N::List { heads: b, .. })) => {
            budget.spend(2)?;
            let pair = (
                *document
                    .edges
                    .get(a.start + left.position)
                    .ok_or(E::InvalidFlowValue)?,
                *document
                    .edges
                    .get(b.start + right.position)
                    .ok_or(E::InvalidFlowValue)?,
            );
            left.position = left.position.checked_add(1).ok_or(E::Limit)?;
            right.position = right.position.checked_add(1).ok_or(E::Limit)?;
            Ok(CompareStep::Pair {
                pair,
                continuation: Some(CompareFrame::List { left, right }),
            })
        }
        (Some(N::List { .. }), _) | (_, Some(N::List { .. })) => Ok(CompareStep::Done(false)),
        (Some(_), Some(_)) => Ok(CompareStep::Pair {
            pair: (left.node, right.node),
            continuation: None,
        }),
        _ => Err(E::InvalidFlowValue),
    }
}

fn push_comparison(
    frames: &mut Vec<CompareFrame>,
    frame: CompareFrame,
    budget: &mut Budget,
) -> Result<(), E> {
    let needed = frames.len().checked_add(1).ok_or(E::Limit)?;
    grow_scratch(
        frames,
        needed,
        budget.limits.max_nodes.min(budget.limits.max_work_units),
        budget,
    )?;
    budget.spend(1)?;
    frames.push(frame);
    Ok(())
}
