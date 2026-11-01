//! Uncapped source-shape views and separately capped eligible key construction.

use crate::blob::{
    OriginalValueBuildError, OriginalValueBuildError as E, OriginalValuePartition,
    OriginalValueRef, OriginalValueRefKind,
};

mod base64;
mod state;
mod tag;

pub use state::{owns_original_state_key, parse_original_state_key, render_original_state_key};

const MAX_KEY_BYTES: usize = 65_535;
const MAX_VALUE_VERSION: u64 = 9_007_199_254_740_991;

/// Inspect source binary reference shape, without an I/O or constructor cap.
///
/// Named components remain encoded views. Success grants no stored availability.
#[must_use]
pub fn parse_original_value_ref(input: &[u8]) -> Option<OriginalValueRef<'_>> {
    let (tag_prefix, rest) = split_tag(input, *b"}:v:")?;
    let (kind, rest) = rest.split_first()?;
    let rest = rest.strip_prefix(b":")?;
    if *kind == b'n' {
        let mut fields = rest.split(|byte| *byte == b':');
        let owner = fields.next()?;
        let name = fields.next()?;
        let version = fields.next()?;
        if fields.next().is_some()
            || owner.is_empty()
            || name.is_empty()
            || !base64::valid(owner)
            || !base64::valid(name)
        {
            return None;
        }
        return Some(OriginalValueRef::Named {
            tag_prefix,
            owner_component: owner,
            name_component: name,
            version: parse_version(version)?,
        });
    }
    let kind = match *kind {
        b'p' => OriginalValueRefKind::Payload,
        b'r' => OriginalValueRefKind::Result,
        b'e' => OriginalValueRefKind::Error,
        b's' => OriginalValueRefKind::Shared,
        _ => return None,
    };
    let separator = rest.iter().rposition(|byte| *byte == b':')?;
    let id = rest.get(..separator)?;
    if id.is_empty() {
        return None;
    }
    Some(OriginalValueRef::Regular {
        tag_prefix,
        kind,
        id,
        version: parse_version(rest.get(separator.checked_add(1)?..)?)?,
    })
}

/// Build a canonical eligible regular reference with one final owned buffer.
///
/// # Errors
///
/// Invalid semantic fields refuse before cap checks. Arithmetic/output limits
/// and actual reservation failures remain distinct operational errors.
pub fn render_original_value_ref(
    id: &[u8],
    kind: OriginalValueRefKind,
    version: u64,
    partition: OriginalValuePartition<'_>,
    maximum: usize,
) -> Result<Vec<u8>, OriginalValueBuildError> {
    if id.is_empty() || version > MAX_VALUE_VERSION {
        return Err(E::InvalidInput);
    }
    let fixed = id
        .len()
        .checked_add(decimal_length(version))
        .and_then(|length| length.checked_add(6))
        .ok_or(E::Limit)?;
    let maximum = maximum.min(MAX_KEY_BYTES);
    let remaining = maximum.checked_sub(fixed).ok_or(E::Limit)?;
    let mut tag_bytes = [0_u8; 49];
    let tag_length = tag::render_tag(partition, id, remaining, &mut tag_bytes)?;
    let length = fixed.checked_add(tag_length).ok_or(E::Limit)?;
    if length > maximum {
        return Err(E::Limit);
    }
    let mut output = reserve_key(length)?;
    let mut cursor = 0;
    put(
        &mut output,
        &mut cursor,
        tag_bytes.get(..tag_length).ok_or(E::Limit)?,
    )?;
    put(&mut output, &mut cursor, b":v:")?;
    let kind = match kind {
        OriginalValueRefKind::Payload => b'p',
        OriginalValueRefKind::Result => b'r',
        OriginalValueRefKind::Error => b'e',
        OriginalValueRefKind::Shared => b's',
    };
    put(&mut output, &mut cursor, &[kind])?;
    put(&mut output, &mut cursor, b":")?;
    put(&mut output, &mut cursor, id)?;
    put(&mut output, &mut cursor, b":")?;
    put_version(&mut output, &mut cursor, version)?;
    if cursor != length {
        return Err(E::Limit);
    }
    Ok(output)
}

/// Build a canonical named reference, encoding opaque owner/name exactly once.
///
/// # Errors
///
/// Invalid semantic fields refuse before cap checks. Arithmetic/output limits
/// and actual reservation failures remain distinct operational errors.
pub fn render_original_named_value_ref(
    owner: &[u8],
    name: &[u8],
    version: u64,
    partition: OriginalValuePartition<'_>,
    maximum: usize,
) -> Result<Vec<u8>, OriginalValueBuildError> {
    if owner.is_empty() || name.is_empty() || version > MAX_VALUE_VERSION {
        return Err(E::InvalidInput);
    }
    let owner_length = base64::encoded_len(owner.len())?;
    let name_length = base64::encoded_len(name.len())?;
    let fixed = owner_length
        .checked_add(name_length)
        .and_then(|length| length.checked_add(decimal_length(version)))
        .and_then(|length| length.checked_add(7))
        .ok_or(E::Limit)?;
    let maximum = maximum.min(MAX_KEY_BYTES);
    let remaining = maximum.checked_sub(fixed).ok_or(E::Limit)?;
    let mut tag_bytes = [0_u8; 49];
    let tag_length = tag::render_tag(partition, owner, remaining, &mut tag_bytes)?;
    let length = fixed.checked_add(tag_length).ok_or(E::Limit)?;
    if length > maximum {
        return Err(E::Limit);
    }
    let mut output = reserve_key(length)?;
    let mut cursor = 0;
    put(
        &mut output,
        &mut cursor,
        tag_bytes.get(..tag_length).ok_or(E::Limit)?,
    )?;
    put(&mut output, &mut cursor, b":v:n:")?;
    for (input, size) in [(owner, owner_length), (name, name_length)] {
        let end = cursor.checked_add(size).ok_or(E::Limit)?;
        let written = base64::encode_into(input, output.get_mut(cursor..end).ok_or(E::Limit)?)?;
        if written != size {
            return Err(E::Limit);
        }
        cursor = end;
        put(&mut output, &mut cursor, b":")?;
    }
    put_version(&mut output, &mut cursor, version)?;
    if cursor != length {
        return Err(E::Limit);
    }
    Ok(output)
}

fn split_tag(input: &[u8], marker: [u8; 4]) -> Option<(&[u8], &[u8])> {
    let rest = input.strip_prefix(b"f:{")?;
    let position = rest
        .get(..rest.len().min(49))?
        .windows(4)
        .position(|bytes| bytes == marker)?;
    if !tag::valid_tag(rest.get(..position)?) {
        return None;
    }
    let prefix_end = 3_usize.checked_add(position)?.checked_add(1)?;
    let rest_start = prefix_end.checked_add(3)?;
    Some((input.get(..prefix_end)?, input.get(rest_start..)?))
}

fn parse_version(input: &[u8]) -> Option<u64> {
    if input.is_empty() || (input.len() > 1 && input.first() == Some(&b'0')) {
        return None;
    }
    let mut value = 0_u64;
    for byte in input {
        let digit = byte.checked_sub(b'0')?;
        if digit > 9 {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u64::from(digit))?;
        if value > MAX_VALUE_VERSION {
            return None;
        }
    }
    Some(value)
}

fn decimal_length(mut value: u64) -> usize {
    let mut length = 1;
    while value >= 10 {
        value /= 10;
        length += 1;
    }
    length
}

fn reserve_key(length: usize) -> Result<Vec<u8>, E> {
    if length > MAX_KEY_BYTES {
        return Err(E::Limit);
    }
    let mut output = Vec::new();
    output.try_reserve_exact(length).map_err(E::ReserveFailed)?;
    output.resize(length, 0);
    Ok(output)
}

fn put(output: &mut [u8], cursor: &mut usize, input: &[u8]) -> Result<(), E> {
    let end = cursor.checked_add(input.len()).ok_or(E::Limit)?;
    output
        .get_mut(*cursor..end)
        .ok_or(E::Limit)?
        .copy_from_slice(input);
    *cursor = end;
    Ok(())
}

fn put_version(output: &mut [u8], cursor: &mut usize, mut value: u64) -> Result<(), E> {
    let mut digits = [0_u8; 20];
    let mut start = digits.len();
    loop {
        start = start.checked_sub(1).ok_or(E::Limit)?;
        let digit = u8::try_from(value % 10).map_err(|_| E::Limit)?;
        *digits.get_mut(start).ok_or(E::Limit)? = b'0' + digit;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    put(output, cursor, digits.get(start..).ok_or(E::Limit)?)
}
