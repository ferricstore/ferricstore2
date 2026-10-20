//! Fresh captured-table admission and branch-specific safe lookups.

use super::{Error, OriginalEtfSafety, PolicySchemaCharge, overflow, pay};

pub(super) fn admit(
    context: &OriginalEtfSafety<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    pay(charge, 128)?;
    let rows = context
        .atoms
        .len()
        .checked_add(context.active_exports.len())
        .and_then(|n| n.checked_add(context.local_funs.len()))
        .and_then(|n| n.checked_add(usize::from(context.local_node.is_some())))
        .ok_or_else(overflow)?;
    if rows > 4_096 {
        return Err(Error::ItemLimit);
    }
    let mut bytes = 0_usize;
    admit_atoms(context, &mut bytes, charge)?;
    admit_exports(context, &mut bytes, charge)?;
    admit_funs(context, &mut bytes, charge)?;
    if let Some((node, _)) = context.local_node {
        row(node.len(), 4, charge)?;
        text(node, &mut bytes)?;
    }
    Ok(())
}

fn admit_atoms(
    context: &OriginalEtfSafety<'_>,
    bytes: &mut usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let mut fingerprints = [0_u32; 4_096];
    for (index, &name) in context.atoms.iter().enumerate() {
        row(name.len(), 0, charge)?;
        text(name, bytes)?;
        let hash = fingerprint(name.as_bytes());
        for (prior, &old_hash) in fingerprints[..index].iter().enumerate() {
            if old_hash == hash {
                comparison(name.len(), context.atoms[prior].len(), charge)?;
                if name == context.atoms[prior] {
                    return Err(Error::Malformed);
                }
            }
        }
        fingerprints[index] = hash;
    }
    Ok(())
}

fn row(text: usize, fixed: u64, charge: &mut PolicySchemaCharge) -> Result<(), Error> {
    pay(
        charge,
        u64::try_from(text)
            .map_err(|_| overflow())?
            .checked_add(fixed)
            .and_then(|n| n.checked_add(128))
            .ok_or_else(overflow)?,
    )
}

fn text(value: &str, total: &mut usize) -> Result<(), Error> {
    *total = total.checked_add(value.len()).ok_or_else(overflow)?;
    if *total > 262_144 || value.chars().count() > 255 {
        return Err(Error::ByteLimit);
    }
    Ok(())
}

fn admit_exports(
    context: &OriginalEtfSafety<'_>,
    bytes: &mut usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let mut fingerprints = [0_u32; 4_096];
    for (index, &(module, function, arity)) in context.active_exports.iter().enumerate() {
        row(
            module
                .len()
                .checked_add(function.len())
                .ok_or_else(overflow)?,
            4,
            charge,
        )?;
        text(module, bytes)?;
        text(function, bytes)?;
        let hash = fingerprint(module.as_bytes()).rotate_left(5)
            ^ fingerprint(function.as_bytes())
            ^ arity;
        for (prior, &old_hash) in fingerprints[..index].iter().enumerate() {
            if hash == old_hash {
                let (old_module, old_function, old_arity) = context.active_exports[prior];
                comparison(
                    module.len() + function.len() + 4,
                    old_module.len() + old_function.len() + 4,
                    charge,
                )?;
                if old_module == module && old_function == function && old_arity == arity {
                    return Err(Error::Malformed);
                }
            }
        }
        fingerprints[index] = hash;
    }
    Ok(())
}

fn admit_funs(
    context: &OriginalEtfSafety<'_>,
    bytes: &mut usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let mut fingerprints = [0_u32; 4_096];
    for (index, value) in context.local_funs.iter().enumerate() {
        row(value.module.len(), 25, charge)?;
        text(value.module, bytes)?;
        let hash = fingerprint(value.module.as_bytes())
            ^ fingerprint(&value.index.to_le_bytes()).rotate_left(3)
            ^ u32::from(value.arity)
            ^ fingerprint(&value.uniq).rotate_left(9);
        for (prior, &old_hash) in fingerprints[..index].iter().enumerate() {
            if old_hash == hash {
                let old = context.local_funs[prior];
                comparison(old.module.len() + 21, value.module.len() + 21, charge)?;
                if old.module == value.module
                    && old.index == value.index
                    && old.arity == value.arity
                    && old.uniq == value.uniq
                {
                    return Err(Error::Malformed);
                }
            }
        }
        fingerprints[index] = hash;
    }
    Ok(())
}

fn fingerprint(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5_u32, |value, &byte| {
        (value ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

pub(super) fn comparison(
    left: usize,
    right: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let length = left.checked_add(right).ok_or_else(overflow)?;
    row(length, 0, charge)
}

pub(super) fn atom(
    context: &OriginalEtfSafety<'_>,
    raw: &[u8],
    latin: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<u32, Error> {
    atom_profiled(context, raw, latin, false, charge)
}

pub(super) fn atom_profiled(
    context: &OriginalEtfSafety<'_>,
    raw: &[u8],
    latin: bool,
    portable: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<u32, Error> {
    let utf8 = if latin {
        None
    } else {
        Some(std::str::from_utf8(raw).map_err(|_| Error::Malformed)?)
    };
    let count = utf8.map_or(raw.len(), |text| text.chars().count());
    if count > 255 {
        return Err(Error::Malformed);
    }
    let width = utf8.map_or_else(
        || raw.iter().map(|&b| if b < 128 { 1 } else { 2 }).sum(),
        str::len,
    );
    for (index, &name) in context.atoms.iter().enumerate() {
        comparison(width, name.len(), charge)?;
        let equal = utf8.map_or_else(
            || name.chars().eq(raw.iter().map(|&byte| char::from(byte))),
            |text| text == name,
        );
        if equal {
            return u32::try_from(index).map_err(|_| overflow());
        }
    }
    Err(if portable {
        Error::UnsupportedPolicyProfile
    } else {
        Error::Malformed
    })
}

pub(super) fn node(
    context: &OriginalEtfSafety<'_>,
    name: &str,
    creation: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    let (local, local_creation) = context.local_node.ok_or(Error::UnsupportedTag)?;
    comparison(name.len() + 4, local.len() + 4, charge)?;
    Ok(name == local && creation == local_creation)
}

pub(super) fn exported(
    context: &OriginalEtfSafety<'_>,
    module: &str,
    function: &str,
    arity: u32,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    for &(old_module, old_function, old_arity) in context.active_exports {
        comparison(
            module.len() + function.len() + 4,
            old_module.len() + old_function.len() + 4,
            charge,
        )?;
        if module == old_module && function == old_function && arity == old_arity {
            return Ok(());
        }
    }
    Err(Error::Malformed)
}

/// Returns the immutable emitter profile, including the explicit nil compatibility atom.
#[must_use]
pub const fn canonical_original_etf_safety() -> OriginalEtfSafety<'static> {
    OriginalEtfSafety::new(CANONICAL_ATOMS, &[], None, &[])
}

const CANONICAL_ATOMS: &[&str] = &[
    "all",
    "allowed",
    "approvals",
    "approximate_global",
    "assignees",
    "audit",
    "backoff",
    "base_ms",
    "budgets",
    "circuits",
    "denials",
    "denied",
    "effect_events",
    "effects",
    "enforcement",
    "error_classes",
    "exhausted_to",
    "exponential",
    "failure_rate_pct",
    "failure_threshold",
    "false",
    "fifo",
    "fixed",
    "flow_policy_v1",
    "full",
    "governance",
    "half_open_max_probes",
    "half_open_success_threshold",
    "history_max_events",
    "indexed_attributes",
    "indexed_state_meta",
    "jitter_pct",
    "kind",
    "latency_threshold_ms",
    "lease_size",
    "ledger",
    "limit",
    "limits",
    "linear",
    "max_active_ms",
    "max_ms",
    "max_retries",
    "min_calls",
    "minimal",
    "mode",
    "nil",
    "none",
    "off",
    "open_ms",
    "parallel",
    "require_idempotency_key",
    "retention",
    "retry",
    "sampled",
    "states",
    "strict_global",
    "timeout_ms",
    "true",
    "ttl_ms",
    "type",
    "unit",
    "version",
    "window_ms",
];
