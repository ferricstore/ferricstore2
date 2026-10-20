//! Exact fixed and captured-history retained business grammar, without allocation.

use super::{
    Error, Field, Gov, Kind, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, Reason,
    Tracking, charge_schema, overflow, parts,
};
use crate::policy::complete::{
    PolicyIndexedInputRejection as Indexed, PolicyRetentionInputRejection as Retention,
    PolicyRetryInputRejection as Retry,
};
use crate::policy::installation::PolicyError;
use crate::workflow::request::{PolicyRejection, RetainedStatus};

/// Restores only an exact original producer diagnosis, including its captured maximum.
/// # Errors
/// Preserves profile and cumulative work/length refusal; unmatched bytes return None.
pub fn original_policy_set_rejection_from_body(
    response: &[u8],
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<RetainedStatus>, Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    pay(charge, 128)?;
    for (error, body) in [
        (
            PolicyError::InvalidGeneration,
            b"ERR invalid flow policy generation".as_slice(),
        ),
        (
            PolicyError::StaleGeneration,
            b"ERR stale flow policy generation".as_slice(),
        ),
        (
            PolicyError::GenerationExhausted,
            b"ERR flow policy generation exhausted".as_slice(),
        ),
    ] {
        if matches_parts(response, [body, b"", b"", b""], charge)? {
            return Ok(Some(RetainedStatus::PolicyRejected(
                PolicyRejection::new(error)
                    .map_err(|_| Error::Schema(PolicySchemaError::InvalidAuthority))?,
            )));
        }
    }
    for reason in FIXED {
        if matches_reason(response, *reason, charge)? {
            return Ok(Some(RetainedStatus::OriginalPolicySetRejected(*reason)));
        }
    }
    for field in [
        Field::Allowed,
        Field::Denied,
        Field::ApprovalEffects,
        Field::ApprovalStates,
        Field::Assignees,
        Field::ErrorClasses,
    ] {
        for reason in [Gov::StringList(field), Gov::ListBytes(field)] {
            let reason = Reason::Governance(reason);
            if matches_reason(response, reason, charge)? {
                return Ok(Some(RetainedStatus::OriginalPolicySetRejected(reason)));
            }
        }
    }
    for field in [
        Field::LeaseSize,
        Field::WindowMillis,
        Field::FailureThreshold,
        Field::OpenMillis,
        Field::MinCalls,
        Field::LatencyMillis,
        Field::HalfOpenProbes,
        Field::HalfOpenSuccesses,
        Field::ApprovalTimeout,
    ] {
        let reason = Reason::Governance(Gov::Positive(field));
        if matches_reason(response, reason, charge)? {
            return Ok(Some(RetainedStatus::OriginalPolicySetRejected(reason)));
        }
    }
    for kind in [Kind::Limits, Kind::Budgets, Kind::Circuits] {
        for reason in [
            Gov::RuleMapShape(kind),
            Gov::RuleShape(kind),
            Gov::RuleCount(kind),
            Gov::RuleName(kind),
            Gov::RuleNameBytes(kind),
        ] {
            let reason = Reason::Governance(reason);
            if matches_reason(response, reason, charge)? {
                return Ok(Some(RetainedStatus::OriginalPolicySetRejected(reason)));
            }
        }
    }
    let (prefix, _) =
        crate::policy::complete::retention_input::rejection_parts(Retention::History {
            maximum: 1,
        })?;
    pay(
        charge,
        128_usize
            .checked_add(response.len())
            .and_then(|n| n.checked_add(prefix.len()))
            .ok_or_else(overflow)?,
    )?;
    let Some(digits) = response.strip_prefix(prefix) else {
        return Ok(None);
    };
    Ok(decimal(digits, charge)?.map(|maximum| {
        RetainedStatus::OriginalPolicySetRejected(Reason::Retention(Retention::History { maximum }))
    }))
}

fn matches_reason(
    response: &[u8],
    reason: Reason,
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    let (parts, maximum) = parts(reason)?;
    if maximum.is_some() {
        return Err(Error::Schema(PolicySchemaError::InvalidAuthority));
    }
    matches_parts(response, parts, charge)
}
fn matches_parts(
    response: &[u8],
    parts: [&[u8]; 4],
    charge: &mut PolicySchemaCharge,
) -> Result<bool, Error> {
    let length = parts
        .iter()
        .try_fold(0_usize, |n, p| n.checked_add(p.len()))
        .ok_or_else(overflow)?;
    pay(
        charge,
        128_usize
            .checked_add(response.len())
            .and_then(|n| n.checked_add(length))
            .ok_or_else(overflow)?,
    )?;
    if response.len() != length {
        return Ok(false);
    }
    let mut offset = 0;
    for part in parts {
        let end = offset + part.len();
        if response.get(offset..end) != Some(part) {
            return Ok(false);
        }
        offset = end;
    }
    Ok(true)
}
fn decimal(digits: &[u8], charge: &mut PolicySchemaCharge) -> Result<Option<u32>, Error> {
    pay(
        charge,
        128_usize.checked_add(digits.len()).ok_or_else(overflow)?,
    )?;
    if digits.is_empty() || digits[0] == b'0' || digits.len() > 7 {
        return Ok(None);
    }
    let mut value = 0_u32;
    for &digit in digits {
        if !digit.is_ascii_digit() {
            return Ok(None);
        }
        value = value * 10 + u32::from(digit - b'0');
    }
    Ok((value <= crate::retention::MAX_HISTORY_EVENTS).then_some(value))
}
fn pay(charge: &mut PolicySchemaCharge, amount: usize) -> Result<(), Error> {
    charge_schema(charge, u64::try_from(amount).map_err(|_| overflow())?, 0).map_err(Error::Schema)
}

const FIXED: &[Reason] = &[
    Reason::RootShape,
    Reason::RootKeyword,
    Reason::TopStateMode,
    Reason::Version,
    Reason::MaxActive,
    Reason::StatesShape,
    Reason::StatesList,
    Reason::StateName,
    Reason::RunningState,
    Reason::StatePolicyShape,
    Reason::StateMaxActive,
    Reason::StateMode,
    Reason::SnapshotBytes,
    Reason::Retry(Retry::Shape),
    Reason::Retry(Retry::DeprecatedMaxAttempts),
    Reason::Retry(Retry::MaxRetries),
    Reason::Retry(Retry::BackoffShape),
    Reason::Retry(Retry::BackoffKind),
    Reason::Retry(Retry::BaseMillis),
    Reason::Retry(Retry::MaxMillis),
    Reason::Retry(Retry::JitterPercent),
    Reason::Retry(Retry::ExhaustedTo),
    Reason::Retry(Retry::RunningExhaustion),
    Reason::Indexed(Indexed::AttributesList),
    Reason::Indexed(Indexed::AttributeString),
    Reason::Indexed(Indexed::AttributeUtf8),
    Reason::Indexed(Indexed::AttributeEmpty),
    Reason::Indexed(Indexed::AttributeBytes),
    Reason::Indexed(Indexed::AttributeReserved),
    Reason::Indexed(Indexed::AttributesCount),
    Reason::Indexed(Indexed::StateMetaShape),
    Reason::Indexed(Indexed::StateMetaUtf8),
    Reason::Indexed(Indexed::StateMetaEmpty),
    Reason::Indexed(Indexed::StateMetaBytes),
    Reason::Indexed(Indexed::StateMetaReserved),
    Reason::Indexed(Indexed::StateMetaCount),
    Reason::Retention(Retention::Shape),
    Reason::Retention(Retention::Ttl),
    Reason::Retention(Retention::HistoryHot),
    Reason::Governance(Gov::Shape),
    Reason::Governance(Gov::Mode),
    Reason::Governance(Gov::EffectsShape),
    Reason::Governance(Gov::AuditShape),
    Reason::Governance(Gov::ApprovalsShape),
    Reason::Governance(Gov::Denials),
    Reason::Governance(Gov::Enforcement),
    Reason::Governance(Gov::RequireIdempotencyBoolean),
    Reason::Governance(Gov::EffectEventsBoolean),
    Reason::Governance(Gov::ListCount),
    Reason::Governance(Gov::ListValue),
    Reason::Governance(Gov::Limit),
    Reason::Governance(Gov::Unit),
    Reason::Governance(Gov::Percent),
    Reason::Governance(Gov::PolicyBytes),
    Reason::Governance(Gov::Tracking(Tracking::MinCallsAbove64)),
    Reason::Governance(Gov::Tracking(Tracking::ThresholdAbove64WithoutRate)),
    Reason::Governance(Gov::Tracking(Tracking::MissingMinCallsForLargeThreshold)),
];
