//! One measured final owner, prospectively paid writes and real backpatches.

use super::canonical;
use super::value::PolicySetFrameValue as Cursor;
use super::{
    Error, OriginalPolicySetFrameInput, OriginalPolicySetMapOrders, PREFIX, PolicySchemaCharge,
    add, overflow, visit,
};
use crate::policy::complete::flow_input::value::PolicyInputKind as Kind;

pub(super) enum PolicySetFrameSink<'out> {
    Measure {
        position: usize,
        maps: usize,
        records: usize,
        payload: usize,
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

impl PolicySetFrameSink<'_> {
    pub(super) fn position(&self) -> usize {
        match self {
            Self::Measure { position, .. } | Self::Emit { position, .. } => *position,
        }
    }
    pub(super) fn maps(&self) -> usize {
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
    pub(super) fn patch(
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
    pub(super) fn side_root(
        &mut self,
        count: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
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
        length: usize,
        orders: &OriginalPolicySetMapOrders<'_>,
        charge: &mut PolicySchemaCharge,
    ) -> Result<usize, Error> {
        visit(charge, 128)?;
        let id = self.maps();
        let (_, inverse) =
            orders.patch_record(u32::try_from(id).map_err(|_| overflow())?, length, charge)?;
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
            Self::Emit { .. } => self.emit_record(node, inverse, charge)?,
        }
        Ok(id)
    }
    fn payload(&mut self, length: usize) -> Result<(), Error> {
        if let Self::Measure { payload, .. } = self {
            *payload = add(*payload, length)?;
            if *payload > super::MAX_PATCH {
                return Err(Error::ByteLimit);
            }
        }
        Ok(())
    }

    fn emit_record(
        &mut self,
        node: usize,
        inverse: &[u32],
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        let length = inverse.len();
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
        for (rank, &source) in inverse.iter().enumerate() {
            self.u32(usize::try_from(source).map_err(|_| overflow())?, charge)?;
            self.patch(
                add(
                    base,
                    add(
                        8,
                        usize::try_from(source)
                            .map_err(|_| overflow())?
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
}

pub(super) fn intent(
    input: &OriginalPolicySetFrameInput<'_, '_, '_>,
    sink: &mut PolicySetFrameSink<'_>,
    slots: &mut Vec<u32>,
    charge: &mut PolicySchemaCharge,
) -> Result<std::ops::Range<usize>, Error> {
    visit(charge, 128)?;
    sink.write(PREFIX, charge)?;
    sink.raw(input.scope.tenant().as_bytes(), charge)?;
    sink.raw(input.prepared.policy_key(), charge)?;
    sink.write(&[u8::from(input.prepared.replace())], charge)?;
    sink.write(
        &[u8::from(input.prepared.response_state().is_some())],
        charge,
    )?;
    if let Some(state) = input.prepared.response_state() {
        sink.raw(state, charge)?;
    }
    sink.write(
        &[u8::from(input.prepared.expected_generation().is_some())],
        charge,
    )?;
    if let Some(expected) = input.prepared.expected_generation() {
        sink.write(&expected.to_be_bytes(), charge)?;
    }
    let start = sink.position();
    node(
        Cursor::Prepared(input.prepared.patch()),
        sink,
        slots,
        input.orders,
        charge,
    )?;
    Ok(start..sink.position())
}

fn node(
    input: Cursor<'_>,
    sink: &mut PolicySetFrameSink<'_>,
    slots: &mut Vec<u32>,
    orders: &OriginalPolicySetMapOrders<'_>,
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
        sink.payload(magnitude.len())?;
        sink.write(&[1], charge)?;
        sink.write(&[u8::from(negative)], charge)?;
        sink.u32(magnitude.len(), charge)?;
        sink.write(magnitude, charge)?;
    } else if input.is_nil() {
        sink.write(&[0], charge)?;
    } else if let Some(bytes) = input.atom_bytes().or_else(|| input.binary_bytes()) {
        sink.payload(bytes.len())?;
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
        compound(input, (begin, shape), sink, slots, orders, charge)?;
    }
    sink.patch(begin, sink.position() - begin, charge)
}

fn compound(
    input: Cursor<'_>,
    fields: (usize, (usize, bool)),
    sink: &mut PolicySetFrameSink<'_>,
    slots: &mut Vec<u32>,
    orders: &OriginalPolicySetMapOrders<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let (begin, (length, proper)) = fields;
    if input.kind() == Kind::Map {
        return map(input, begin, sink, slots, orders, charge);
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
        node(value, sink, slots, orders, charge)?;
    }
    sink.patch(table + 4 * children, sink.position() - begin, charge)
}

fn map(
    input: Cursor<'_>,
    begin: usize,
    sink: &mut PolicySetFrameSink<'_>,
    slots: &mut Vec<u32>,
    orders: &OriginalPolicySetMapOrders<'_>,
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
    sink.record(begin, length, orders, charge)?;
    for rank in 0..length {
        let (key, value) = canonical::row(
            input,
            usize::try_from(slots[start + rank]).map_err(|_| overflow())?,
            charge,
        )?;
        sink.patch(table + rank * 8, sink.position() - begin, charge)?;
        node(key, sink, slots, orders, charge)?;
        sink.patch(table + rank * 8 + 4, sink.position() - begin, charge)?;
        node(value, sink, slots, orders, charge)?;
    }
    sink.patch(table + length * 8, sink.position() - begin, charge)?;
    slots.truncate(start);
    Ok(())
}
