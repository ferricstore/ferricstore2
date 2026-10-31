//! Verify the full V2 owner once, then borrow its governance maps.

use super::super::{
    CompletePolicyValue, GovernanceData, LimitData, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, rules::merge_limit, verify_complete_policy_value_v2,
};
use super::{
    Error, ResolvedRunningClaimPolicy as Resolved, RunningClaimPolicyVersion as Version, aggregate,
    overflow, pay,
};
use crate::{policy::definition::PolicyVersion, workflow::MAX_EXACT_MILLIS};

pub(super) fn resolve<'v>(
    value: &'v CompletePolicyValue,
    kind: &[u8],
    states: Option<&[&[u8]]>,
    amount: u64,
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<Resolved<'v>>, Error> {
    verify_complete_policy_value_v2(value, schema, charge)?;
    let data = value.data();
    pay(
        charge,
        kind.len()
            .checked_add(data.workflow_type.len())
            .and_then(|n| n.checked_add(128))
            .ok_or_else(overflow)?,
    )?;
    if data.workflow_type != kind {
        return Err(Error::Schema(PolicySchemaError::InvalidAuthority));
    }
    let base = running(data.governance.as_ref(), charge)?;
    let mut result = None;
    if let Some(states) = states {
        for name in states {
            pay(charge, name.len().checked_add(128).ok_or_else(overflow)?)?;
            let local = running(
                data.states
                    .get(*name)
                    .and_then(|state| state.governance.as_ref()),
                charge,
            )?;
            aggregate(&mut result, merge_limit(base, local), amount)?;
        }
    } else {
        aggregate(&mut result, base.copied(), amount)?;
        for (name, state) in &data.states {
            pay(charge, name.len().checked_add(128).ok_or_else(overflow)?)?;
            let local = running(state.governance.as_ref(), charge)?;
            aggregate(&mut result, merge_limit(base, local), amount)?;
        }
    }
    if let Some(result) = &mut result {
        result.config_version = Some(value.generation());
        result.policy_version = data.version.as_ref().and_then(|version| match version {
            PolicyVersion::Number(n) if *n <= MAX_EXACT_MILLIS => Some(Version::Number(*n)),
            PolicyVersion::Bytes(bytes) if !bytes.is_empty() => Some(Version::Bytes(bytes)),
            _ => None,
        });
    }
    Ok(result)
}

fn running<'v>(
    governance: Option<&'v GovernanceData>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<&'v LimitData>, Error> {
    pay(charge, 128 + b"running".len())?;
    Ok(governance
        .and_then(|g| g.limits.as_ref())
        .and_then(|rules| rules.get(b"running".as_slice())))
}
