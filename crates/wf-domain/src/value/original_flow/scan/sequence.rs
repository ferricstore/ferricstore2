//! Contiguous compound edges, zero-head aliases and STRING expansion.

use crate::value::original_flow::scan::frame::ParseFrame;
use crate::value::original_flow::scan::{Scanner, SummaryKind, TermSummary};
use crate::value::original_flow::{OriginalFlowValueError as E, OriginalValueNode as N};

pub(in crate::value::original_flow) fn start(
    scan: &mut Scanner<'_, '_, '_, '_>,
    tag: u8,
    offset: usize,
) -> Result<Option<TermSummary>, E> {
    if tag == 106 {
        return Ok(Some(scan.emit(
            N::EmptyList,
            SummaryKind::Other,
            (tag, offset),
        )?));
    }
    let header = scan.take(match tag {
        104 => 1,
        107 => 2,
        _ => 4,
    })?;
    let bytes = scan.input.get(header).ok_or(E::InvalidFlowValue)?;
    let count = match tag {
        104 => usize::from(*bytes.first().ok_or(E::InvalidFlowValue)?),
        107 => usize::from(u16::from_be_bytes(
            bytes.try_into().map_err(|_| E::InvalidFlowValue)?,
        )),
        _ => usize::try_from(u32::from_be_bytes(
            bytes.try_into().map_err(|_| E::InvalidFlowValue)?,
        ))
        .map_err(|_| E::Limit)?,
    };
    if tag == 107 {
        let bytes = scan.take(count)?;
        if count == 0 {
            return Ok(Some(scan.emit(
                N::EmptyList,
                SummaryKind::Other,
                (tag, offset),
            )?));
        }
        let minimum = scan
            .budget
            .require_nodes(scan.counts.finished, count.checked_add(2).ok_or(E::Limit)?)?;
        scan.counts.planned = scan.counts.planned.max(minimum);
        let slots = scan.slots(count)?;
        scan.push_frame(ParseFrame::String {
            tag,
            offset,
            depth: scan.next_depth,
            bytes,
            slots,
            next: 0,
        })?;
        return Ok(None);
    }
    let edges = if tag == 116 {
        count.checked_mul(2).ok_or(E::InvalidFlowValue)?
    } else {
        count
    };
    let total = edges
        .checked_add(usize::from(tag == 108))
        .ok_or(E::InvalidFlowValue)?;
    if total > scan.input.len() - scan.cursor {
        return Err(E::InvalidFlowValue);
    }
    if tag == 108 && count == 0 {
        scan.push_frame(ParseFrame::Alias {
            tag,
            offset,
            depth: scan.next_depth,
        })?;
        return Ok(None);
    }
    let minimum = scan
        .budget
        .require_nodes(scan.counts.finished, total.checked_add(1).ok_or(E::Limit)?)?;
    scan.counts.planned = scan.counts.planned.max(minimum);
    let slots = scan.slots(edges)?;
    if total == 0 {
        let node = if tag == 116 {
            N::Map { entries: slots }
        } else {
            N::Tuple { children: slots }
        };
        return Ok(Some(scan.emit(node, SummaryKind::Other, (tag, offset))?));
    }
    scan.push_frame(ParseFrame::Sequence {
        tag,
        offset,
        depth: scan.next_depth,
        slots,
        filled: 0,
        total,
    })?;
    Ok(None)
}

pub(in crate::value::original_flow) fn accept(
    scan: &mut Scanner<'_, '_, '_, '_>,
    frame: ParseFrame,
    child: TermSummary,
) -> Result<Option<TermSummary>, E> {
    let ParseFrame::Sequence {
        tag,
        offset,
        depth,
        slots,
        filled,
        total,
    } = frame
    else {
        return Err(E::InvalidFlowValue);
    };
    if tag != 108 || filled < slots.len() {
        scan.write_edge(slots.start.checked_add(filled).ok_or(E::Limit)?, child.id)?;
    }
    let filled = filled.checked_add(1).ok_or(E::Limit)?;
    if filled < total {
        scan.push_frame(ParseFrame::Sequence {
            tag,
            offset,
            depth,
            slots,
            filled,
            total,
        })?;
        return Ok(None);
    }
    let node = match tag {
        104 | 105 => N::Tuple { children: slots },
        108 => N::List {
            heads: slots,
            tail: child.id,
        },
        116 => N::Map { entries: slots },
        _ => return Err(E::InvalidFlowValue),
    };
    Ok(Some(scan.emit(node, SummaryKind::Other, (tag, offset))?))
}

pub(in crate::value::original_flow) fn string_step(
    scan: &mut Scanner<'_, '_, '_, '_>,
    frame: ParseFrame,
) -> Result<Option<TermSummary>, E> {
    let ParseFrame::String {
        tag,
        offset,
        depth,
        bytes,
        slots,
        next,
    } = frame
    else {
        return Err(E::InvalidFlowValue);
    };
    if next < bytes.len() {
        scan.budget.spend(1)?;
        let value = i64::from(
            *scan
                .input
                .get(bytes.start + next)
                .ok_or(E::InvalidFlowValue)?,
        );
        let child = scan.emit(
            N::Integer(value),
            SummaryKind::Integer(value),
            (tag, offset),
        )?;
        scan.write_edge(slots.start + next, child.id)?;
        scan.push_frame(ParseFrame::String {
            tag,
            offset,
            depth,
            bytes,
            slots,
            next: next + 1,
        })?;
        return Ok(None);
    }
    let tail = scan.emit(N::EmptyList, SummaryKind::Other, (tag, offset))?;
    Ok(Some(scan.emit(
        N::List {
            heads: slots,
            tail: tail.id,
        },
        SummaryKind::Other,
        (tag, offset),
    )?))
}
