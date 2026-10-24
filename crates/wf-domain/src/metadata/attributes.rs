//! Original attribute predicates and normalization at the captured boundary.

use super::{
    MAX_ATTRIBUTE_BYTES, MAX_ATTRIBUTE_LIST_VALUES, MAX_ATTRIBUTES, MAX_VALUE_BYTES, NameKind,
    OriginalAttributeValue, OriginalAttributes, OriginalMetadataError, OriginalMetadataInput,
    atom_spelling, canonical_name, canonical_scalar, is_last_name, normalize_name,
    normalize_scalar, scalar_size,
};

/// Checks canonical original attributes without rebuilding their map.
#[must_use]
pub fn valid_original_attributes(value: OriginalMetadataInput<'_, '_>) -> bool {
    let OriginalMetadataInput::Map(rows) = value else {
        return false;
    };
    if rows.len() > MAX_ATTRIBUTES {
        return false;
    }
    let mut total = 0;
    for &(key, value) in rows {
        if !canonical_name(key, NameKind::Attribute)
            || !(valid_original_attribute_scalar(value) || valid_original_attribute_list(value))
        {
            return false;
        }
        let OriginalMetadataInput::Binary(name) = key else {
            return false;
        };
        let Ok(size) = attribute_value_size(value) else {
            return false;
        };
        total += name.len() + size;
    }
    total <= MAX_ATTRIBUTE_BYTES
}

/// Checks the original bounded canonical scalar classes.
#[must_use]
pub fn valid_original_attribute_scalar(value: OriginalMetadataInput<'_, '_>) -> bool {
    canonical_scalar(value)
}

/// Checks a nonempty bounded proper list of unique opaque binaries.
#[must_use]
pub fn valid_original_attribute_list(value: OriginalMetadataInput<'_, '_>) -> bool {
    let OriginalMetadataInput::List(values) = value else {
        return false;
    };
    if values.is_empty() || values.len() > MAX_ATTRIBUTE_LIST_VALUES {
        return false;
    }
    for (index, &value) in values.iter().enumerate() {
        let OriginalMetadataInput::Binary(bytes) = value else {
            return false;
        };
        if bytes.len() > MAX_VALUE_BYTES
            || values[index + 1..]
                .iter()
                .any(|&other| matches!(other, OriginalMetadataInput::Binary(b) if b == bytes))
        {
            return false;
        }
    }
    true
}

/// Normalizes every source row before allocating the surviving output spines.
///
/// # Errors
/// Preserves source diagnoses, improper-list argument errors and reserve failures.
pub fn normalize_original_attributes<'leaf>(
    value: OriginalMetadataInput<'_, 'leaf>,
) -> Result<OriginalAttributes<'leaf>, OriginalMetadataError> {
    let rows = match value {
        OriginalMetadataInput::Nil | OriginalMetadataInput::Atom("nil") => &[][..],
        OriginalMetadataInput::Map(rows) => rows,
        _ => {
            return Err(OriginalMetadataError::SourceReturned(
                "ERR flow attributes must be a map",
            ));
        }
    };
    validate_entries(rows)?;
    build_attributes(rows)
}

fn validate_entries(
    rows: &[(OriginalMetadataInput<'_, '_>, OriginalMetadataInput<'_, '_>)],
) -> Result<(), OriginalMetadataError> {
    if rows.len() > MAX_ATTRIBUTES {
        return Err(OriginalMetadataError::SourceReturned(
            "ERR too many flow attributes",
        ));
    }
    for &(name, value) in rows {
        normalize_name(name, NameKind::Attribute)?;
        match value {
            OriginalMetadataInput::List(values)
                if !values.is_empty() && values.len() <= MAX_ATTRIBUTE_LIST_VALUES =>
            {
                for &member in values {
                    list_binary(member)?;
                }
            }
            OriginalMetadataInput::List(_) | OriginalMetadataInput::ImproperList { .. } => {
                attribute_value_size(value)?;
            }
            _ => {
                normalize_scalar(value, true)?;
            }
        }
    }
    let mut total = 0;
    for (index, &(name, value)) in rows.iter().enumerate() {
        if is_last_name(rows, index, NameKind::Attribute)? {
            total +=
                normalize_name(name, NameKind::Attribute)?.len() + attribute_value_size(value)?;
        }
    }
    if total > MAX_ATTRIBUTE_BYTES {
        return Err(OriginalMetadataError::SourceReturned(
            "ERR flow attributes too large",
        ));
    }
    Ok(())
}

fn attribute_value_size(
    value: OriginalMetadataInput<'_, '_>,
) -> Result<usize, OriginalMetadataError> {
    match value {
        OriginalMetadataInput::ImproperList { .. } => {
            Err(OriginalMetadataError::RaisedArgumentError)
        }
        OriginalMetadataInput::List(values) => {
            if values.is_empty() {
                return Err(OriginalMetadataError::SourceReturned(
                    "ERR flow attribute list must not be empty",
                ));
            }
            if values.len() > MAX_ATTRIBUTE_LIST_VALUES {
                return Err(OriginalMetadataError::SourceReturned(
                    "ERR flow attribute list too large",
                ));
            }
            for &value in values {
                list_binary(value)?;
            }
            let mut total = 0;
            for (index, &value) in values.iter().enumerate() {
                if is_last_list_value(values, index)? {
                    total += list_binary(value)?.len();
                }
            }
            Ok(total)
        }
        _ => normalize_scalar(value, true).map(scalar_size),
    }
}

fn list_binary<'leaf>(
    value: OriginalMetadataInput<'_, 'leaf>,
) -> Result<&'leaf [u8], OriginalMetadataError> {
    let bytes =
        match value {
            OriginalMetadataInput::Binary(bytes) => bytes,
            _ => atom_spelling(value).map(str::as_bytes).ok_or(
                OriginalMetadataError::SourceReturned(
                    "ERR flow attribute list values must be strings",
                ),
            )?,
        };
    if bytes.len() > MAX_VALUE_BYTES {
        return Err(OriginalMetadataError::SourceReturned(
            "ERR flow attribute value too large",
        ));
    }
    Ok(bytes)
}

fn is_last_list_value(
    values: &[OriginalMetadataInput<'_, '_>],
    index: usize,
) -> Result<bool, OriginalMetadataError> {
    let value = list_binary(values[index])?;
    for &other in &values[index + 1..] {
        if list_binary(other)? == value {
            return Ok(false);
        }
    }
    Ok(true)
}

fn build_attribute_value<'leaf>(
    value: OriginalMetadataInput<'_, 'leaf>,
) -> Result<OriginalAttributeValue<'leaf>, OriginalMetadataError> {
    attribute_value_size(value)?;
    if let OriginalMetadataInput::List(values) = value {
        let mut count = 0;
        for index in 0..values.len() {
            count += usize::from(is_last_list_value(values, index)?);
        }
        let mut output = Vec::new();
        output
            .try_reserve_exact(count)
            .map_err(OriginalMetadataError::ReserveFailed)?;
        for (index, &value) in values.iter().enumerate() {
            if is_last_list_value(values, index)? {
                output.push(list_binary(value)?);
            }
        }
        Ok(OriginalAttributeValue::List(output))
    } else {
        normalize_scalar(value, true).map(OriginalAttributeValue::Scalar)
    }
}

fn build_attributes<'leaf>(
    rows: &[(
        OriginalMetadataInput<'_, 'leaf>,
        OriginalMetadataInput<'_, 'leaf>,
    )],
) -> Result<OriginalAttributes<'leaf>, OriginalMetadataError> {
    let mut count = 0;
    for index in 0..rows.len() {
        count += usize::from(is_last_name(rows, index, NameKind::Attribute)?);
    }
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(count)
        .map_err(OriginalMetadataError::ReserveFailed)?;
    for (index, &(name, value)) in rows.iter().enumerate() {
        if is_last_name(rows, index, NameKind::Attribute)? {
            entries.push((
                normalize_name(name, NameKind::Attribute)?,
                build_attribute_value(value)?,
            ));
        }
    }
    Ok(OriginalAttributes { entries })
}
