//! Bounded raw indexed-policy classes, source-order checks and final owned fields.

use super::PolicySchemaLimits;
use super::admission::charge_schema;
use super::{CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError};

mod key;

use super::flow_input::value::{PolicyInputCursor as Cursor, PolicyInputKind as Kind};

pub(in crate::policy::complete) struct BorrowedPolicyIndexed<'input> {
    pub(in crate::policy::complete) count: Option<usize>,
    pub(in crate::policy::complete) names: [Option<&'input [u8]>; 3],
    pub(in crate::policy::complete) state_meta: Option<&'input [u8]>,
}

type IndexedOwners = (Option<Vec<Vec<u8>>>, Option<Vec<u8>>);

/// Borrowed input classes for the two original indexed-policy fields.
///
/// This view supplies no raw parser, ETF, current-policy or payment authority.
/// Atom `"nil"` has the same contextual meaning as [`Self::Nil`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawPolicyIndexedValue<'input> {
    /// The original nil atom.
    Nil,
    /// An original binary, whose UTF-8 is checked when reached as a key.
    Binary(&'input [u8]),
    /// An atom's borrowed UTF-8 spelling, without atom allocation or interning.
    Atom(&'input str),
    /// An original list; shared descendants count as separate occurrences.
    List(&'input [Self]),
    /// A reached unsupported source input class.
    Other,
}

/// Exact reached source diagnostics, separate from operational refusals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyIndexedInputRejection {
    /// The attribute field is not a list or nil.
    AttributesList,
    /// An attribute list member is not a binary or atom.
    AttributeString,
    /// An attribute key is not valid UTF-8.
    AttributeUtf8,
    /// An attribute key is empty after source trimming.
    AttributeEmpty,
    /// An attribute key exceeds 64 trimmed bytes.
    AttributeBytes,
    /// An attribute key begins with the reserved prefix.
    AttributeReserved,
    /// More than three unique normalized attribute keys remain.
    AttributesCount,
    /// Metadata has an unsupported input class.
    StateMetaShape,
    /// A metadata key is not valid UTF-8.
    StateMetaUtf8,
    /// A nonempty metadata input trims to an empty key.
    StateMetaEmpty,
    /// A metadata key exceeds 64 trimmed bytes.
    StateMetaBytes,
    /// A metadata key begins with the reserved prefix.
    StateMetaReserved,
    /// A reached metadata list has more than one unique normalized key.
    StateMetaCount,
}

/// Normalizes both fields in source order, then copies only final unique leaves.
///
/// The operational preflight bounds both borrowed trees to depth 64, 4096 value
/// occurrences and 262144 raw bytes before semantic checking. Absent attributes
/// remain absent; present nil/empty lists become a present empty vector. Metadata
/// nil/empty values disappear. Input owners remain unchanged and live while final
/// owners are built. Logical scratch does not measure native allocation or stack.
///
/// # Errors
/// Outer errors preserve profile, checked length, work, raw-cap and allocation
/// refusals. An inner error is only the reached original normalization diagnosis.
pub fn normalize_complete_policy_indexed_input(
    attributes: Option<RawPolicyIndexedValue<'_>>,
    state_meta: Option<RawPolicyIndexedValue<'_>>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<IndexedOwners, PolicyIndexedInputRejection>, Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let mut measured = (0_u64, 0_u64, 0_u64);
    for (value, metadata_depth) in [(attributes, None), (state_meta, Some(0))] {
        if let Some(value) = value
            && let Some(list) = preflight_node(value, 0, &mut measured, charge)?
        {
            preflight_list(list, 0, metadata_depth, &mut measured, charge)?;
        }
    }
    let attribute_scratch = u64::from(
        matches!(attributes, Some(RawPolicyIndexedValue::List(list)) if !list.is_empty()),
    ) * 192;
    let scratch = measured
        .2
        .checked_mul(64)
        .and_then(|n| n.checked_add(attribute_scratch))
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, 0, scratch).map_err(Error::Schema)?;
    let fields = match borrow_indexed(
        attributes.map(Cursor::Indexed),
        state_meta.map(Cursor::Indexed),
        charge,
    )? {
        Ok(fields) => fields,
        Err(reason) => return Ok(Err(reason)),
    };
    let attributes = own_attributes(fields.count, &fields.names, charge)?;
    let metadata = fields
        .state_meta
        .map(|name| copy_key(name, charge))
        .transpose()?;
    Ok(Ok((attributes, metadata)))
}

pub(in crate::policy::complete) fn borrow_indexed<'input>(
    attributes: Option<Cursor<'input>>,
    state_meta: Option<Cursor<'input>>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<BorrowedPolicyIndexed<'input>, PolicyIndexedInputRejection>, Error> {
    let mut names = [None; 3];
    let count = match borrow_attributes(attributes, &mut names, charge)? {
        Ok(count) => count,
        Err(reason) => return Ok(Err(reason)),
    };
    let state_meta = if let Some(value) = state_meta {
        match borrow_metadata(value, 0, charge)? {
            Ok(name) => name,
            Err(reason) => return Ok(Err(reason)),
        }
    } else {
        charge_schema(charge, 128, 0).map_err(Error::Schema)?;
        None
    };
    Ok(Ok(BorrowedPolicyIndexed {
        count,
        names,
        state_meta,
    }))
}

fn preflight_node<'input>(
    value: RawPolicyIndexedValue<'input>,
    depth: usize,
    measured: &mut (u64, u64, u64),
    charge: &mut PolicySchemaCharge,
) -> Result<Option<&'input [RawPolicyIndexedValue<'input>]>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    measured.0 = measured
        .0
        .checked_add(1)
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    if depth > 64 || measured.0 > 4_096 {
        return Err(Error::ItemLimit);
    }
    let length = match value {
        RawPolicyIndexedValue::Binary(bytes) => bytes.len(),
        RawPolicyIndexedValue::Atom(text) if text != "nil" => text.len(),
        RawPolicyIndexedValue::List(list) => return Ok(Some(list)),
        _ => return Ok(None),
    };
    let length =
        u64::try_from(length).map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?;
    measured.1 = measured
        .1
        .checked_add(length)
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    if measured.1 > 262_144 {
        return Err(Error::ByteLimit);
    }
    charge_schema(charge, length, 0).map_err(Error::Schema)?;
    Ok(None)
}

fn preflight_list(
    list: &[RawPolicyIndexedValue<'_>],
    depth: usize,
    metadata_depth: Option<u64>,
    measured: &mut (u64, u64, u64),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    if list.is_empty() {
        return Ok(());
    }
    let metadata_depth = metadata_depth.map(|n| n + 1);
    measured.2 = measured.2.max(metadata_depth.unwrap_or(0));
    let child_depth = depth
        .checked_add(1)
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    for &value in list {
        // The prospective candidate node and its depth/count caps precede descent.
        if let Some(child) = preflight_node(value, child_depth, measured, charge)? {
            preflight_list(child, child_depth, metadata_depth, measured, charge)?;
        }
    }
    Ok(())
}

pub(in crate::policy::complete) fn borrow_attributes<'input>(
    value: Option<Cursor<'input>>,
    names: &mut [Option<&'input [u8]>; 3],
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Option<usize>, PolicyIndexedInputRejection>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let list = match value {
        None => return Ok(Ok(None)),
        Some(value) if value.is_nil() => return Ok(Ok(Some(0))),
        Some(value) if matches!(value.kind(), Kind::List | Kind::ImproperList) => value,
        _ => return Ok(Err(PolicyIndexedInputRejection::AttributesList)),
    };
    let mut count = 0;
    let mut overflow = false;
    let mut index = 0;
    while let Some(value) = list.list_item(index, charge)? {
        index += 1;
        let name = match key::normalize_key(value, true, charge)? {
            Ok(name) => name,
            Err(reason) => return Ok(Err(reason)),
        };
        if overflow {
            continue;
        }
        let mut duplicate = false;
        for existing in names.iter().take(count).flatten() {
            if key::equal_keys(existing, name, charge)? {
                duplicate = true;
                break;
            }
        }
        if !duplicate {
            if count == names.len() {
                overflow = true;
            } else {
                names[count] = Some(name);
                count += 1;
            }
        }
    }
    Ok(if overflow {
        Err(PolicyIndexedInputRejection::AttributesCount)
    } else {
        Ok(Some(count))
    })
}

pub(in crate::policy::complete) fn borrow_metadata<'input>(
    value: Cursor<'input>,
    depth: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Option<&'input [u8]>, PolicyIndexedInputRejection>, Error> {
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    if value.is_nil() || value.atom_bytes() == Some(b"") || value.binary_bytes() == Some(b"") {
        return Ok(Ok(None));
    }
    match value.kind() {
        Kind::List | Kind::ImproperList => {
            let mut first = None;
            let mut overflow = false;
            let mut index = 0;
            while let Some(child) = value.list_item(index, charge)? {
                index += 1;
                if depth >= 64 {
                    return Err(Error::ItemLimit);
                }
                let name = match borrow_metadata(child, depth + 1, charge)? {
                    Ok(name) => name,
                    Err(reason) => return Ok(Err(reason)),
                };
                if let Some(name) = name {
                    if overflow {
                        continue;
                    }
                    if let Some(existing) = first {
                        if !key::equal_keys(existing, name, charge)? {
                            overflow = true;
                        }
                    } else {
                        first = Some(name);
                    }
                }
            }
            Ok(if overflow {
                Err(PolicyIndexedInputRejection::StateMetaCount)
            } else {
                Ok(first)
            })
        }
        Kind::Atom | Kind::Binary => Ok(key::normalize_key(value, false, charge)?.map(Some)),
        _ => Ok(Err(PolicyIndexedInputRejection::StateMetaShape)),
    }
}

fn own_attributes(
    count: Option<usize>,
    names: &[Option<&[u8]>; 3],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Vec<Vec<u8>>>, Error> {
    let Some(count) = count else {
        return Ok(None);
    };
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let reserve = u64::try_from(count)
        .ok()
        .and_then(|n| n.checked_mul(128))
        .and_then(|n| n.checked_add(128))
        .ok_or(Error::Schema(PolicySchemaError::LengthOverflow))?;
    charge_schema(charge, reserve, 0).map_err(Error::Schema)?;
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(count)
        .map_err(|_| Error::AllocationFailure)?;
    for name in names.iter().take(count).flatten() {
        owned.push(copy_key(name, charge)?);
    }
    Ok(Some(owned))
}

fn copy_key(name: &[u8], charge: &mut PolicySchemaCharge) -> Result<Vec<u8>, Error> {
    key::charge_visit(charge, name.len())?;
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(name.len())
        .map_err(|_| Error::AllocationFailure)?;
    owned.extend_from_slice(name);
    Ok(owned)
}
