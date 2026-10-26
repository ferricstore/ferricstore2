//! No-allocation full structural pass before any request/item/key materialization.

use super::{CodecError, CodecLimits, claim, common, keys, policy, reader::Reader};

pub(super) fn preflight(encoded: &[u8], limits: CodecLimits) -> Result<(), CodecError> {
    super::value_batch::validate(encoded, limits)
}

pub(super) fn legacy_preflight(encoded: &[u8], limits: CodecLimits) -> Result<(), CodecError> {
    if encoded.len() > limits.admission.decision_bytes() {
        return Err(CodecError::ByteLimit);
    }
    let mut reader = Reader::new(encoded, limits);
    let header = common::header(&mut reader)?;
    if header.intent.version() == 8 {
        return Err(CodecError::UnsupportedVersion);
    }
    let scope = keys::scope_length(header.scope())?;
    let mut derived = scope;
    if header.operation_tag == 4 {
        super::public_mutation::preflight(&mut reader, &header)?;
        return reader.finish();
    }
    if header.operation_tag == 2 {
        policy::preflight(&mut reader)?;
        return reader.finish();
    }
    if header.operation_tag == 3 {
        claim::preflight(&mut reader)?;
        return reader.finish();
    }
    let count = common::count(&mut reader)?;
    for _ in 0..count {
        let item = common::item(&mut reader)?;
        keys::charge(
            &mut derived,
            keys::state_length(scope, item.partition.len(), item.id.len())?,
            limits.admission.decision_bytes(),
        )?;
        operation(&mut reader, item.tag, false)?;
    }
    reader.finish()
}

fn field(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    reader.field().map(|_| ())
}
fn guard(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    token(reader)?;
    reader.u64().map(|_| ())
}
fn token(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    reader
        .nonempty(reader.limits.admission.inline_bytes())
        .map(|_| ())
}
fn optional_field(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    reader.optional(field).map(|_| ())
}
fn optional_time(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    reader.optional(Reader::u64).map(|_| ())
}
fn claim(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    field(reader)?;
    field(reader)?;
    token(reader)?;
    reader.u64().map(|_| ())
}
fn terminal(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    guard(reader)?;
    optional_field(reader)?;
    optional_field(reader)
}
fn retry_override(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    reader.optional(Reader::u16)?;
    reader.optional(|reader| {
        if reader.byte()? > 3 {
            return Err(CodecError::UnsupportedTag);
        }
        reader.u64()?;
        reader.u64()?;
        reader.byte().map(|_| ())
    })?;
    optional_field(reader)
}

pub(super) fn operation(reader: &mut Reader<'_>, tag: u8, public: bool) -> Result<(), CodecError> {
    match tag {
        1 => {
            field(reader)?;
            field(reader)?;
            optional_time(reader)?;
            optional_field(reader)
        }
        2 | 3 => claim(reader),
        4 => {
            field(reader)?;
            field(reader)?;
            reader.u64()?;
            reader.optional(token)?;
            optional_time(reader)?;
            optional_field(reader)
        }
        5 | 6 => terminal(reader),
        7 => {
            guard(reader)?;
            if reader.byte()? > 1 {
                return Err(CodecError::UnsupportedTag);
            }
            reader.u64()?;
            optional_field(reader)?;
            optional_field(reader)
        }
        8 => {
            guard(reader)?;
            optional_time(reader)?;
            retry_override(reader)?;
            if !public {
                reader.u32()?;
            }
            optional_field(reader)?;
            optional_field(reader)
        }
        9 => {
            guard(reader)?;
            reader.u64().map(|_| ())
        }
        10 => {
            reader.u64()?;
            reader.optional(token)?;
            optional_field(reader)?;
            optional_time(reader)?;
            optional_field(reader)
        }
        _ => Err(CodecError::UnsupportedTag),
    }
}
