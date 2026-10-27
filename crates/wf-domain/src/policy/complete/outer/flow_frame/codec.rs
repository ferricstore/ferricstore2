//! One measured final owner, prospectively paid writes and real backpatches.

use super::canonical::{self, FlowFrameCanonicalSlot};
use super::{
    AdmissionLimits, CompletePolicyRequestError, Error, OriginalFlowPolicyReplaceInput, PREFIX,
    PolicySchemaCharge, add, cap, overflow, visit,
};
use crate::policy::complete::flow_input::value::{
    PolicyInputCursor as Cursor, PolicyInputKind as Kind,
};

pub(super) enum FlowFrameSink<'out> {
    Measure {
        position: usize,
        maps: usize,
        records: usize,
    },
    Emit {
        output: &'out mut [u8],
        position: usize,
        maps: usize,
        intent: usize,
        side: usize,
        record: usize,
    },
}

impl FlowFrameSink<'_> {
    pub(super) fn position(&self) -> usize {
        match self {
            Self::Measure { position, .. } | Self::Emit { position, .. } => *position,
        }
    }
    fn maps(&self) -> usize {
        match self {
            Self::Measure { maps, .. } | Self::Emit { maps, .. } => *maps,
        }
    }
    pub(super) fn write(
        &mut self,
        bytes: &[u8],
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        visit(charge, bytes.len())?;
        let end = add(self.position(), bytes.len())?;
        match self {
            Self::Measure { position, .. } => *position = end,
            Self::Emit {
                output, position, ..
            } => {
                output
                    .get_mut(*position..end)
                    .ok_or(Error::Malformed)?
                    .copy_from_slice(bytes);
                *position = end;
            }
        }
        Ok(())
    }
    pub(super) fn u32(
        &mut self,
        value: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        self.write(
            &u32::try_from(value).map_err(|_| overflow())?.to_be_bytes(),
            charge,
        )
    }
    fn patch(
        &mut self,
        offset: usize,
        value: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        if let Self::Emit { output, .. } = self {
            visit(charge, 132)?;
            output
                .get_mut(offset..add(offset, 4)?)
                .ok_or(Error::Malformed)?
                .copy_from_slice(&u32::try_from(value).map_err(|_| overflow())?.to_be_bytes());
        }
        Ok(())
    }
    pub(super) fn raw(
        &mut self,
        bytes: &[u8],
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        visit(charge, 128)?;
        self.u32(bytes.len(), charge)?;
        self.write(bytes, charge)
    }
    fn side_root(&mut self, count: usize, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
        visit(charge, 128)?;
        let saved = self.position();
        if let Self::Emit { position, side, .. } = self {
            *position = *side;
        }
        self.u32(count, charge)?;
        for _ in 0..=count {
            self.u32(0, charge)?;
        }
        if let Self::Emit { position, .. } = self {
            *position = saved;
        }
        Ok(())
    }
    fn record(
        &mut self,
        node: usize,
        start: usize,
        length: usize,
        slots: &[FlowFrameCanonicalSlot],
        charge: &mut PolicySchemaCharge,
    ) -> Result<usize, Error> {
        visit(charge, 128)?;
        let id = self.maps();
        match self {
            Self::Measure { maps, records, .. } => {
                *maps = add(*maps, 1)?;
                *records = add(
                    *records,
                    add(8, length.checked_mul(8).ok_or_else(overflow)?)?,
                )?;
                for _ in 0..add(2, length.checked_mul(2).ok_or_else(overflow)?)? {
                    visit(charge, 4)?;
                }
            }
            Self::Emit { .. } => self.emit_record(node, start, length, slots, charge)?,
        }
        Ok(id)
    }
    fn emit_record(
        &mut self,
        node: usize,
        start: usize,
        length: usize,
        slots: &[FlowFrameCanonicalSlot],
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        let Self::Emit {
            position,
            maps,
            intent,
            side,
            record,
            ..
        } = self
        else {
            return Err(Error::Malformed);
        };
        let (saved, id, base, relative_node, relative_record) = (
            *position,
            *maps,
            *record,
            node.checked_sub(*intent).ok_or(Error::Malformed)?,
            record.checked_sub(*side).ok_or(Error::Malformed)?,
        );
        let table = add(*side, add(4, id.checked_mul(4).ok_or_else(overflow)?)?)?;
        *maps = add(*maps, 1)?;
        *record = add(
            *record,
            add(8, length.checked_mul(8).ok_or_else(overflow)?)?,
        )?;
        self.patch(table, relative_record, charge)?;
        self.patch(base, relative_node, charge)?;
        self.patch(add(base, 4)?, length, charge)?;
        if let Self::Emit { position, .. } = self {
            *position = add(base, 8)?;
        }
        for _ in 0..length {
            self.u32(0, charge)?;
        }
        for rank in 0..length {
            self.u32(slots[start + rank].source, charge)?;
            self.patch(
                add(
                    base,
                    add(
                        8,
                        slots[start + rank]
                            .source
                            .checked_mul(4)
                            .ok_or_else(overflow)?,
                    )?,
                )?,
                rank,
                charge,
            )?;
        }
        if let Self::Emit { position, .. } = self {
            *position = saved;
        }
        Ok(())
    }
    fn finish(
        &mut self,
        (end, count, frame_length): (usize, usize, usize),
        side_length: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        let Self::Emit {
            position,
            maps,
            record,
            ..
        } = self
        else {
            return Err(Error::Malformed);
        };
        if *position != end || *maps != count || *record != frame_length {
            return Err(Error::Malformed);
        }
        self.patch(
            add(end, add(4, count.checked_mul(4).ok_or_else(overflow)?)?)?,
            side_length,
            charge,
        )
    }
}

pub(super) fn prepare(
    input: &OriginalFlowPolicyReplaceInput<'_>,
    admission: AdmissionLimits,
    slots: &mut Vec<FlowFrameCanonicalSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<(Vec<u8>, usize, std::ops::Range<usize>), CompletePolicyRequestError> {
    let mut measure = FlowFrameSink::Measure {
        position: 0,
        maps: 0,
        records: 0,
    };
    let digest = super::write_header(input, 0, 0, &mut measure, charge)?;
    let begin = measure.position();
    intent(input, &mut measure, slots, charge)?;
    let end = measure.position();
    let count = measure.maps();
    measure.side_root(count, charge)?;
    let FlowFrameSink::Measure {
        position, records, ..
    } = measure
    else {
        return Err(Error::Malformed.into());
    };
    let side_length = add(add(8, count.checked_mul(4).ok_or_else(overflow)?)?, records)?;
    let frame_length = add(position, records)?;
    cap(frame_length, admission)?;
    let intent_length = end - begin;
    if intent_length > super::MAX_FRAME {
        return Err(Error::ByteLimit.into());
    }
    visit(charge, add(128, frame_length)?)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(frame_length)
        .map_err(|_| Error::AllocationFailure)?;
    visit(charge, frame_length)?;
    output.resize(frame_length, 0);
    let mut emit = FlowFrameSink::Emit {
        output: &mut output,
        position: 0,
        maps: 0,
        intent: begin,
        side: end,
        record: add(end, add(8, count.checked_mul(4).ok_or_else(overflow)?)?)?,
    };
    super::write_header(input, intent_length, side_length, &mut emit, charge)?;
    emit.side_root(count, charge)?;
    intent(input, &mut emit, slots, charge)?;
    emit.finish((end, count, frame_length), side_length, charge)?;
    visit(charge, add(128, intent_length)?)?;
    visit(charge, 160)?;
    visit(charge, add(128, frame_length)?)?;
    Ok((output, digest, begin..end))
}

fn intent(
    input: &OriginalFlowPolicyReplaceInput<'_>,
    sink: &mut FlowFrameSink<'_>,
    slots: &mut Vec<FlowFrameCanonicalSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    visit(charge, 128)?;
    sink.write(PREFIX, charge)?;
    sink.raw(input.scope.tenant().as_bytes(), charge)?;
    sink.raw(input.policy_key, charge)?;
    node(Cursor::Flow(input.expected), sink, slots, charge)?;
    node(Cursor::Flow(input.patch), sink, slots, charge)
}

fn node(
    input: Cursor<'_>,
    sink: &mut FlowFrameSink<'_>,
    slots: &mut Vec<FlowFrameCanonicalSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let mut small = [0; 16];
    let integer = if input.kind() == Kind::Integer {
        Some(canonical::magnitude(input, &mut small, charge)?)
    } else {
        None
    };
    visit(charge, 128)?;
    let begin = sink.position();
    sink.u32(0, charge)?;
    if let Some((negative, magnitude)) = integer {
        sink.write(&[1], charge)?;
        sink.write(&[u8::from(negative)], charge)?;
        sink.u32(magnitude.len(), charge)?;
        sink.write(magnitude, charge)?;
    } else if input.is_nil() {
        sink.write(&[0], charge)?;
    } else if let Some(bytes) = input.atom_bytes().or_else(|| input.binary_bytes()) {
        sink.write(&[if input.is_atom() { 2 } else { 3 }], charge)?;
        sink.u32(bytes.len(), charge)?;
        sink.write(bytes, charge)?;
    } else {
        let shape = if matches!(input.kind(), Kind::List | Kind::ImproperList) {
            canonical::list_shape(input, charge)?
        } else {
            (input.len(), true)
        };
        sink.write(
            &[match input.kind() {
                Kind::Map => 4,
                Kind::List | Kind::ImproperList => {
                    if shape.1 {
                        5
                    } else {
                        7
                    }
                }
                Kind::Tuple => 6,
                _ => return Err(Error::UnsupportedTag),
            }],
            charge,
        )?;
        compound(input, (begin, shape), sink, slots, charge)?;
    }
    sink.patch(begin, sink.position() - begin, charge)
}

fn compound(
    input: Cursor<'_>,
    fields: (usize, (usize, bool)),
    sink: &mut FlowFrameSink<'_>,
    slots: &mut Vec<FlowFrameCanonicalSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let (begin, (length, proper)) = fields;
    if input.kind() == Kind::Map {
        return map(input, begin, sink, slots, charge);
    }
    sink.u32(length, charge)?;
    let table = sink.position();
    let children = add(length, usize::from(!proper))?;
    for _ in 0..=children {
        sink.u32(0, charge)?;
    }
    let (mut cursor, mut index) = (input, 0);
    for child in 0..children {
        sink.patch(table + 4 * child, sink.position() - begin, charge)?;
        let value = if input.kind() == Kind::Tuple {
            input.child(child).ok_or(Error::Malformed)?
        } else {
            canonical::next(&mut cursor, &mut index, charge)?.unwrap_or(cursor)
        };
        node(value, sink, slots, charge)?;
    }
    sink.patch(table + 4 * children, sink.position() - begin, charge)
}

fn map(
    input: Cursor<'_>,
    begin: usize,
    sink: &mut FlowFrameSink<'_>,
    slots: &mut Vec<FlowFrameCanonicalSlot>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let start = canonical::prepare(input, slots, charge)?;
    let length = input.len();
    sink.u32(sink.maps(), charge)?;
    sink.u32(length, charge)?;
    let table = sink.position();
    for _ in 0..=length * 2 {
        sink.u32(0, charge)?;
    }
    sink.record(begin, start, length, slots, charge)?;
    for rank in 0..length {
        let (key, value) = canonical::row(input, slots[start + rank].source, charge)?;
        sink.patch(table + rank * 8, sink.position() - begin, charge)?;
        node(key, sink, slots, charge)?;
        sink.patch(table + rank * 8 + 4, sink.position() - begin, charge)?;
        node(value, sink, slots, charge)?;
    }
    sink.patch(table + length * 8, sink.position() - begin, charge)?;
    slots.truncate(start);
    Ok(())
}
