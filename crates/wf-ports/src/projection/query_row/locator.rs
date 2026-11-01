//! Original durable physical locator rules and exact flag/varint production.
use super::{
    I,
    budget::{Budget, add},
    model::{OriginalQueryLocator, OriginalQueryRecordError as E},
    primitive,
    record::RecordPlan,
    writer::Writer,
};
pub(super) struct LocatorPlan<'a> {
    source: u8,
    flags: u8,
    source_id: u64,
    raft: u64,
    offset: u64,
    value_size: u64,
    frame: Option<u64>,
    expiry: Option<u64>,
    generation: Option<u64>,
    checksum: &'a [u8],
    pub(super) length: usize,
}
pub(super) fn plan<'a>(
    input: OriginalQueryLocator<'_, 'a>,
    record: &RecordPlan<'_, '_>,
    expiry: u64,
    budget: &mut Budget,
) -> Result<LocatorPlan<'a>, E> {
    plan_identity(input, record.id, record.version, expiry, budget)
}
pub(super) fn plan_identity<'a>(
    input: OriginalQueryLocator<'_, 'a>,
    expected_id: &[u8],
    expected_version: u64,
    expiry: u64,
    budget: &mut Budget,
) -> Result<LocatorPlan<'a>, E> {
    let I::Map(rows) = input.fields else {
        return Err(E::InvalidRecord);
    };
    let flow_id = field(rows, "flow_id", budget)?;
    if let I::Binary(id) = flow_id {
        budget.spend(id.len())?;
    } else {
        return Err(E::InvalidRecord);
    }
    if !matches!(flow_id, I::Binary(id) if id == expected_id)
        || !matches!(field(rows, "kind", budget)?, I::Atom("state"))
    {
        return Err(E::InvalidRecord);
    }
    if primitive::uint(field(rows, "version", budget)?, budget)? != expected_version {
        return Err(E::InvalidRecord);
    }
    let raft = primitive::uint(field(rows, "raft_index", budget)?, budget)?;
    let offset = primitive::uint(field(rows, "offset", budget)?, budget)?;
    let value_size = primitive::uint(field(rows, "value_size", budget)?, budget)?;
    let frame = optional(field(rows, "frame_size", budget)?, budget)?;
    let source_expiry = optional(field(rows, "expire_at_ms", budget)?, budget)?;
    let generation = optional(field(rows, "segment_generation", budget)?, budget)?;
    let I::Binary(checksum) = field(rows, "checksum", budget)? else {
        return Err(E::InvalidRecord);
    };
    if checksum.len() != 32 || value_size == 0 {
        return Err(E::InvalidRecord);
    }
    if source_expiry.is_some_and(|v| v > 0 && (expiry == 0 || v < expiry)) {
        return Err(E::InvalidRecord);
    }
    let (source, source_id) = source(field(rows, "file_id", budget)?, budget)?;
    if source != 0 && (source_id != raft || generation.is_none() || frame.is_none_or(|v| v < 8)) {
        return Err(E::InvalidRecord);
    }
    let flags = u8::from(raft != source_id)
        | (u8::from(generation.is_some()) << 1)
        | if source_expiry.is_none() {
            4
        } else if source_expiry != Some(expiry) {
            8
        } else {
            0
        }
        | (u8::from(frame.is_some()) << 4);
    let mut result = LocatorPlan {
        source,
        flags,
        source_id,
        raft,
        offset,
        value_size,
        frame,
        expiry: source_expiry,
        generation,
        checksum,
        length: 0,
    };
    let mut out = Writer::measure();
    emit(&result, &mut out, budget)?;
    result.length = out.length;
    if result.length > 104 {
        return Err(E::InvalidRecord);
    }
    Ok(result)
}
pub(super) fn emit(plan: &LocatorPlan<'_>, out: &mut Writer, budget: &mut Budget) -> Result<(), E> {
    out.byte(plan.source, budget)?;
    out.byte(plan.flags, budget)?;
    out.varint(plan.source_id, budget)?;
    if plan.flags & 1 != 0 {
        out.varint(plan.raft, budget)?;
    }
    out.varint(plan.offset, budget)?;
    out.varint(plan.value_size, budget)?;
    if let Some(value) = plan.frame {
        out.varint(value, budget)?;
    }
    if plan.flags & 8 != 0 {
        out.varint(plan.expiry.ok_or(E::InvalidRecord)?, budget)?;
    }
    if let Some(value) = plan.generation {
        out.varint(value, budget)?;
    }
    out.put(plan.checksum, budget)
}
fn field<'r, 'a>(
    rows: &[(I<'r, 'a>, I<'r, 'a>)],
    name: &str,
    budget: &mut Budget,
) -> Result<I<'r, 'a>, E> {
    budget.spend(add(rows.len(), 1)?)?;
    Ok(rows
        .iter()
        .find_map(|&(key, value)| matches!(key, I::Atom(key) if key == name).then_some(value))
        .unwrap_or(I::Nil))
}
fn optional(value: I<'_, '_>, budget: &mut Budget) -> Result<Option<u64>, E> {
    if primitive::nil(value) {
        Ok(None)
    } else {
        primitive::uint(value, budget).map(Some)
    }
}
fn source(value: I<'_, '_>, budget: &mut Budget) -> Result<(u8, u64), E> {
    if let I::Tuple(pair) = value {
        let [tag, index] = pair else {
            return Err(E::InvalidRecord);
        };
        let tag = match tag {
            I::Atom("waraft_segment") => 1,
            I::Atom("waraft_projection") => 2,
            I::Atom("waraft_apply_projection") => 3,
            _ => return Err(E::InvalidRecord),
        };
        let index = primitive::uint(*index, budget)?;
        if index == 0 {
            return Err(E::InvalidRecord);
        }
        Ok((tag, index))
    } else {
        primitive::uint(value, budget).map(|id| (0, id))
    }
}
