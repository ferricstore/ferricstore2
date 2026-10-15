//! Visible occurrence accounting and queried direct-map correspondence checks.

use super::value::{PolicyInputCursor as Cursor, PolicyInputKind as Kind, visit};
use super::{Error, PolicySchemaCharge, PolicySchemaError, charge_schema};

const QUERY_NAMES: [&[u8]; 46] = [
    b"mode",
    b"version",
    b"max_active_ms",
    b"retry",
    b"retention",
    b"indexed_attributes",
    b"indexed_state_meta",
    b"governance",
    b"states",
    b"max_attempts",
    b"max_retries",
    b"backoff",
    b"exhausted_to",
    b"kind",
    b"base_ms",
    b"max_ms",
    b"jitter_pct",
    b"ttl_ms",
    b"history_hot_max_events",
    b"history_max_events",
    b"effects",
    b"audit",
    b"limits",
    b"budgets",
    b"circuits",
    b"approvals",
    b"allowed",
    b"denied",
    b"require_idempotency_key",
    b"effect_events",
    b"denials",
    b"limit",
    b"enforcement",
    b"lease_size",
    b"window_ms",
    b"unit",
    b"failure_threshold",
    b"open_ms",
    b"min_calls",
    b"failure_rate_pct",
    b"latency_threshold_ms",
    b"error_classes",
    b"half_open_max_probes",
    b"half_open_success_threshold",
    b"assignees",
    b"timeout_ms",
];

pub(super) fn measure(
    workflow_type: Cursor<'_>,
    attrs: Cursor<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(usize, usize), Error> {
    let mut measured = (0_usize, 0_usize, 0_usize);
    node(workflow_type, 0, &mut measured, charge)?;
    node(attrs, 0, &mut measured, charge)?;
    Ok((measured.0, measured.2))
}

fn node(
    value: Cursor<'_>,
    depth: usize,
    measured: &mut (usize, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    visit(charge, 0)?;
    measured.0 = measured.0.checked_add(1).ok_or_else(overflow)?;
    if depth > 64 || measured.0 > 65_536 {
        return Err(Error::ItemLimit);
    }
    measured.2 = measured.2.max(depth);
    match value.kind() {
        Kind::Atom => text(
            value.atom_bytes().ok_or(Error::Malformed)?.len(),
            measured,
            charge,
        ),
        Kind::Binary => text(
            value.binary_bytes().ok_or(Error::Malformed)?.len(),
            measured,
            charge,
        ),
        Kind::Integer => match value.big_integer() {
            Some((_, bytes)) => text(bytes.len(), measured, charge),
            None => Ok(()),
        },
        Kind::Map => {
            let mut mask = 0_u128;
            for index in 0..value.len() {
                let (key, child) = value.row(index).ok_or(Error::Malformed)?;
                node(key, depth + 1, measured, charge)?;
                queried(key, &mut mask, charge)?;
                node(child, depth + 1, measured, charge)?;
            }
            Ok(())
        }
        Kind::Keyword => {
            for index in 0..value.len() {
                let (key, child) = value.row(index).ok_or(Error::Malformed)?;
                node(key, depth + 1, measured, charge)?;
                node(child, depth + 1, measured, charge)?;
            }
            Ok(())
        }
        Kind::List | Kind::Tuple => {
            for index in 0..value.len() {
                let child = value.child(index).ok_or(Error::Malformed)?;
                node(child, depth + 1, measured, charge)?;
            }
            Ok(())
        }
        Kind::ImproperList => {
            if value.len() == 0 {
                return Err(Error::Malformed);
            }
            for index in 0..value.len() {
                let child = value.child(index).ok_or(Error::Malformed)?;
                node(child, depth + 1, measured, charge)?;
            }
            node(
                value.tail().ok_or(Error::Malformed)?,
                depth + 1,
                measured,
                charge,
            )
        }
        _ => Ok(()),
    }
}

fn queried(key: Cursor<'_>, mask: &mut u128, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    let Some(bytes) = key.key_bytes() else {
        return Ok(());
    };
    for (index, name) in QUERY_NAMES.iter().enumerate() {
        visit(
            charge,
            bytes.len().checked_add(name.len()).ok_or_else(overflow)?,
        )?;
        if bytes == *name {
            visit(charge, 0)?;
            let bit = 1_u128 << (index + usize::from(!key.is_atom()) * 46);
            if *mask & bit != 0 {
                return Err(Error::Malformed);
            }
            *mask |= bit;
            break;
        }
    }
    Ok(())
}

fn text(
    length: usize,
    measured: &mut (usize, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    measured.1 = measured.1.checked_add(length).ok_or_else(overflow)?;
    if measured.1 > 64 * 1_024 * 1_024 {
        return Err(Error::ByteLimit);
    }
    charge_schema(charge, u64::try_from(length).map_err(|_| overflow())?, 0).map_err(Error::Schema)
}

fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
