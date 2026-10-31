//! Complete physical identities for exclusive recovery; not a public query API.

use super::{StateValueError, components, copy_components, expected, header};
use crate::state::value::{canonical_history_position, finish, read_u8, read_u64, read_u128};
use wf_types::identity::{GroupId, RecoveryId};

pub(crate) struct DecodedStateKey {
    family: u8,
    rank: u8,
    number: u128,
    bytes: Vec<u8>,
    ranges: [(usize, usize); 7],
}

impl DecodedStateKey {
    pub(crate) const fn family(&self) -> u8 {
        self.family
    }
    pub(crate) const fn rank(&self) -> u8 {
        self.rank
    }
    pub(crate) const fn number(&self) -> u128 {
        self.number
    }
    pub(crate) fn part(&self, index: usize) -> &[u8] {
        let (start, length) = self.ranges[index];
        &self.bytes[start..start + length]
    }
}

pub(crate) fn decode_state_key(
    bytes: &[u8],
    group: &GroupId,
    recovery: &RecoveryId,
    maximum_wire_bytes: usize,
    maximum_owned_bytes: usize,
) -> Result<DecodedStateKey, StateValueError> {
    let family = *bytes.get(6).ok_or(StateValueError::Malformed)?;
    let mut offset = header(bytes, family, maximum_wire_bytes, maximum_owned_bytes)?;
    let mut parts = [(0, 0); 7];
    let mut length = 0_usize;
    let (before, after) = match family {
        0 | 7 => (2, 0),
        1 | 10 => (5, 0),
        2 | 6 | 8 | 9 | 12 | 15 => (4, 0),
        13 => (7, 0),
        14 => (6, 0),
        16 | 17 => (3, 0),
        3 => (3, 4),
        4 => (6, 1),
        5 | 11 => (5, 1),
        _ => return Err(StateValueError::UnsupportedTag),
    };
    for part in &mut parts[..before] {
        let (found, n) = components::<1>(bytes, &mut offset)?;
        *part = found[0];
        length = length
            .checked_add(n)
            .ok_or(StateValueError::LengthOverflow)?;
    }
    let rank = if matches!(family, 3 | 4) {
        read_u8(bytes, &mut offset)?
    } else {
        0
    };
    let number = match family {
        3 | 5 | 11 | 14 | 15 | 17 => u128::from(read_u64(bytes, &mut offset)?),
        4 | 8 => read_u128(bytes, &mut offset)?,
        _ => 0,
    };
    for part in &mut parts[before..before + after] {
        let (found, n) = components::<1>(bytes, &mut offset)?;
        *part = found[0];
        length = length
            .checked_add(n)
            .ok_or(StateValueError::LengthOverflow)?;
    }
    finish(bytes, offset)?;
    if family == 7 {
        expected(bytes, &parts[..1], &[recovery.as_bytes()])?;
    } else {
        expected(bytes, &parts[..2], &[group.as_bytes(), recovery.as_bytes()])?;
    }
    if family == 3 && rank > 2 || family == 4 && rank > 1 {
        return Err(StateValueError::InvalidValue);
    }
    let logical = length
        .checked_add(256)
        .ok_or(StateValueError::LengthOverflow)?;
    if logical > maximum_owned_bytes {
        return Err(StateValueError::Limit {
            bytes: logical,
            maximum: maximum_owned_bytes,
        });
    }
    let mut ranges = [(0, 0); 7];
    let owned = copy_components::<7>(bytes, &parts[..before + after], length, &mut ranges)?;
    let decoded = DecodedStateKey {
        family,
        rank,
        number,
        bytes: owned,
        ranges,
    };
    if matches!(family, 12..=15) {
        super::credit::validate_decoded(&decoded)?;
    }
    if matches!(family, 16 | 17) {
        super::catalog::validate(&decoded)?;
    }
    if matches!(family, 5 | 11)
        && u128::from(canonical_history_position(decoded.part(5))?.0) != number
    {
        return Err(StateValueError::InvalidValue);
    }
    if matches!(family, 8 | 9) {
        super::workflow_values::validate_reference(decoded.part(3))?;
    }
    if matches!(family, 10 | 11)
        && [decoded.part(3), decoded.part(4)]
            .iter()
            .any(|part| part.is_empty() || part.len() > 65_535)
    {
        return Err(StateValueError::InvalidValue);
    }
    Ok(decoded)
}
