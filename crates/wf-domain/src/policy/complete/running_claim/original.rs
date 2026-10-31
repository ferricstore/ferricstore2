//! Resolve source deep-merge presence directly over one complete original index.

use super::super::{
    LimitData, PolicySchemaCharge, PolicySchemaLimits,
    flow_input::value::PolicyInputKind as Kind,
    original_entry::{TermPosition as Position, indexed::OriginalPolicyCurrentIndex as Index},
    record_projection::original::{field, read},
};
use super::{
    Error, ResolvedRunningClaimPolicy as Resolved, RunningClaimPolicyVersion as Version, aggregate,
    pay,
};
use crate::workflow::{MAX_EXACT_MILLIS, active::LimitEnforcement};

pub(super) fn resolve<'raw>(
    raw: &'raw [u8],
    kind: &[u8],
    states: Option<&[&[u8]]>,
    amount: u64,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Resolved<'raw>>, Error> {
    let index = read(raw, kind, schema, charge)?;
    let root = index.cursor().position();
    let base = field(&index, root, b"governance", charge)?
        .filter(|&p| index.at(p).kind() == Kind::Map)
        .unwrap_or(root);
    let catalog =
        field(&index, root, b"states", charge)?.filter(|&p| index.at(p).kind() == Kind::Map);
    let mut result = None;
    if let Some(states) = states {
        for name in states {
            let config = catalog
                .map(|p| super::lookup::binary(&index, p, name, charge))
                .transpose()?
                .flatten();
            let local = state_governance(&index, config, charge)?;
            aggregate(&mut result, rule(&index, base, local, charge)?, amount)?;
        }
    } else {
        aggregate(&mut result, rule(&index, base, None, charge)?, amount)?;
        if let Some(catalog) = catalog {
            for ordinal in 0..index.at(catalog).len() {
                pay(charge, 128)?;
                let (name, config) = index.at(catalog).row(ordinal).ok_or(Error::Malformed)?;
                let local = state_governance(
                    &index,
                    name.binary_bytes().map(|_| config.position()),
                    charge,
                )?;
                aggregate(&mut result, rule(&index, base, local, charge)?, amount)?;
            }
        }
    }
    if let Some(result) = &mut result {
        let version = field(&index, root, b"version", charge)?;
        result.config_version = config_version(&index, version)?;
        result.policy_version = version.and_then(|p| customer_version(&index, p));
    }
    Ok(result)
}

fn state_governance(
    index: &Index<'_, '_>,
    config: Option<Position>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Position>, Error> {
    let Some(config) = config else {
        return Ok(None);
    };
    if index.at(config).kind() != Kind::Map {
        return Err(Error::UnsupportedPolicyProfile);
    }
    Ok(field(index, config, b"governance", charge)?.filter(|&p| index.at(p).kind() == Kind::Map))
}

fn rule(
    index: &Index<'_, '_>,
    base: Position,
    local: Option<Position>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<LimitData>, Error> {
    let (base, local) = rule_maps(index, base, local, charge)?;
    let Some(limit) = merged_field(index, base, local, b"limit", charge)? else {
        return Ok(None);
    };
    let cursor = index.at(limit);
    if cursor.integer().is_none() && cursor.big_integer().is_some_and(|(negative, _)| !negative) {
        return Err(Error::UnsupportedPolicyProfile);
    }
    let Some(limit) = cursor.integer().filter(|n| *n >= 0) else {
        return Ok(None);
    };
    let limit = u64::try_from(limit)
        .ok()
        .filter(|n| *n <= MAX_EXACT_MILLIS)
        .ok_or(Error::UnsupportedPolicyProfile)?;
    let enforcement = merged_field(index, base, local, b"enforcement", charge)?;
    let enforcement = enforcement
        .map(|p| match index.at(p).atom_bytes() {
            Some(b"strict_global") => Ok(LimitEnforcement::StrictGlobal),
            Some(b"approximate_global") => Ok(LimitEnforcement::ApproximateGlobal),
            _ => Err(Error::UnsupportedPolicyProfile),
        })
        .transpose()?;
    let lease = merged_field(index, base, local, b"lease_size", charge)?;
    let lease_size = lease
        .map(|p| {
            index
                .at(p)
                .integer()
                .and_then(|n| u64::try_from(n).ok())
                .filter(|n| *n > 0 && *n <= MAX_EXACT_MILLIS)
                .ok_or(Error::UnsupportedPolicyProfile)
        })
        .transpose()?;
    Ok(Some(LimitData {
        limit,
        enforcement,
        lease_size,
    }))
}

fn rule_maps(
    index: &Index<'_, '_>,
    base: Position,
    local: Option<Position>,
    charge: &mut PolicySchemaCharge,
) -> Result<(Option<Position>, Option<Position>), Error> {
    let base_limits = field(index, base, b"limits", charge)?;
    let local_limits = local
        .map(|p| field(index, p, b"limits", charge))
        .transpose()?
        .flatten();
    if let Some(position) = local_limits.filter(|&p| index.at(p).kind() != Kind::Map) {
        return if index.at(position).atom_bytes() == Some(b"nil") {
            Ok((None, None))
        } else {
            Err(Error::UnsupportedPolicyProfile)
        };
    }
    if local_limits.is_none()
        && base_limits.is_some_and(|p| {
            index.at(p).kind() != Kind::Map && index.at(p).atom_bytes() != Some(b"nil")
        })
    {
        return Err(Error::UnsupportedPolicyProfile);
    }
    let base = base_limits
        .filter(|&p| index.at(p).kind() == Kind::Map)
        .map(|p| super::lookup::binary(index, p, b"running", charge))
        .transpose()?
        .flatten();
    let local = local_limits
        .map(|p| super::lookup::binary(index, p, b"running", charge))
        .transpose()?
        .flatten();
    if local.is_some_and(|p| index.at(p).kind() != Kind::Map) {
        return Ok((None, None));
    }
    Ok((base.filter(|&p| index.at(p).kind() == Kind::Map), local))
}

fn merged_field(
    index: &Index<'_, '_>,
    base: Option<Position>,
    local: Option<Position>,
    name: &[u8],
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Position>, Error> {
    let local = local
        .map(|p| field(index, p, name, charge))
        .transpose()?
        .flatten();
    if local.is_some() {
        return Ok(local);
    }
    base.map(|p| field(index, p, name, charge))
        .transpose()
        .map(Option::flatten)
}

fn config_version(index: &Index<'_, '_>, version: Option<Position>) -> Result<Option<u64>, Error> {
    if index.entry().generation() > 0 {
        return Ok(Some(index.entry().generation()));
    }
    let Some(version) = version else {
        return Ok(None);
    };
    let cursor = index.at(version);
    match cursor.integer() {
        Some(n) if n >= 0 => Ok(Some(
            u64::try_from(n).map_err(|_| Error::UnsupportedPolicyProfile)?,
        )),
        None if cursor.big_integer().is_some_and(|(negative, _)| !negative) => {
            Err(Error::UnsupportedPolicyProfile)
        }
        _ => Ok(None),
    }
}

fn customer_version<'raw>(index: &Index<'raw, '_>, p: Position) -> Option<Version<'raw>> {
    let cursor = index.at(p);
    if let Some(bytes) = cursor.binary_bytes().filter(|b| !b.is_empty()) {
        return Some(Version::Bytes(bytes));
    }
    cursor
        .integer()
        .and_then(|n| u64::try_from(n).ok())
        .filter(|n| *n <= MAX_EXACT_MILLIS)
        .map(Version::Number)
}
