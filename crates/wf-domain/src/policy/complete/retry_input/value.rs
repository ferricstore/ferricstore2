//! Fresh paid source selectors and scalar validation over borrowed input.

use super::{
    Backoff, Error, Key, PolicyRetryInputRejection as Reason, PolicySchemaCharge,
    PolicySchemaError, charge_schema,
};
use crate::policy::BackoffKind;

use super::super::flow_input::value::{
    PolicyInputCursor as Cursor, PolicyInputMap as Map, classify_map, select, unsigned,
};

pub(super) fn key_bytes(key: Key<'_>) -> Option<&[u8]> {
    match key {
        Key::Atom(text) => Some(text.as_bytes()),
        Key::Binary(bytes) => Some(bytes),
        Key::Other => None,
    }
}

pub(super) fn queried_key(
    key: Key<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<u16>, Error> {
    for (index, name) in [
        b"max_attempts".as_slice(),
        b"max_retries",
        b"backoff",
        b"exhausted_to",
        b"kind",
        b"base_ms",
        b"max_ms",
        b"jitter_pct",
    ]
    .into_iter()
    .enumerate()
    {
        if compare(key, name, charge)? {
            let offset = usize::from(matches!(key, Key::Binary(_))) * 8;
            return Ok(Some(1 << (index + offset)));
        }
    }
    Ok(None)
}

pub(super) fn retry_limit(
    selected: Option<Cursor<'_>>,
    fill_defaults: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Option<u16>, Reason>, Error> {
    if selected.is_none() && !fill_defaults {
        return Ok(Ok(None));
    }
    let maximum = match number(selected, 3, 1_000, Reason::MaxRetries, charge)? {
        Ok(value) => value,
        Err(reason) => return Ok(Err(reason)),
    };
    Ok(Ok(Some(u16::try_from(maximum).map_err(|_| {
        Error::Schema(PolicySchemaError::LengthOverflow)
    })?)))
}

pub(super) fn backoff(
    selected: Option<Cursor<'_>>,
    fill_defaults: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Option<Backoff>, Reason>, Error> {
    if selected.is_none() && !fill_defaults {
        return Ok(Ok(None));
    }
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let input = match selected {
        None => Map::Empty,
        Some(input) if input.is_nil() => Map::Empty,
        Some(input) => match classify_map(input, charge)? {
            Some(input) => input,
            None => return Ok(Err(Reason::BackoffShape)),
        },
    };
    let selected = select(input, b"kind", charge)?;
    let kind = match kind(selected, charge)? {
        Ok(kind) => kind,
        Err(reason) => return Ok(Err(reason)),
    };
    let selected = select(input, b"base_ms", charge)?;
    let base_ms = match number(selected, 1_000, 2_592_000_000, Reason::BaseMillis, charge)? {
        Ok(value) => value,
        Err(reason) => return Ok(Err(reason)),
    };
    let selected = select(input, b"max_ms", charge)?;
    let max_ms = match number(selected, 30_000, 2_592_000_000, Reason::MaxMillis, charge)? {
        Ok(value) => value,
        Err(reason) => return Ok(Err(reason)),
    };
    let selected = select(input, b"jitter_pct", charge)?;
    let jitter_pct = match number(selected, 20, 100, Reason::JitterPercent, charge)? {
        Ok(value) => {
            u8::try_from(value).map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?
        }
        Err(reason) => return Ok(Err(reason)),
    };
    Ok(Ok(Some(Backoff {
        kind,
        base_ms,
        max_ms,
        jitter_pct,
    })))
}

fn number(
    selected: Option<Cursor<'_>>,
    default: u64,
    maximum: u64,
    reason: Reason,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<u64, Reason>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let Some(selected) = selected else {
        return Ok(Ok(default));
    };
    Ok(unsigned(selected, maximum, charge)?.ok_or(reason))
}

fn kind(
    selected: Option<Cursor<'_>>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<BackoffKind, Reason>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let Some(selected) = selected else {
        return Ok(Ok(BackoffKind::Exponential));
    };
    if selected.is_nil() {
        return Ok(Err(Reason::BackoffKind));
    }
    let Some(bytes) = selected.key_bytes() else {
        return Ok(Err(Reason::BackoffKind));
    };
    let work = u64::try_from(bytes.len())
        .ok()
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(26))
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, work, 0).map_err(Error::Schema)?;
    Ok(match bytes {
        b"none" => Ok(BackoffKind::None),
        b"fixed" => Ok(BackoffKind::Fixed),
        b"linear" => Ok(BackoffKind::Linear),
        b"exponential" => Ok(BackoffKind::Exponential),
        _ => Err(Reason::BackoffKind),
    })
}

pub(super) fn target<'input>(
    selected: Option<Cursor<'input>>,
    fill_defaults: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Option<&'input [u8]>, Reason>, Error> {
    if selected.is_none() && !fill_defaults {
        return Ok(Ok(None));
    }
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let Some(selected) = selected else {
        return Ok(Ok(Some(b"failed")));
    };
    let Some(bytes) = selected.binary_bytes() else {
        return Ok(Err(Reason::ExhaustedTo));
    };
    if bytes.is_empty() {
        return Ok(Err(Reason::ExhaustedTo));
    }
    let work = bytes
        .len()
        .checked_add(7)
        .and_then(|n| u64::try_from(n).ok())
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, work, 0).map_err(Error::Schema)?;
    Ok(if bytes == b"running" {
        Err(Reason::RunningExhaustion)
    } else {
        Ok(Some(bytes))
    })
}

fn compare(key: Key<'_>, name: &[u8], charge: &mut PolicySchemaCharge) -> Result<bool, Error> {
    let Some(bytes) = key_bytes(key) else {
        return Ok(false);
    };
    let length = bytes
        .len()
        .checked_add(name.len())
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_visit(length, charge)?;
    Ok(bytes == name)
}

pub(super) fn charge_visit(length: usize, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    let work = u64::try_from(length)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, work, 0).map_err(Error::Schema)
}
