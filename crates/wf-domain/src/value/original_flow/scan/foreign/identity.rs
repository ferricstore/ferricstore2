//! Source-ordered identity fields and local reference normalization.

use crate::value::original_flow::context::local_node;
use crate::value::original_flow::scan::frame::ParseFrame;
use crate::value::original_flow::scan::{Scanner, SummaryKind, TermSummary};
use crate::value::original_flow::{OriginalFlowValueError as E, OriginalValueNode as N};

pub(in crate::value::original_flow) fn finish_identity(
    scan: &mut Scanner<'_, '_, '_, '_>,
    frame: &ParseFrame,
    child: TermSummary,
) -> Result<Option<TermSummary>, E> {
    let &ParseFrame::Identity {
        tag, offset, words, ..
    } = frame
    else {
        return Err(E::InvalidFlowValue);
    };
    let reference = matches!(tag, 101 | 114 | 90);
    let pid = matches!(tag, 103 | 88);
    let old = matches!(tag, 101 | 102 | 103 | 114);
    let (creation, id, serial, mut range) = identity_fields(scan, tag, words)?;
    if old && creation > 3 {
        return Err(E::InvalidFlowValue);
    }
    if reference && words == 0 {
        return Err(E::UnsupportedProfile { tag, offset });
    }
    let first = if reference {
        scan.budget.spend(4)?;
        u32::from_be_bytes(
            scan.input
                .get(range.start..range.start + 4)
                .ok_or(E::InvalidFlowValue)?
                .try_into()
                .map_err(|_| E::InvalidFlowValue)?,
        )
    } else {
        0
    };
    if matches!(tag, 101 | 114) && first >= 1 << 18 {
        return Err(E::InvalidFlowValue);
    }
    let local = local_node(
        scan.context,
        scan.state,
        scan.input,
        child,
        (creation, (tag, offset)),
        scan.budget,
    )?;
    let node = if reference {
        if local {
            range = normalize_local_reference(scan, words, first, range)?;
        }
        N::ForeignReference {
            node: child.id,
            creation,
            words: range,
            local_to_context: local,
        }
    } else if pid {
        let id = u32::try_from(id).map_err(|_| E::InvalidFlowValue)?;
        if local && id > 0x0fff_ffff {
            return Err(E::InvalidFlowValue);
        }
        N::ForeignPid {
            node: child.id,
            id,
            serial,
            creation,
            local_to_context: local,
        }
    } else {
        if local && id >= 1_u64 << 60 {
            return Err(E::InvalidFlowValue);
        }
        N::ForeignPort {
            node: child.id,
            id,
            creation,
            local_to_context: local,
        }
    };
    let kind = if pid {
        SummaryKind::Pid
    } else {
        SummaryKind::Other
    };
    Ok(Some(scan.emit(node, kind, (tag, offset))?))
}

fn identity_fields(
    scan: &mut Scanner<'_, '_, '_, '_>,
    tag: u8,
    words: usize,
) -> Result<(u32, u64, u32, std::ops::Range<usize>), E> {
    let reference = matches!(tag, 101 | 114 | 90);
    let pid = matches!(tag, 103 | 88);
    let old = matches!(tag, 101 | 102 | 103 | 114);
    let width = match tag {
        114 | 90 => 0,
        103 | 88 | 120 => 8,
        101 | 102 | 89 => 4,
        _ => return Err(E::InvalidFlowValue),
    };
    let before = scan.take(width)?;
    let creation = scan.take(if old { 1 } else { 4 })?;
    let bytes = scan.input.get(creation).ok_or(E::InvalidFlowValue)?;
    let creation = if old {
        u32::from(*bytes.first().ok_or(E::InvalidFlowValue)?)
    } else {
        u32::from_be_bytes(bytes.try_into().map_err(|_| E::InvalidFlowValue)?)
    };
    if reference {
        let range = if tag == 101 {
            before
        } else {
            scan.take(words.checked_mul(4).ok_or(E::Limit)?)?
        };
        return Ok((creation, 0, 0, range));
    }
    let bytes = scan.input.get(before).ok_or(E::InvalidFlowValue)?;
    let id = if tag == 120 {
        u64::from_be_bytes(bytes.try_into().map_err(|_| E::InvalidFlowValue)?)
    } else {
        u64::from(u32::from_be_bytes(
            bytes
                .get(..4)
                .ok_or(E::InvalidFlowValue)?
                .try_into()
                .map_err(|_| E::InvalidFlowValue)?,
        ))
    };
    let serial = if pid {
        u32::from_be_bytes(
            bytes
                .get(4..8)
                .ok_or(E::InvalidFlowValue)?
                .try_into()
                .map_err(|_| E::InvalidFlowValue)?,
        )
    } else {
        0
    };
    Ok((creation, id, serial, 0..0))
}

fn normalize_local_reference(
    scan: &mut Scanner<'_, '_, '_, '_>,
    words: usize,
    first: u32,
    mut range: std::ops::Range<usize>,
) -> Result<std::ops::Range<usize>, E> {
    if first >= 1 << 18 {
        return Err(E::InvalidFlowValue);
    }
    let mut pid_reference = false;
    if words == 5 {
        scan.budget.spend(4)?;
        let second = u32::from_be_bytes(
            scan.input
                .get(range.start + 4..range.start + 8)
                .ok_or(E::InvalidFlowValue)?
                .try_into()
                .map_err(|_| E::InvalidFlowValue)?,
        );
        pid_reference = second & (1 << 16) != 0;
        if pid_reference {
            scan.budget.spend(4)?;
            let embedded = u32::from_be_bytes(
                scan.input
                    .get(range.start + 12..range.start + 16)
                    .ok_or(E::InvalidFlowValue)?
                    .try_into()
                    .map_err(|_| E::InvalidFlowValue)?,
            );
            if embedded & 15 != 3 {
                return Err(E::InvalidFlowValue);
            }
        }
    }
    range.end = range.start + 4 * words.min(if pid_reference { 5 } else { 3 });
    Ok(range)
}
