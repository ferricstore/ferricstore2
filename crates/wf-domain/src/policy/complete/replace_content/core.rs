//! Primitive scalar decisions within the already paid semantic stream.

use crate::policy::RetryOverride;
use crate::policy::definition::PolicyVersion;
use crate::policy::installation::PolicyError;
use crate::retention::{MAX_RETENTION_MILLIS, RetentionError, RetentionOverride};
use crate::value::{DomainError, RetryPolicyField};

use super::super::{CompletePolicyData, CompleteStatePolicyData, PolicySchemaError};
use super::CompletePolicyReplaceRejectionKind as Kind;

pub(super) fn check_replace_core(
    data: &CompletePolicyData,
    history_limit: u32,
) -> Result<Option<Kind>, PolicySchemaError> {
    if data.workflow_type.is_empty() {
        return Ok(Some(Kind::EmptyType));
    }
    if matches!(&data.version, Some(PolicyVersion::Bytes(bytes)) if bytes.is_empty()) {
        return Ok(Some(Kind::InvalidVersion));
    }
    if data
        .max_active_ms
        .is_some_and(|value| value == 0 || value > MAX_RETENTION_MILLIS)
    {
        return Ok(Some(Kind::InvalidMaxActive));
    }
    if let Some(retry) = &data.retry
        && let Some(kind) = check_replace_retry(retry)?
    {
        return Ok(Some(kind));
    }
    Ok(check_replace_retention(data.retention, history_limit))
}

pub(super) fn check_replace_state(
    name: &[u8],
    state: &CompleteStatePolicyData,
    history_limit: u32,
) -> Result<Option<Kind>, PolicySchemaError> {
    if name.is_empty() {
        return Ok(Some(Kind::EmptyState));
    }
    if name == b"running" {
        return Ok(Some(Kind::RunningState));
    }
    // Represented state-mode variants are already valid, without normalization.
    if let Some(retry) = &state.retry
        && let Some(kind) = check_replace_retry(retry)?
    {
        return Ok(Some(kind));
    }
    Ok(check_replace_retention(state.retention, history_limit))
}

fn check_replace_retry(retry: &RetryOverride) -> Result<Option<Kind>, PolicySchemaError> {
    let Err(error) = retry.validate() else {
        return Ok(None);
    };
    let kind = match error {
        DomainError::InvalidRetryPolicy(field) => match field {
            RetryPolicyField::MaxRetries => Kind::RetryMaxRetries,
            RetryPolicyField::BaseMillis => Kind::RetryBaseMillis,
            RetryPolicyField::MaxMillis => Kind::RetryMaxMillis,
            RetryPolicyField::JitterPercent => Kind::RetryJitterPercent,
            RetryPolicyField::ExhaustedTo => Kind::RetryExhaustedTo,
            RetryPolicyField::RunningExhaustion => Kind::RetryRunningExhaustion,
            _ => return Err(PolicySchemaError::Policy(PolicyError::Domain(error))),
        },
        _ => return Err(PolicySchemaError::Policy(PolicyError::Domain(error))),
    };
    Ok(Some(kind))
}

fn check_replace_retention(
    retention: Option<RetentionOverride>,
    history_limit: u32,
) -> Option<Kind> {
    match retention?.validate(history_limit) {
        Ok(()) => None,
        Err(RetentionError::Ttl) => Some(Kind::RetentionTtl),
        Err(RetentionError::History) => Some(Kind::RetentionHistory {
            maximum: history_limit,
        }),
    }
}
