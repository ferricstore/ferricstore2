//! Original state-metadata validation and normalization.

use super::{
    MAX_STATE_ENTRIES, MAX_STATE_META_BYTES, MAX_STATES, NameKind, OriginalMetadataError,
    OriginalMetadataInput, OriginalMetadataScalar, OriginalStateMeta, canonical_name,
    canonical_scalar, is_last_name, normalize_name, normalize_scalar, scalar_size,
};

/// Normalizes one original state-metadata key, borrowing its UTF8 slice.
///
/// # Errors
/// This separate name entry preserves original state-key normalization errors.
pub fn normalize_original_state_metadata_name<'a>(
    value: OriginalMetadataInput<'_, 'a>,
) -> Result<&'a str, OriginalMetadataError> {
    normalize_name(value, NameKind::StateKey)
}

/// Checks canonical scalar-only state metadata without rebuilding maps.
#[must_use]
pub fn valid_original_state_meta(value: OriginalMetadataInput<'_, '_>) -> bool {
    let OriginalMetadataInput::Map(rows) = value else {
        return false;
    };
    if rows.len() > MAX_STATES {
        return false;
    }
    let mut total = 0;
    for &(state, value) in rows {
        if !canonical_name(state, NameKind::State) {
            return false;
        }
        let OriginalMetadataInput::Binary(state) = state else {
            return false;
        };
        let Some(size) = canonical_entry_map_size(value) else {
            return false;
        };
        total += state.len() + size;
    }
    total <= MAX_STATE_META_BYTES
}

/// Normalizes all source states and entries before allocating surviving rows.
///
/// # Errors
/// Preserves original name, class and logical-limit diagnoses and reserve failures.
pub fn normalize_original_state_meta<'leaf>(
    value: OriginalMetadataInput<'_, 'leaf>,
) -> Result<OriginalStateMeta<'leaf>, OriginalMetadataError> {
    let rows = match value {
        OriginalMetadataInput::Nil | OriginalMetadataInput::Atom("nil") => &[][..],
        OriginalMetadataInput::Map(rows) => rows,
        _ => {
            return Err(OriginalMetadataError::SourceReturned(
                "ERR flow state_meta must be a map",
            ));
        }
    };
    validate_states(rows)?;
    build_state_meta(rows)
}

fn validate_states(
    rows: &[(OriginalMetadataInput<'_, '_>, OriginalMetadataInput<'_, '_>)],
) -> Result<(), OriginalMetadataError> {
    if rows.len() > MAX_STATES {
        return Err(OriginalMetadataError::SourceReturned(
            "ERR too many flow state_meta states",
        ));
    }
    for &(state, value) in rows {
        normalize_name(state, NameKind::State)?;
        let entries = match value {
            OriginalMetadataInput::Nil | OriginalMetadataInput::Atom("nil") => &[][..],
            OriginalMetadataInput::Map(entries) => entries,
            _ => {
                return Err(OriginalMetadataError::SourceReturned(
                    "ERR flow state_meta must be a map",
                ));
            }
        };
        if entries.len() > MAX_STATE_ENTRIES {
            return Err(OriginalMetadataError::SourceReturned(
                "ERR too many flow state_meta entries",
            ));
        }
        for &(name, value) in entries {
            normalize_name(name, NameKind::StateKey)?;
            normalize_scalar(value, false)?;
        }
    }
    let mut total = 0;
    for (index, &(state, value)) in rows.iter().enumerate() {
        if is_last_name(rows, index, NameKind::State)? {
            total += normalize_name(state, NameKind::State)?.len() + entry_map_size(value)?;
        }
    }
    if total > MAX_STATE_META_BYTES {
        return Err(OriginalMetadataError::SourceReturned(
            "ERR flow state_meta too large",
        ));
    }
    Ok(())
}

fn entry_map_size(value: OriginalMetadataInput<'_, '_>) -> Result<usize, OriginalMetadataError> {
    let rows = match value {
        OriginalMetadataInput::Nil | OriginalMetadataInput::Atom("nil") => &[][..],
        OriginalMetadataInput::Map(rows) => rows,
        _ => {
            return Err(OriginalMetadataError::SourceReturned(
                "ERR flow state_meta must be a map",
            ));
        }
    };
    if rows.len() > MAX_STATE_ENTRIES {
        return Err(OriginalMetadataError::SourceReturned(
            "ERR too many flow state_meta entries",
        ));
    }
    for &(name, value) in rows {
        normalize_name(name, NameKind::StateKey)?;
        normalize_scalar(value, false)?;
    }
    let mut total = 0;
    for (index, &(name, value)) in rows.iter().enumerate() {
        if is_last_name(rows, index, NameKind::StateKey)? {
            total += normalize_name(name, NameKind::StateKey)?.len()
                + scalar_size(normalize_scalar(value, false)?);
        }
    }
    Ok(total)
}

fn canonical_entry_map_size(value: OriginalMetadataInput<'_, '_>) -> Option<usize> {
    let OriginalMetadataInput::Map(rows) = value else {
        return None;
    };
    if rows.len() > MAX_STATE_ENTRIES {
        return None;
    }
    let mut total = 0;
    for &(name, value) in rows {
        if !canonical_name(name, NameKind::StateKey) || !canonical_scalar(value) {
            return None;
        }
        let OriginalMetadataInput::Binary(name) = name else {
            return None;
        };
        total += name.len() + scalar_size(normalize_scalar(value, false).ok()?);
    }
    Some(total)
}

fn build_entry_map<'leaf>(
    value: OriginalMetadataInput<'_, 'leaf>,
) -> Result<Vec<(&'leaf str, OriginalMetadataScalar<'leaf>)>, OriginalMetadataError> {
    entry_map_size(value)?;
    let rows = match value {
        OriginalMetadataInput::Nil | OriginalMetadataInput::Atom("nil") => &[][..],
        OriginalMetadataInput::Map(rows) => rows,
        _ => {
            return Err(OriginalMetadataError::SourceReturned(
                "ERR flow state_meta must be a map",
            ));
        }
    };
    let mut count = 0;
    for index in 0..rows.len() {
        count += usize::from(is_last_name(rows, index, NameKind::StateKey)?);
    }
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(count)
        .map_err(OriginalMetadataError::ReserveFailed)?;
    for (index, &(name, value)) in rows.iter().enumerate() {
        if is_last_name(rows, index, NameKind::StateKey)? {
            entries.push((
                normalize_name(name, NameKind::StateKey)?,
                normalize_scalar(value, false)?,
            ));
        }
    }
    Ok(entries)
}

fn build_state_meta<'leaf>(
    rows: &[(
        OriginalMetadataInput<'_, 'leaf>,
        OriginalMetadataInput<'_, 'leaf>,
    )],
) -> Result<OriginalStateMeta<'leaf>, OriginalMetadataError> {
    let mut count = 0;
    for index in 0..rows.len() {
        count += usize::from(is_last_name(rows, index, NameKind::State)?);
    }
    let mut states = Vec::new();
    states
        .try_reserve_exact(count)
        .map_err(OriginalMetadataError::ReserveFailed)?;
    for (index, &(state, value)) in rows.iter().enumerate() {
        if is_last_name(rows, index, NameKind::State)? {
            states.push((
                normalize_name(state, NameKind::State)?,
                build_entry_map(value)?,
            ));
        }
    }
    Ok(OriginalStateMeta { states })
}
