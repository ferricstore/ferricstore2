//! Source defaults and guards without time generation, policy or record reads.

use wf_types::bounds::MAX_RECORD_ID_BYTES;

use super::input::{bytes, field, number, required_bytes, required_number, validate_fields};
use super::{
    CoreMutation, MutationCallerError, MutationFieldValue, MutationInput, NormalizedMutation,
};
use crate::workflow::{MAX_EXACT_MILLIS, admission::AdmissionLimits};

/// Normalizes the selected scalar source schema into borrowed core effects.
///
/// Common options are overridden by item options. No caller bytes are copied;
/// generated time, automatic partition construction and Fresh policy stay outside.
/// # Errors
/// Returns the first unsupported/invalid represented field or raw admission error.
pub fn normalize_core_mutation<'a>(
    input: MutationInput<'a>,
    common: &'a [(&'a str, MutationFieldValue<'a>)],
    base_partition: Option<&'a [u8]>,
    limits: AdmissionLimits,
    maximum_raw_bytes: usize,
) -> Result<NormalizedMutation<'a>, MutationCallerError> {
    super::measure_core_mutation_input(input, common, maximum_raw_bytes)?;
    validate_fields(input, common)?;
    if input.id.is_empty()
        || input.id.len() > MAX_RECORD_ID_BYTES
        || input.id.starts_with(b"__ferricstore_schedule__:")
    {
        return Err(MutationCallerError::InvalidId);
    }
    let partition = partition(input, common, base_partition, limits)?;
    let requested_now_ms = number(input, common, "now_ms")?;
    let operation = operation(input, common, requested_now_ms, limits)?;
    Ok(NormalizedMutation {
        id: input.id,
        partition,
        requested_now_ms,
        operation,
    })
}

fn partition<'a>(
    input: MutationInput<'a>,
    common: &[(&str, MutationFieldValue<'a>)],
    base: Option<&'a [u8]>,
    limits: AdmissionLimits,
) -> Result<Option<&'a [u8]>, MutationCallerError> {
    if base.is_some_and(|value| value.is_empty() || value.len() > limits.inline_bytes()) {
        return Err(MutationCallerError::InvalidField("partition_key"));
    }
    let selected = bytes(
        input,
        common,
        "partition_key",
        limits,
        base.is_none(),
        false,
    )?;
    match (base, selected) {
        (Some(base), Some(item)) if base != item => Err(MutationCallerError::PartitionMismatch),
        (Some(base), _) => Ok(Some(base)),
        (None, item) => Ok(item),
    }
}

fn operation<'a>(
    input: MutationInput<'a>,
    common: &[(&str, MutationFieldValue<'a>)],
    now: Option<u64>,
    limits: AdmissionLimits,
) -> Result<CoreMutation<'a>, MutationCallerError> {
    match input.operation {
        "create" => create(input, common, now, limits),
        "transition" => Ok(CoreMutation::Transition {
            from: required_bytes(input, common, "from", limits)?,
            to: state(required_bytes(input, common, "to", limits)?, "to")?,
            fence: required_number(input, common, "fencing_token")?,
            lease_token: bytes(input, common, "lease_token", limits, true, false)?,
            run_at_ms: number(input, common, "run_at_ms")?.or(now),
            payload: bytes(input, common, "payload", limits, false, true)?,
        }),
        "complete" | "fail" => terminal(input, common, limits),
        "retry" => retry(input, common, limits),
        "extend_lease" => extend(input, common, now, limits),
        "cancel" => Ok(CoreMutation::Cancel {
            fence: required_number(input, common, "fencing_token")?,
            lease_token: bytes(input, common, "lease_token", limits, true, false)?,
            expected_state: bytes(input, common, "expect_state", limits, true, false)?,
            expected_version: number(input, common, "expected_version")?,
            reason: bytes(input, common, "reason", limits, false, true)?,
        }),
        _ => Err(MutationCallerError::InvalidField("operation")),
    }
}

fn create<'a>(
    input: MutationInput<'a>,
    common: &[(&str, MutationFieldValue<'a>)],
    now: Option<u64>,
    limits: AdmissionLimits,
) -> Result<CoreMutation<'a>, MutationCallerError> {
    let workflow_type = required_bytes(input, common, "type", limits)?;
    if workflow_type == b"__ferricstore_schedule" {
        return Err(MutationCallerError::InvalidField("type"));
    }
    let initial = bytes(input, common, "state", limits, false, false)?.unwrap_or(b"queued");
    let priority = number(input, common, "priority")?.unwrap_or(0);
    if priority > 2 {
        return Err(MutationCallerError::InvalidField("priority"));
    }
    Ok(CoreMutation::Create {
        workflow_type,
        state: state(initial, "state")?,
        priority: u8::try_from(priority)
            .map_err(|_| MutationCallerError::InvalidField("priority"))?,
        run_at_ms: number(input, common, "run_at_ms")?.or(now),
        payload: bytes(input, common, "payload", limits, false, true)?,
    })
}

fn state<'a>(value: &'a [u8], name: &'static str) -> Result<&'a [u8], MutationCallerError> {
    if value == b"running" {
        Err(MutationCallerError::InvalidField(name))
    } else {
        Ok(value)
    }
}

fn terminal<'a>(
    input: MutationInput<'a>,
    common: &[(&str, MutationFieldValue<'a>)],
    limits: AdmissionLimits,
) -> Result<CoreMutation<'a>, MutationCallerError> {
    let lease_token = required_bytes(input, common, "lease_token", limits)?;
    let fence = required_number(input, common, "fencing_token")?;
    let payload = bytes(input, common, "payload", limits, false, true)?;
    if input.operation == "complete" {
        Ok(CoreMutation::Complete {
            lease_token,
            fence,
            payload,
            value: bytes(input, common, "result", limits, false, true)?,
        })
    } else {
        Ok(CoreMutation::Fail {
            lease_token,
            fence,
            payload,
            value: bytes(input, common, "error", limits, false, true)?,
        })
    }
}

fn retry<'a>(
    input: MutationInput<'a>,
    common: &[(&str, MutationFieldValue<'a>)],
    limits: AdmissionLimits,
) -> Result<CoreMutation<'a>, MutationCallerError> {
    let retry = match field(input, common, "retry") {
        None | Some(MutationFieldValue::Nil) => None,
        Some(MutationFieldValue::Retry(retry)) => {
            retry
                .validate()
                .map_err(|_| MutationCallerError::InvalidField("retry"))?;
            if retry
                .exhausted_to
                .as_ref()
                .is_some_and(|value| value.len() > limits.inline_bytes())
            {
                return Err(MutationCallerError::InvalidField("retry"));
            }
            Some(retry)
        }
        Some(MutationFieldValue::Unsupported) => {
            return Err(MutationCallerError::UnsupportedFeature("retry"));
        }
        _ => return Err(MutationCallerError::InvalidField("retry")),
    };
    Ok(CoreMutation::Retry {
        lease_token: required_bytes(input, common, "lease_token", limits)?,
        fence: required_number(input, common, "fencing_token")?,
        run_at_ms: number(input, common, "run_at_ms")?,
        retry,
        payload: bytes(input, common, "payload", limits, false, true)?,
        error: bytes(input, common, "error", limits, false, true)?,
    })
}

fn extend<'a>(
    input: MutationInput<'a>,
    common: &[(&str, MutationFieldValue<'a>)],
    now: Option<u64>,
    limits: AdmissionLimits,
) -> Result<CoreMutation<'a>, MutationCallerError> {
    let lease_ms = number(input, common, "lease_ms")?.unwrap_or(30_000);
    if lease_ms == 0
        || now.is_some_and(|now| {
            now.checked_add(lease_ms)
                .is_none_or(|deadline| deadline > MAX_EXACT_MILLIS)
        })
    {
        return Err(MutationCallerError::InvalidField("lease_ms"));
    }
    Ok(CoreMutation::ExtendLease {
        lease_token: required_bytes(input, common, "lease_token", limits)?,
        fence: required_number(input, common, "fencing_token")?,
        lease_ms,
    })
}
