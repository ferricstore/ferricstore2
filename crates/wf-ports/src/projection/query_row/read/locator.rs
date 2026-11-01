//! Exact locator decoding and shared producer validation of typed locations.
use super::super::{
    I,
    budget::Budget,
    locator::{self, LocatorPlan},
    model::{OriginalQueryLocator, OriginalQueryRecordError as E},
};
use super::{cursor::Cursor, frame::Frame, model::OriginalQueryLocation};
pub(super) fn parse<'a>(frame: &Frame<'a>, b: &mut Budget) -> Result<OriginalQueryLocation<'a>, E> {
    let mut c = Cursor::new(frame.locator);
    let tag = c.byte(b)?;
    let flags = c.byte(b)?;
    if flags & !31 != 0 || flags & 0x0c == 0x0c || tag > 3 {
        return Err(E::InvalidRecord);
    }
    let source_id = c.varint(b)?;
    let raft_index = if flags & 1 == 0 {
        source_id
    } else {
        c.varint(b)?
    };
    let offset = c.varint(b)?;
    let value_size = c.varint(b)?;
    let frame_size = optional(&mut c, flags & 16 != 0, b)?;
    let expire_at_ms = if flags & 4 != 0 {
        None
    } else if flags & 8 != 0 {
        Some(c.varint(b)?)
    } else {
        Some(frame.expiry)
    };
    let segment_generation = optional(&mut c, flags & 2 != 0, b)?;
    let checksum = c.take(32, b)?;
    c.finish()?;
    let location = OriginalQueryLocation {
        flow_id: frame.id,
        version: frame.version,
        raft_index,
        source_tag: tag,
        source_id,
        offset,
        value_size,
        frame_size,
        checksum,
        expire_at_ms,
        segment_generation,
    };
    plan(&location, frame.expiry, b)?;
    Ok(location)
}
fn optional(c: &mut Cursor<'_>, present: bool, b: &mut Budget) -> Result<Option<u64>, E> {
    if present {
        c.varint(b).map(Some)
    } else {
        Ok(None)
    }
}
pub(super) fn plan<'a>(
    l: &OriginalQueryLocation<'a>,
    row_expiry: u64,
    b: &mut Budget,
) -> Result<LocatorPlan<'a>, E> {
    let tag = match l.source_tag {
        0 => "",
        1 => "waraft_segment",
        2 => "waraft_projection",
        3 => "waraft_apply_projection",
        _ => return Err(E::InvalidRecord),
    };
    let pair = [I::Atom(tag), I::Integer(i128::from(l.source_id))];
    let file = if l.source_tag == 0 {
        I::Integer(i128::from(l.source_id))
    } else {
        I::Tuple(&pair)
    };
    let fields = [
        (I::Atom("flow_id"), I::Binary(l.flow_id)),
        (I::Atom("kind"), I::Atom("state")),
        (I::Atom("version"), I::Integer(i128::from(l.version))),
        (I::Atom("raft_index"), I::Integer(i128::from(l.raft_index))),
        (I::Atom("file_id"), file),
        (I::Atom("offset"), I::Integer(i128::from(l.offset))),
        (I::Atom("value_size"), I::Integer(i128::from(l.value_size))),
        (I::Atom("frame_size"), number(l.frame_size)),
        (I::Atom("checksum"), I::Binary(l.checksum)),
        (I::Atom("expire_at_ms"), number(l.expire_at_ms)),
        (I::Atom("segment_generation"), number(l.segment_generation)),
    ];
    locator::plan_identity(
        OriginalQueryLocator {
            fields: I::Map(&fields),
        },
        l.flow_id,
        l.version,
        row_expiry,
        b,
    )
}
fn number(value: Option<u64>) -> I<'static, 'static> {
    value.map_or(I::Nil, |v| I::Integer(i128::from(v)))
}
