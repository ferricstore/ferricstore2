//! One bounded raw metric and allocation-free field selection/validation.

use super::{MutationCallerError, MutationFieldValue, MutationInput};
use crate::workflow::{MAX_EXACT_MILLIS, admission::AdmissionLimits};

/// Measures every captured occurrence, including shadowed common options.
///
/// Each field slice is bounded before its contents are inspected. This is a
/// per-item metric; an outer caller must also admit its cumulative whole input.
/// # Errors
/// Refuses invalid caps/counts, arithmetic overflow or actual raw-byte excess.
pub fn measure_core_mutation_input(
    input: MutationInput<'_>,
    common: &[(&str, MutationFieldValue<'_>)],
    maximum_raw_bytes: usize,
) -> Result<usize, MutationCallerError> {
    if maximum_raw_bytes == 0 {
        return Err(MutationCallerError::InvalidLimits);
    }
    if input.fields.len() > 8 || common.len() > 8 {
        return Err(MutationCallerError::InvalidOptions);
    }
    let mut total = input
        .id
        .len()
        .checked_add(input.operation.len())
        .ok_or(MutationCallerError::LengthOverflow)?;
    for (name, value) in common.iter().chain(input.fields) {
        let width = match value {
            MutationFieldValue::Nil | MutationFieldValue::Unsupported => 0,
            MutationFieldValue::Bool(_) => 1,
            MutationFieldValue::Integer(_) => 16,
            MutationFieldValue::Bytes(bytes) => bytes.len(),
            MutationFieldValue::Retry(retry) => 32_usize
                .checked_add(retry.exhausted_to.as_ref().map_or(0, Vec::len))
                .ok_or(MutationCallerError::LengthOverflow)?,
        };
        total = total
            .checked_add(1)
            .and_then(|n| n.checked_add(name.len()))
            .and_then(|n| n.checked_add(width))
            .ok_or(MutationCallerError::LengthOverflow)?;
    }
    if total > maximum_raw_bytes {
        Err(MutationCallerError::RawBytes)
    } else {
        Ok(total)
    }
}

pub(super) fn validate_fields(
    input: MutationInput<'_>,
    common: &[(&str, MutationFieldValue<'_>)],
) -> Result<(), MutationCallerError> {
    for fields in [common, input.fields] {
        for (index, (name, _)) in fields.iter().enumerate() {
            if fields[..index].iter().any(|(prior, _)| prior == name) {
                return Err(MutationCallerError::DuplicateField);
            }
            validate_name(input.operation, name)?;
        }
    }
    Ok(())
}

fn validate_name(operation: &str, name: &str) -> Result<(), MutationCallerError> {
    let allowed: &[&str] = match operation {
        "create" => &["type", "state", "priority", "run_at_ms", "payload"],
        "transition" => &[
            "from",
            "to",
            "fencing_token",
            "lease_token",
            "run_at_ms",
            "payload",
        ],
        "complete" => &["lease_token", "fencing_token", "payload", "result"],
        "fail" => &["lease_token", "fencing_token", "payload", "error"],
        "retry" => &[
            "lease_token",
            "fencing_token",
            "run_at_ms",
            "retry",
            "payload",
            "error",
        ],
        "extend_lease" => &["lease_token", "fencing_token", "lease_ms"],
        "cancel" => &[
            "fencing_token",
            "lease_token",
            "expect_state",
            "expected_version",
            "reason",
        ],
        _ => return Err(MutationCallerError::InvalidField("operation")),
    };
    if matches!(name, "partition_key" | "now_ms") || allowed.contains(&name) {
        return Ok(());
    }
    let feature = match name {
        "priority" if operation == "transition" => "priority",
        "attributes" => "attributes",
        "parent_flow_id" => "parent_flow_id",
        "root_flow_id" => "root_flow_id",
        "correlation_id" => "correlation_id",
        "retention_ttl_ms" => "retention_ttl_ms",
        "ttl_ms" => "ttl_ms",
        "history_hot_max_events" => "history_hot_max_events",
        "history_max_events" => "history_max_events",
        "max_active_ms" => "max_active_ms",
        "idempotent" => "idempotent",
        "state_meta" => "state_meta",
        "payload_ref" | "result_ref" | "error_ref" => "value_reference",
        _ => return Err(MutationCallerError::UnknownField),
    };
    Err(MutationCallerError::UnsupportedFeature(feature))
}

pub(super) fn field<'a>(
    input: MutationInput<'a>,
    common: &[(&str, MutationFieldValue<'a>)],
    name: &str,
) -> Option<MutationFieldValue<'a>> {
    input
        .fields
        .iter()
        .chain(common)
        .find_map(|(key, value)| (name == *key).then_some(*value))
}

pub(super) fn bytes<'a>(
    input: MutationInput<'a>,
    common: &[(&str, MutationFieldValue<'a>)],
    name: &'static str,
    limits: AdmissionLimits,
    nil: bool,
    empty: bool,
) -> Result<Option<&'a [u8]>, MutationCallerError> {
    match field(input, common, name) {
        None => Ok(None),
        Some(MutationFieldValue::Nil) if nil => Ok(None),
        Some(MutationFieldValue::Bytes(value))
            if (empty || !value.is_empty()) && value.len() <= limits.inline_bytes() =>
        {
            Ok(Some(value))
        }
        Some(MutationFieldValue::Unsupported) => Err(MutationCallerError::UnsupportedFeature(name)),
        _ => Err(MutationCallerError::InvalidField(name)),
    }
}

pub(super) fn required_bytes<'a>(
    input: MutationInput<'a>,
    common: &[(&str, MutationFieldValue<'a>)],
    name: &'static str,
    limits: AdmissionLimits,
) -> Result<&'a [u8], MutationCallerError> {
    bytes(input, common, name, limits, false, false)?.ok_or(MutationCallerError::MissingField(name))
}

pub(super) fn number(
    input: MutationInput<'_>,
    common: &[(&str, MutationFieldValue<'_>)],
    name: &'static str,
) -> Result<Option<u64>, MutationCallerError> {
    match field(input, common, name) {
        None => Ok(None),
        Some(MutationFieldValue::Integer(value)) => u64::try_from(value)
            .ok()
            .filter(|value| *value <= MAX_EXACT_MILLIS)
            .map(Some)
            .ok_or(MutationCallerError::InvalidField(name)),
        Some(MutationFieldValue::Unsupported) => Err(MutationCallerError::UnsupportedFeature(name)),
        _ => Err(MutationCallerError::InvalidField(name)),
    }
}

pub(super) fn required_number(
    input: MutationInput<'_>,
    common: &[(&str, MutationFieldValue<'_>)],
    name: &'static str,
) -> Result<u64, MutationCallerError> {
    number(input, common, name)?.ok_or(MutationCallerError::MissingField(name))
}
