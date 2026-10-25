//! Source-ordered foreign identities as DATA, without execution capabilities.

use crate::value::original_flow::context::{check_export, local_function};
use crate::value::original_flow::scan::foreign::identity::finish_identity;

mod identity;
use crate::value::original_flow::scan::frame::ParseFrame;
use crate::value::original_flow::scan::{Scanner, SummaryKind, TermSummary};
use crate::value::original_flow::{OriginalFlowValueError as E, OriginalValueNode as N};

pub(in crate::value::original_flow) fn start(
    scan: &mut Scanner<'_, '_, '_, '_>,
    tag: u8,
    offset: usize,
) -> Result<Option<TermSummary>, E> {
    let depth = scan.next_depth;
    if tag == 112 {
        start_local_fun(scan, tag, offset)?;
    } else if tag == 113 {
        if scan.input.len() - scan.cursor < 6 {
            return Err(E::InvalidFlowValue);
        }
        let minimum = scan.budget.require_nodes(scan.counts.finished, 4)?;
        scan.counts.planned = scan.counts.planned.max(minimum);
        scan.push_frame(ParseFrame::Export {
            tag,
            offset,
            depth,
            module: None,
            function: None,
        })?;
    } else {
        let words = if matches!(tag, 114 | 90) {
            let range = scan.take(2)?;
            usize::from(u16::from_be_bytes(
                scan.input
                    .get(range)
                    .ok_or(E::InvalidFlowValue)?
                    .try_into()
                    .map_err(|_| E::InvalidFlowValue)?,
            ))
        } else {
            1
        };
        if words > 5 {
            return Err(E::InvalidFlowValue);
        }
        let fixed = match tag {
            103 => 9,
            88 | 120 => 12,
            102 | 101 => 5,
            89 => 8,
            114 => 1 + 4 * words,
            90 => 4 + 4 * words,
            _ => return Err(E::InvalidFlowValue),
        };
        if scan.input.len() - scan.cursor < fixed + 2 {
            return Err(E::InvalidFlowValue);
        }
        let minimum = scan.budget.require_nodes(scan.counts.finished, 2)?;
        scan.counts.planned = scan.counts.planned.max(minimum);
        scan.push_frame(ParseFrame::Identity {
            tag,
            offset,
            depth,
            words,
        })?;
    }
    Ok(None)
}

pub(in crate::value::original_flow) fn accept(
    scan: &mut Scanner<'_, '_, '_, '_>,
    mut frame: ParseFrame,
    child: TermSummary,
) -> Result<Option<TermSummary>, E> {
    if matches!(frame, ParseFrame::Identity { .. }) {
        return finish_identity(scan, &frame, child);
    }
    if matches!(
        frame,
        ParseFrame::Export {
            module: Some(_),
            function: Some(_),
            ..
        }
    ) {
        return finish_export(scan, &frame, child);
    }
    if matches!(
        frame,
        ParseFrame::LocalFun {
            module: Some(_),
            old_index: Some(_),
            old_uniq: Some(_),
            creator: None,
            ..
        }
    ) {
        return finish_local_fun(scan, frame, child);
    }
    match &mut frame {
        ParseFrame::Export {
            module, function, ..
        } => {
            if module.is_none() {
                *module = Some(child);
            } else {
                *function = Some(child);
            }
        }
        ParseFrame::LocalFun {
            module,
            old_index,
            old_uniq,
            creator,
            free,
            filled,
            ..
        } => {
            if module.is_none() {
                *module = Some(child);
            } else if old_index.is_none() || old_uniq.is_none() {
                let SummaryKind::Integer(value) = child.kind else {
                    return Err(E::InvalidFlowValue);
                };
                if !(-(1_i64 << 59)..(1_i64 << 59)).contains(&value) {
                    return Err(E::InvalidFlowValue);
                }
                if old_index.is_none() {
                    *old_index = Some(value);
                } else {
                    *old_uniq = Some(value);
                }
            } else if creator.is_some() {
                if *filled >= free.len() {
                    return Err(E::InvalidFlowValue);
                }
                scan.write_edge(free.start + *filled, child.id)?;
                *filled += 1;
                if *filled == free.len() {
                    return finish_local_fun(scan, frame, child);
                }
            } else {
                return Err(E::InvalidFlowValue);
            }
        }
        _ => return Err(E::InvalidFlowValue),
    }
    scan.push_frame(frame)?;
    Ok(None)
}

pub(in crate::value::original_flow) fn finish_export(
    scan: &mut Scanner<'_, '_, '_, '_>,
    frame: &ParseFrame,
    child: TermSummary,
) -> Result<Option<TermSummary>, E> {
    let &ParseFrame::Export {
        tag,
        offset,
        module: Some(module),
        function: Some(function),
        ..
    } = frame
    else {
        return Err(E::InvalidFlowValue);
    };
    let SummaryKind::Integer(value) = child.kind else {
        return Err(E::InvalidFlowValue);
    };
    if !(0..(1_i64 << 59)).contains(&value) {
        return Err(E::InvalidFlowValue);
    }
    let arity = u32::from_le_bytes(
        value.to_le_bytes()[..4]
            .try_into()
            .map_err(|_| E::InvalidFlowValue)?,
    );
    check_export(
        scan.context,
        scan.state,
        scan.input,
        module,
        function,
        (arity, (tag, offset)),
        scan.budget,
    )?;
    Ok(Some(scan.emit(
        N::ForeignExport {
            module: module.id,
            function: function.id,
            arity,
        },
        SummaryKind::Other,
        (tag, offset),
    )?))
}

pub(in crate::value::original_flow) fn finish_local_fun(
    scan: &mut Scanner<'_, '_, '_, '_>,
    frame: ParseFrame,
    child: TermSummary,
) -> Result<Option<TermSummary>, E> {
    let ParseFrame::LocalFun {
        tag,
        offset,
        depth,
        module: Some(module),
        arity,
        index,
        uniq,
        old_index: Some(old_index),
        old_uniq: Some(mut old_uniq),
        mut creator,
        free,
        filled,
    } = frame
    else {
        return Err(E::InvalidFlowValue);
    };
    if creator.is_none() {
        if !matches!(child.kind, SummaryKind::Pid) {
            return Err(E::InvalidFlowValue);
        }
        creator = Some(child.id);
        let incoming = i32::from_le_bytes(
            old_uniq.to_le_bytes()[..4]
                .try_into()
                .map_err(|_| E::InvalidFlowValue)?,
        );
        old_uniq = i64::from(local_function(
            scan.context,
            scan.state,
            scan.input,
            (module, index, arity, uniq, incoming, tag, offset),
            scan.budget,
        )?);
        if !free.is_empty() {
            scan.push_frame(ParseFrame::LocalFun {
                tag,
                offset,
                depth,
                module: Some(module),
                arity,
                index,
                uniq,
                old_index: Some(old_index),
                old_uniq: Some(old_uniq),
                creator,
                free,
                filled,
            })?;
            return Ok(None);
        }
    }
    let creator = creator.ok_or(E::InvalidFlowValue)?;
    Ok(Some(scan.emit(
        N::ForeignLocalFun {
            module: module.id,
            arity,
            index,
            uniq,
            old_index,
            old_uniq,
            creator,
            free,
        },
        SummaryKind::Other,
        (tag, offset),
    )?))
}

fn start_local_fun(scan: &mut Scanner<'_, '_, '_, '_>, tag: u8, offset: usize) -> Result<(), E> {
    let depth = scan.next_depth;
    let header = scan.take(29)?;
    let bytes = scan.input.get(header).ok_or(E::InvalidFlowValue)?;
    let arity = *bytes.get(4).ok_or(E::InvalidFlowValue)?;
    let uniq = bytes
        .get(5..21)
        .ok_or(E::InvalidFlowValue)?
        .try_into()
        .map_err(|_| E::InvalidFlowValue)?;
    let index = i32::from_be_bytes(
        bytes
            .get(21..25)
            .ok_or(E::InvalidFlowValue)?
            .try_into()
            .map_err(|_| E::InvalidFlowValue)?,
    );
    let count = usize::try_from(u32::from_be_bytes(
        bytes
            .get(25..29)
            .ok_or(E::InvalidFlowValue)?
            .try_into()
            .map_err(|_| E::InvalidFlowValue)?,
    ))
    .map_err(|_| E::Limit)?;
    if count > 255 || scan.input.len() - scan.cursor < count + 18 {
        return Err(E::InvalidFlowValue);
    }
    let minimum = scan.budget.require_nodes(scan.counts.finished, count + 6)?;
    scan.counts.planned = scan.counts.planned.max(minimum);
    let free = scan.slots(count)?;
    scan.push_frame(ParseFrame::LocalFun {
        tag,
        offset,
        depth,
        module: None,
        arity,
        index,
        uniq,
        old_index: None,
        old_uniq: None,
        creator: None,
        free,
        filled: 0,
    })?;
    Ok(())
}
