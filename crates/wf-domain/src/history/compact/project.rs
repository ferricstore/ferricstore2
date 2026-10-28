//! Flat protocol fields borrow payload/context bytes and own only decimal text.

use std::borrow::Cow;

use super::{
    OriginalCompactHistoryContext as C, OriginalCompactHistoryError as E,
    OriginalCompactHistoryField as F,
    budget::{Budget, add},
    decode::Parsed,
    primitives::{DecodeError, Reader},
};

pub(super) fn run<'a>(
    bytes: &'a [u8],
    parsed: &Parsed<'a>,
    context: C<'a>,
    budget: &mut Budget,
) -> Result<Vec<F<'a>>, E> {
    let count = add(21, parsed.metadata_valid)?;
    let mut fields = budget.reserve(count)?;
    binary(&mut fields, b"event", parsed.event, budget)?;
    integer(&mut fields, b"version", parsed.version, false, budget)?;
    integer(&mut fields, b"at", parsed.at, false, budget)?;
    string(&mut fields, b"id", context.id, budget)?;
    string(&mut fields, b"type", context.workflow_type, budget)?;
    string(&mut fields, b"state", parsed.state, budget)?;
    for (key, value) in [
        b"priority".as_slice(),
        b"attempts",
        b"fencing_token",
        b"created_at_ms",
        b"updated_at_ms",
    ]
    .into_iter()
    .zip(parsed.numbers)
    {
        integer(&mut fields, key, value, false, budget)?;
    }
    integer(
        &mut fields,
        b"next_run_at_ms",
        parsed.numbers[5],
        true,
        budget,
    )?;
    integer(
        &mut fields,
        b"lease_deadline_ms",
        parsed.numbers[6],
        true,
        budget,
    )?;
    string(&mut fields, b"lease_owner", parsed.references[0], budget)?;
    string(&mut fields, b"payload_ref", parsed.references[1], budget)?;
    string(
        &mut fields,
        b"parent_flow_id",
        context.parent_flow_id,
        budget,
    )?;
    string(&mut fields, b"root_flow_id", context.root_flow_id, budget)?;
    string(
        &mut fields,
        b"correlation_id",
        context.correlation_id,
        budget,
    )?;
    string(&mut fields, b"result_ref", parsed.references[2], budget)?;
    string(&mut fields, b"error_ref", parsed.references[3], budget)?;
    string(
        &mut fields,
        b"rewound_to_event_id",
        parsed.references[4],
        budget,
    )?;
    let suffix = bytes.get(parsed.metadata_start..).ok_or(E::InvalidInput)?;
    let mut reader = Reader::new(suffix);
    for _ in 0..parsed.metadata_count {
        budget.spend(1)?;
        let key = reader.binary(budget).map_err(operational)?;
        let value = reader.binary(budget).map_err(operational)?;
        if let (Some(key), Some(value)) = (key, value) {
            binary(&mut fields, key, Some(value), budget)?;
        }
    }
    if fields.len() != count {
        return Err(E::InvalidInput);
    }
    Ok(fields)
}

fn operational(error: DecodeError) -> E {
    match error {
        DecodeError::Malformed => E::InvalidInput,
        DecodeError::Operational(error) => error,
    }
}

fn string<'a>(
    fields: &mut Vec<F<'a>>,
    key: &'a [u8],
    value: Option<&'a [u8]>,
    budget: &mut Budget,
) -> Result<(), E> {
    binary(fields, key, Some(value.unwrap_or(b"")), budget)
}

fn binary<'a>(
    fields: &mut Vec<F<'a>>,
    key: &'a [u8],
    value: Option<&'a [u8]>,
    budget: &mut Budget,
) -> Result<(), E> {
    budget.output(add(key.len(), value.map_or(0, <[u8]>::len))?)?;
    insert(fields, key, value.map(Cow::Borrowed), budget)
}

fn integer<'a>(
    fields: &mut Vec<F<'a>>,
    key: &'a [u8],
    value: Option<u128>,
    optional: bool,
    budget: &mut Budget,
) -> Result<(), E> {
    if optional && value.is_none() {
        return binary(fields, key, Some(b""), budget);
    }
    budget.spend(41)?;
    let mut scratch = [0; 40];
    let mut start = scratch.len();
    let mut value = value.unwrap_or(0);
    loop {
        start -= 1;
        scratch[start] = b'0' + u8::try_from(value % 10).map_err(|_| E::InvalidInput)?;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let text = &scratch[start..];
    budget.output(add(key.len(), text.len())?)?;
    let mut owned = budget.reserve(text.len())?;
    budget.spend(text.len())?;
    owned.extend_from_slice(text);
    insert(fields, key, Some(Cow::Owned(owned)), budget)
}

fn insert<'a>(
    fields: &mut Vec<F<'a>>,
    key: &'a [u8],
    value: Option<Cow<'a, [u8]>>,
    budget: &mut Budget,
) -> Result<(), E> {
    budget.spend(1)?;
    if fields.len() == fields.capacity() {
        return Err(E::InvalidInput);
    }
    fields.push(F {
        key: Cow::Borrowed(key),
        value,
    });
    Ok(())
}
