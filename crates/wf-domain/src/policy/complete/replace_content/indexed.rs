//! Borrowed UTF-8 diagnostics with separate normalized-owner correspondence.

use std::collections::BTreeSet;

use super::super::{CompletePolicyData, PolicySchemaCharge, PolicySchemaError};
use super::{CompletePolicyReplaceRejectionKind as Kind, charge_schema, charge_visit};

pub(super) fn check_replace_indexed(
    data: &CompletePolicyData,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Kind>, PolicySchemaError> {
    if let Some(names) = &data.indexed_attributes {
        charge_schema(charge, 128, 0)?;
        // The entry admitted the logical scratch measured by complete metrics.
        let mut seen = BTreeSet::new();
        for name in names {
            if let Some(kind) = check_replace_indexed_key(name, true, charge)? {
                return Ok(Some(kind));
            }
            charge_visit(charge, name.len())?;
            if !seen.insert(name.as_slice()) {
                return Err(PolicySchemaError::InvalidIndexedAttributes);
            }
        }
    }
    if let Some(name) = &data.indexed_state_meta {
        return check_replace_indexed_key(name, false, charge);
    }
    Ok(None)
}

fn check_replace_indexed_key(
    name: &[u8],
    attributes: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Kind>, PolicySchemaError> {
    let operational = if attributes {
        PolicySchemaError::InvalidIndexedAttributes
    } else {
        PolicySchemaError::InvalidIndexedStateMeta
    };
    charge_schema(charge, 128, 0)?;
    charge_visit(charge, name.len())?;
    let Ok(text) = std::str::from_utf8(name) else {
        return Ok(Some(if attributes {
            Kind::IndexedAttributeUtf8
        } else {
            Kind::IndexedStateMetaUtf8
        }));
    };
    charge_visit(charge, name.len())?;
    let trimmed = text.trim_matches(source_white_space);
    if trimmed.is_empty() {
        if !attributes && name.is_empty() {
            // Source direct empty state_meta normalizes to None, not rejection.
            return Err(operational);
        }
        return Ok(Some(if attributes {
            Kind::IndexedAttributeEmpty
        } else {
            Kind::IndexedStateMetaEmpty
        }));
    }
    // The operational preflight already bounded the original owner to 64 bytes.
    charge_visit(charge, name.len())?;
    if trimmed.starts_with("__") {
        return Ok(Some(if attributes {
            Kind::IndexedAttributeReserved
        } else {
            Kind::IndexedStateMetaReserved
        }));
    }
    charge_visit(charge, name.len())?;
    if trimmed.as_bytes() != name {
        return Err(operational);
    }
    Ok(None)
}

fn source_white_space(ch: char) -> bool {
    matches!(ch,
        '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{0085}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}
