//! Exact finite borrowed response parts and one final fallible byte owner.

use super::{Error, FlowPolicyInputRejection as Reason, GovernancePolicyInputRejection as Gov};
use super::{PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, charge_schema};
use crate::policy::complete::retention_input;
use crate::policy::complete::{
    CircuitTrackingFailure as Tracking, GovernanceField as Field, GovernanceRuleKind as Kind,
};

pub(super) fn encode(
    reason: Reason,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    let (parts, maximum) = parts(reason)?;
    let digits = maximum.map_or(0, retention_input::decimal_length);
    let length = parts
        .iter()
        .try_fold(digits, |sum, part| sum.checked_add(part.len()))
        .ok_or_else(overflow)?;
    charge_schema(charge, u64::try_from(digits).map_err(|_| overflow())?, 0)
        .map_err(Error::Schema)?;
    super::value::visit(charge, length)?;
    let mut scratch = [0; 7];
    let decimal = match maximum {
        Some(maximum) => retention_input::write_decimal(maximum, &mut scratch)?,
        None => &[],
    };
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(length)
        .map_err(|_| Error::AllocationFailure)?;
    for part in parts {
        owned.extend_from_slice(part);
    }
    owned.extend_from_slice(decimal);
    Ok(owned)
}

pub(in crate::policy::complete) fn body_len(reason: Reason) -> Result<usize, Error> {
    let (parts, maximum) = parts(reason)?;
    let digits = maximum.map_or(0, retention_input::decimal_length);
    parts
        .iter()
        .try_fold(digits, |sum, part| sum.checked_add(part.len()))
        .ok_or_else(overflow)
}

pub(in crate::policy::complete) fn parts(
    reason: Reason,
) -> Result<([&'static [u8]; 4], Option<u32>), Error> {
    let bytes = match reason {
        Reason::RootShape => b"ERR flow policy must be a map or keyword list".as_slice(),
        Reason::RootKeyword => b"ERR flow opts must be a keyword list",
        Reason::TopStateMode => b"ERR flow state mode is state-level only",
        Reason::Version => {
            b"ERR flow policy version must be a non-empty string or non-negative integer"
        }
        Reason::MaxActive => {
            b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity"
        }
        Reason::StatesShape => b"ERR flow policy states must be a map or keyword list",
        Reason::StatesList => {
            b"ERR flow policy states must be a map, keyword list, or state-policy pair list"
        }
        Reason::StateName => b"ERR flow policy state must be a non-empty string",
        Reason::RunningState => b"ERR flow policy state cannot be running",
        Reason::StatePolicyShape => b"ERR flow state policy must be a map or keyword list",
        Reason::StateMaxActive => b"ERR flow max_active_ms is type-level only",
        Reason::StateMode => b"ERR flow state mode must be parallel or fifo",
        Reason::SnapshotBytes => b"ERR flow policy snapshot exceeds 262144 bytes",
        Reason::Retry(reason) => reason.as_bytes(),
        Reason::Indexed(reason) => reason.message(),
        Reason::Retention(reason) => {
            let (prefix, maximum) = retention_input::rejection_parts(reason)?;
            return Ok((fixed(prefix), maximum));
        }
        Reason::Governance(reason) => return Ok((governance(reason)?, None)),
    };
    Ok((fixed(bytes), None))
}

fn governance(reason: Gov) -> Result<[&'static [u8]; 4], Error> {
    let bytes = match reason {
        Gov::Shape => b"ERR flow governance policy must be a map or keyword list".as_slice(),
        Gov::Mode => b"ERR invalid flow governance mode",
        Gov::EffectsShape => b"ERR flow governance effects policy must be a map or keyword list",
        Gov::AuditShape => b"ERR flow governance audit policy must be a map or keyword list",
        Gov::ApprovalsShape => {
            b"ERR flow governance approvals policy must be a map or keyword list"
        }
        Gov::Denials => b"ERR invalid flow governance denials audit mode",
        Gov::Enforcement => b"ERR invalid flow governance enforcement mode",
        Gov::RequireIdempotencyBoolean => {
            b"ERR flow governance require_idempotency_key must be boolean"
        }
        Gov::EffectEventsBoolean => b"ERR flow governance effect_events must be boolean",
        Gov::ListCount => b"ERR flow governance list must contain at most 1000 values",
        Gov::ListValue => b"ERR flow governance list values must be non-empty strings",
        Gov::Limit => b"ERR flow governance limit must be a non-negative integer",
        Gov::Unit => b"ERR flow governance unit must be a non-empty string of at most 65535 bytes",
        Gov::Percent => b"ERR flow governance failure_rate_pct must be an integer from 1 to 100",
        Gov::PolicyBytes => b"ERR flow governance policy exceeds 1048576-byte durable limit",
        Gov::Tracking(failure) => return Ok(fixed(tracking(failure))),
        Gov::StringList(field) => {
            return Ok(field_parts(
                list_field(field)?,
                b" must be a list of strings",
            ));
        }
        Gov::ListBytes(field) => {
            return Ok(field_parts(
                list_field(field)?,
                if field == Field::ErrorClasses {
                    b" values must be at most 256 bytes"
                } else {
                    b" values must be at most 65535 bytes"
                },
            ));
        }
        Gov::Positive(field) => {
            return Ok(field_parts(
                positive_field(field)?,
                b" must be a positive integer",
            ));
        }
        Gov::RuleMapShape(kind) => {
            return Ok(field_parts(
                plural(kind),
                b" policy must be a map or keyword list",
            ));
        }
        Gov::RuleShape(kind) => {
            return Ok(field_parts(
                singular(kind),
                b" rule must be a map or keyword list",
            ));
        }
        Gov::RuleCount(kind) => {
            return Ok(field_parts(
                plural(kind),
                b" policy must contain at most 1000 rules",
            ));
        }
        Gov::RuleName(kind) => {
            return Ok(field_parts(
                plural(kind),
                b" names must be non-empty strings",
            ));
        }
        Gov::RuleNameBytes(kind) => {
            return Ok(field_parts(
                plural(kind),
                b" names must be at most 65535 bytes",
            ));
        }
    };
    Ok(fixed(bytes))
}

fn list_field(field: Field) -> Result<&'static [u8], Error> {
    Ok(match field {
        Field::Allowed => b"allowed",
        Field::Denied => b"denied",
        Field::ApprovalEffects => b"effects",
        Field::ApprovalStates => b"states",
        Field::Assignees => b"assignees",
        Field::ErrorClasses => b"error_classes",
        _ => return Err(invalid()),
    })
}

fn positive_field(field: Field) -> Result<&'static [u8], Error> {
    Ok(match field {
        Field::LeaseSize => b"lease_size",
        Field::WindowMillis => b"window_ms",
        Field::FailureThreshold => b"failure_threshold",
        Field::OpenMillis => b"open_ms",
        Field::MinCalls => b"min_calls",
        Field::LatencyMillis => b"latency_threshold_ms",
        Field::HalfOpenProbes => b"half_open_max_probes",
        Field::HalfOpenSuccesses => b"half_open_success_threshold",
        Field::ApprovalTimeout => b"timeout_ms",
        _ => return Err(invalid()),
    })
}

fn tracking(failure: Tracking) -> &'static [u8] {
    match failure {
        Tracking::MinCallsAbove64 => b"ERR flow governance circuit min_calls must be <= 64",
        Tracking::ThresholdAbove64WithoutRate => {
            b"ERR flow governance circuit failure_threshold must be <= 64 without failure_rate_pct"
        }
        Tracking::MissingMinCallsForLargeThreshold => {
            b"ERR flow governance circuit min_calls is required when failure_threshold exceeds 64"
        }
    }
}

fn plural(kind: Kind) -> &'static [u8] {
    match kind {
        Kind::Limits => b"limits",
        Kind::Budgets => b"budgets",
        Kind::Circuits => b"circuits",
    }
}
fn singular(kind: Kind) -> &'static [u8] {
    match kind {
        Kind::Limits => b"limit",
        Kind::Budgets => b"budget",
        Kind::Circuits => b"circuit",
    }
}
fn fixed(bytes: &'static [u8]) -> [&'static [u8]; 4] {
    [bytes, b"", b"", b""]
}
fn field_parts(field: &'static [u8], suffix: &'static [u8]) -> [&'static [u8]; 4] {
    [b"ERR flow governance ", field, suffix, b""]
}
fn invalid() -> Error {
    Error::Schema(PolicySchemaError::InvalidAuthority)
}
fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}
