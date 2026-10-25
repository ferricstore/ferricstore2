//! Shared iterative census/build grammar and postorder arena emission.

use std::ops::Range;

use crate::value::original_flow::budget::{Budget, grow_scratch};
use crate::value::original_flow::context::ContextState;
use crate::value::original_flow::{
    OriginalFlowValueError as E, OriginalValueContext as C, OriginalValueNode as N,
};

pub(in crate::value::original_flow) mod foreign;
pub(in crate::value::original_flow) mod frame;
pub(in crate::value::original_flow) mod scalar;
pub(in crate::value::original_flow) mod sequence;

use crate::value::original_flow::scan::frame::ParseFrame;

#[derive(Clone, Copy)]
pub(in crate::value::original_flow) struct Census {
    pub(in crate::value::original_flow) planned: usize,
    pub(in crate::value::original_flow) finished: usize,
    pub(in crate::value::original_flow) edges: usize,
    pub(in crate::value::original_flow) max_depth: usize,
}

pub(in crate::value::original_flow) enum ScanTarget<'out> {
    Census,
    Build {
        nodes: &'out mut Vec<N>,
        edges: &'out mut Vec<usize>,
        expected: Census,
    },
}

#[derive(Clone, Copy)]
pub(in crate::value::original_flow) enum SummaryKind {
    Other,
    Atom {
        start: usize,
        end: usize,
        utf8: bool,
    },
    Integer(i64),
    Pid,
}

#[derive(Clone, Copy)]
pub(in crate::value::original_flow) struct TermSummary {
    pub(in crate::value::original_flow) id: usize,
    pub(in crate::value::original_flow) tag: u8,
    pub(in crate::value::original_flow) offset: usize,
    pub(in crate::value::original_flow) kind: SummaryKind,
}

pub(in crate::value::original_flow) struct Scanner<'input, 'context, 'state, 'out> {
    pub(in crate::value::original_flow) input: &'input [u8],
    pub(in crate::value::original_flow) context: &'context C<'context>,
    pub(in crate::value::original_flow) state: &'state mut ContextState,
    pub(in crate::value::original_flow) budget: &'state mut Budget,
    target: ScanTarget<'out>,
    pub(in crate::value::original_flow) cursor: usize,
    pub(in crate::value::original_flow) counts: Census,
    pub(in crate::value::original_flow) next_depth: usize,
    frames: Vec<ParseFrame>,
}

impl<'input, 'context, 'state, 'out> Scanner<'input, 'context, 'state, 'out> {
    pub(in crate::value::original_flow) fn new(
        input: &'input [u8],
        context: &'context C<'context>,
        state: &'state mut ContextState,
        budget: &'state mut Budget,
        target: ScanTarget<'out>,
        start: usize,
    ) -> Self {
        Self {
            input,
            context,
            state,
            budget,
            target,
            cursor: start,
            counts: Census {
                planned: 0,
                finished: 0,
                edges: 0,
                max_depth: 0,
            },
            next_depth: 1,
            frames: Vec::new(),
        }
    }

    pub(in crate::value::original_flow) fn run(mut self) -> Result<(TermSummary, Census), E> {
        let mut completed = None;
        loop {
            if let Some(term) = completed.take() {
                if self.frames.is_empty() {
                    if self.cursor != self.input.len()
                        || self.counts.finished == 0
                        || self.counts.planned != self.counts.finished
                    {
                        return Err(E::InvalidFlowValue);
                    }
                    if let ScanTarget::Build { expected, .. } = &self.target
                        && (self.counts.finished != expected.finished
                            || self.counts.edges != expected.edges
                            || self.counts.max_depth != expected.max_depth)
                    {
                        return Err(E::InvalidFlowValue);
                    }
                    return Ok((term, self.counts));
                }
                completed = self.accept_child(term)?;
            } else {
                let string_frame = if self.frames.is_empty() {
                    false
                } else {
                    self.budget.spend(1)?;
                    matches!(self.frames.last(), Some(ParseFrame::String { .. }))
                };
                if string_frame {
                    self.budget.spend(2)?;
                    let frame = self.frames.pop().ok_or(E::InvalidFlowValue)?;
                    completed = sequence::string_step(&mut self, frame)?;
                } else {
                    let (tag, offset) = self.begin_node(self.next_depth)?;
                    let expects_atom = if self.frames.is_empty() {
                        false
                    } else {
                        self.budget.spend(1)?;
                        self.frames.last().is_some_and(frame::expects_atom)
                    };
                    if expects_atom && !matches!(tag, 100 | 115 | 118 | 119) {
                        return Err(E::InvalidFlowValue);
                    }
                    completed = match tag {
                        104 | 105 | 106 | 107 | 108 | 116 => {
                            sequence::start(&mut self, tag, offset)?
                        }
                        101 | 114 | 90 | 103 | 88 | 102 | 89 | 120 | 113 | 112 => {
                            foreign::start(&mut self, tag, offset)?
                        }
                        _ => scalar::parse(&mut self, tag, offset)?,
                    };
                }
            }
        }
    }

    pub(in crate::value::original_flow) fn take(
        &mut self,
        count: usize,
    ) -> Result<Range<usize>, E> {
        let end = self.cursor.checked_add(count).ok_or(E::InvalidFlowValue)?;
        if end > self.input.len() {
            return Err(E::InvalidFlowValue);
        }
        self.budget.spend(count)?;
        let range = self.cursor..end;
        self.cursor = end;
        Ok(range)
    }

    pub(in crate::value::original_flow) fn begin_node(
        &mut self,
        depth: usize,
    ) -> Result<(u8, usize), E> {
        self.budget.require_depth(depth)?;
        self.budget.spend(1)?;
        self.counts.max_depth = self.counts.max_depth.max(depth);
        let offset = self.cursor;
        let range = self.take(1)?;
        let tag = *self.input.get(range.start).ok_or(E::InvalidFlowValue)?;
        Ok((tag, offset))
    }

    pub(in crate::value::original_flow) fn emit(
        &mut self,
        node: N,
        kind: SummaryKind,
        site: (u8, usize),
    ) -> Result<TermSummary, E> {
        let id = self.counts.finished;
        let next = self.budget.require_nodes(id, 1)?;
        self.budget.spend(1)?;
        if let ScanTarget::Build {
            nodes, expected, ..
        } = &mut self.target
        {
            if id >= expected.finished || nodes.len() != id || nodes.capacity() < next {
                return Err(E::InvalidFlowValue);
            }
            nodes.push(node);
        }
        self.counts.finished = next;
        self.counts.planned = self.counts.planned.max(next);
        Ok(TermSummary {
            id,
            tag: site.0,
            offset: site.1,
            kind,
        })
    }

    pub(in crate::value::original_flow) fn slots(
        &mut self,
        count: usize,
    ) -> Result<Range<usize>, E> {
        let start = self.counts.edges;
        let end = self.budget.require_edges(start, count)?;
        self.budget.spend(count)?;
        if let ScanTarget::Build {
            edges, expected, ..
        } = &mut self.target
        {
            if end > expected.edges || edges.len() != start || edges.capacity() < end {
                return Err(E::InvalidFlowValue);
            }
            edges.resize(end, usize::MAX);
        }
        self.counts.edges = end;
        Ok(start..end)
    }

    pub(in crate::value::original_flow) fn write_edge(
        &mut self,
        slot: usize,
        id: usize,
    ) -> Result<(), E> {
        if slot >= self.counts.edges || id >= self.counts.finished {
            return Err(E::InvalidFlowValue);
        }
        self.budget.spend(1)?;
        if let ScanTarget::Build { edges, .. } = &mut self.target {
            *edges.get_mut(slot).ok_or(E::InvalidFlowValue)? = id;
        }
        Ok(())
    }

    pub(in crate::value::original_flow) fn push_frame(
        &mut self,
        frame: ParseFrame,
    ) -> Result<(), E> {
        let depth = match &frame {
            ParseFrame::Sequence { depth, .. }
            | ParseFrame::Alias { depth, .. }
            | ParseFrame::String { depth, .. }
            | ParseFrame::Identity { depth, .. }
            | ParseFrame::Export { depth, .. }
            | ParseFrame::LocalFun { depth, .. } => *depth,
        };
        self.next_depth = depth.checked_add(1).ok_or(E::Limit)?;
        let needed = self.frames.len().checked_add(1).ok_or(E::Limit)?;
        grow_scratch(
            &mut self.frames,
            needed,
            self.budget.limits.max_depth,
            self.budget,
        )?;
        self.budget.spend(1)?;
        self.frames.push(frame);
        Ok(())
    }

    pub(in crate::value::original_flow) fn accept_child(
        &mut self,
        child: TermSummary,
    ) -> Result<Option<TermSummary>, E> {
        self.budget.spend(2)?;
        let frame = self.frames.pop().ok_or(E::InvalidFlowValue)?;
        match frame {
            ParseFrame::Alias { tag, offset, .. } => Ok(Some(TermSummary {
                tag,
                offset,
                ..child
            })),
            frame @ ParseFrame::Sequence { .. } => sequence::accept(self, frame, child),
            frame @ (ParseFrame::Identity { .. }
            | ParseFrame::Export { .. }
            | ParseFrame::LocalFun { .. }) => foreign::accept(self, frame, child),
            ParseFrame::String { .. } => Err(E::InvalidFlowValue),
        }
    }
}
