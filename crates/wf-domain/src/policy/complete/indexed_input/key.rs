//! Borrowed source key conversion, fixed diagnostics and explicit Unicode trim.

use std::fmt;

use super::super::flow_input::value::PolicyInputCursor as Cursor;
use super::charge_schema;
use super::{Error, PolicyIndexedInputRejection as Reason, PolicySchemaCharge, PolicySchemaError};

impl Reason {
    /// Reads the exact fixed source response body without allocating.
    #[must_use]
    pub const fn message(self) -> &'static [u8] {
        self.text().as_bytes()
    }

    const fn text(self) -> &'static str {
        match self {
            Self::AttributesList => "ERR flow indexed_attributes must be a list",
            Self::AttributeString => "ERR flow attribute key must be a string",
            Self::AttributeUtf8 => "ERR flow attribute key must be valid UTF-8",
            Self::AttributeEmpty => "ERR flow attribute key must not be empty",
            Self::AttributeBytes => "ERR flow attribute key too large",
            Self::AttributeReserved => "ERR flow attribute key is reserved",
            Self::AttributesCount => "ERR flow indexed_attributes supports at most 3 keys",
            Self::StateMetaShape => "ERR flow indexed_state_meta must be a string or one-item list",
            Self::StateMetaUtf8 => "ERR flow state_meta key must be valid UTF-8",
            Self::StateMetaEmpty => "ERR flow state_meta key must not be empty",
            Self::StateMetaBytes => "ERR flow state_meta key too large",
            Self::StateMetaReserved => "ERR flow state_meta key is reserved",
            Self::StateMetaCount => "ERR flow indexed_state_meta supports at most 1 key",
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.text())
    }
}

impl std::error::Error for Reason {}

pub(super) fn normalize_key<'input>(
    value: Cursor<'input>,
    attributes: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<&'input [u8], Reason>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let Some(bytes) = value.key_bytes() else {
        return Ok(Err(if attributes {
            Reason::AttributeString
        } else {
            Reason::StateMetaShape
        }));
    };
    charge_visit(charge, bytes.len())?;
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Ok(Err(if attributes {
            Reason::AttributeUtf8
        } else {
            Reason::StateMetaUtf8
        }));
    };
    charge_visit(charge, bytes.len())?;
    let trimmed = text.trim_matches(source_white_space).as_bytes();
    charge_visit(charge, trimmed.len())?;
    let reason = if trimmed.is_empty() {
        Some(if attributes {
            Reason::AttributeEmpty
        } else {
            Reason::StateMetaEmpty
        })
    } else if trimmed.len() > 64 {
        Some(if attributes {
            Reason::AttributeBytes
        } else {
            Reason::StateMetaBytes
        })
    } else if trimmed.starts_with(b"__") {
        Some(if attributes {
            Reason::AttributeReserved
        } else {
            Reason::StateMetaReserved
        })
    } else {
        None
    };
    Ok(reason.map_or(Ok(trimmed), Err))
}

pub(super) fn equal_keys(
    left: &[u8],
    right: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    let length = left
        .len()
        .checked_add(right.len())
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_visit(charge, length)?;
    Ok(left == right)
}

pub(super) fn charge_visit(charge: &mut PolicySchemaCharge, length: usize) -> Result<(), Error> {
    let work = u64::try_from(length)
        .ok()
        .and_then(|n| n.checked_add(128))
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, work, 0).map_err(Error::Schema)
}

fn source_white_space(ch: char) -> bool {
    matches!(ch,
        '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{0085}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}
