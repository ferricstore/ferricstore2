//! Existing leaf policy projection; the complete backing remains authoritative.

use super::{
    ClaimTransactionError, ClaimTransactionLimits, InstalledRetryPolicy, PolicySchemaCharge,
    PolicySnapshot, StateTransactionError, StoredPolicyValue, debit,
    original_record_batch_policy_has_governance, original_record_batch_policy_projection_bytes,
    project_original_record_batch_policy, project_record_batch_policy,
    record_batch_policy_projection_bytes,
};

pub(super) fn policy_projection(
    policy: &StoredPolicyValue,
    kind: &[u8],
    limits: ClaimTransactionLimits,
    charge: &mut PolicySchemaCharge,
    staging_used: &mut usize,
    cap: usize,
    capture: Option<wf_ports::log::CapturedGovernedClaimPayment<'_>>,
) -> Result<(PolicySnapshot, InstalledRetryPolicy), ClaimTransactionError> {
    Ok(match policy {
        StoredPolicyValue::Complete(policy) => {
            if policy.data().workflow_type != kind {
                return Err(StateTransactionError::Corrupt.into());
            }
            if capture.is_none()
                && (policy.data().governance.is_some()
                    || policy
                        .data()
                        .states
                        .values()
                        .any(|s| s.governance.is_some()))
            {
                return Err(ClaimTransactionError::UnsupportedGovernance);
            }
            let bytes = record_batch_policy_projection_bytes(policy)?;
            debit(staging_used, bytes, cap)?;
            let (mode, retry) =
                project_record_batch_policy(policy, limits.base.schema, charge, bytes)?;
            (mode.into_policy(), retry)
        }
        StoredPolicyValue::Original(raw) => {
            let owner = raw
                .len()
                .checked_mul(256)
                .and_then(|n| n.checked_add(65_536))
                .ok_or(StateTransactionError::Limit)?;
            debit(staging_used, owner, cap)?;
            if capture.is_none()
                && original_record_batch_policy_has_governance(
                    raw,
                    kind,
                    limits.base.schema,
                    charge,
                )?
            {
                return Err(ClaimTransactionError::UnsupportedGovernance);
            }
            debit(staging_used, owner, cap)?;
            let bytes = original_record_batch_policy_projection_bytes(
                raw,
                kind,
                limits.base.schema,
                charge,
            )?;
            debit(
                staging_used,
                owner
                    .checked_add(bytes)
                    .ok_or(StateTransactionError::Limit)?,
                cap,
            )?;
            project_original_record_batch_policy(raw, kind, limits.base.schema, charge, bytes)?
        }
    })
}

// Only the real atomic parent uses this mode/retry projection, after current
// running governance has been checked under its same unchanged writer.
pub(super) fn atomic_projection(
    policy: &StoredPolicyValue,
    kind: &[u8],
    limits: ClaimTransactionLimits,
    charge: &mut PolicySchemaCharge,
    staging_used: &mut usize,
) -> Result<(PolicySnapshot, InstalledRetryPolicy), ClaimTransactionError> {
    let cap = limits.base.maximum_staging_bytes;
    match policy {
        StoredPolicyValue::Complete(policy) => {
            if policy.data().workflow_type != kind {
                return Err(StateTransactionError::Corrupt.into());
            }
            let bytes = record_batch_policy_projection_bytes(policy)?;
            debit(staging_used, bytes, cap)?;
            let (mode, retry) =
                project_record_batch_policy(policy, limits.base.schema, charge, bytes)?;
            Ok((mode.into_policy(), retry))
        }
        StoredPolicyValue::Original(raw) => {
            let owner = raw
                .len()
                .checked_mul(256)
                .and_then(|n| n.checked_add(65_536))
                .ok_or(StateTransactionError::Limit)?;
            debit(staging_used, owner, cap)?;
            let bytes = original_record_batch_policy_projection_bytes(
                raw,
                kind,
                limits.base.schema,
                charge,
            )?;
            debit(
                staging_used,
                owner
                    .checked_add(bytes)
                    .ok_or(StateTransactionError::Limit)?,
                cap,
            )?;
            Ok(project_original_record_batch_policy(
                raw,
                kind,
                limits.base.schema,
                charge,
                bytes,
            )?)
        }
    }
}
