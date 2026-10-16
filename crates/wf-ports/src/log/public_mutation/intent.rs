//! Static UTF-8 ordered registry, sharing measurement and SHA256 streaming.

use sha2::{Digest, Sha256};
use wf_domain::policy::{BackoffKind, RetryOverride};
use wf_domain::workflow::caller::{CoreMutation, NormalizedMutation};
use wf_domain::workflow::request::ClientIntent;

use super::super::CodecError;

const KEYS: [&str; 19] = [
    "error",
    "expected_state",
    "expected_version",
    "fencing_token",
    "from_state",
    "initial_state",
    "lease_ms",
    "lease_token",
    "partition",
    "payload",
    "priority",
    "reason",
    "requested_now_ms",
    "result",
    "retry",
    "run_at_ms",
    "to_state",
    "workflow_id",
    "workflow_type",
];

pub(super) fn measure(
    tenant: &[u8],
    caller: &NormalizedMutation<'_>,
    maximum: usize,
) -> Result<usize, CodecError> {
    let mut size = 0usize;
    canonical(tenant, caller, &mut |bytes| {
        size = size
            .checked_add(bytes.len())
            .ok_or(CodecError::LengthOverflow)?;
        if size > maximum {
            return Err(CodecError::ByteLimit);
        }
        Ok(())
    })?;
    Ok(size)
}

pub(super) fn prepare(
    tenant: &[u8],
    caller: &NormalizedMutation<'_>,
    maximum: usize,
) -> Result<ClientIntent, CodecError> {
    measure(tenant, caller, maximum)?;
    let mut hash = Sha256::new();
    canonical(tenant, caller, &mut |bytes| {
        hash.update(bytes);
        Ok(())
    })?;
    ClientIntent::new(1, hash.finalize().into()).map_err(|_| CodecError::UnsupportedVersion)
}

fn header(
    tag: u8,
    length: usize,
    out: &mut dyn FnMut(&[u8]) -> Result<(), CodecError>,
) -> Result<(), CodecError> {
    out(&[tag])?;
    out(&u64::try_from(length)
        .map_err(|_| CodecError::LengthOverflow)?
        .to_be_bytes())
}

fn scalar(
    tag: u8,
    value: &[u8],
    out: &mut dyn FnMut(&[u8]) -> Result<(), CodecError>,
) -> Result<(), CodecError> {
    header(tag, value.len(), out)?;
    out(value)
}

fn map(
    count: usize,
    out: &mut dyn FnMut(&[u8]) -> Result<(), CodecError>,
    body: impl Fn(&mut dyn FnMut(&[u8]) -> Result<(), CodecError>) -> Result<(), CodecError>,
) -> Result<(), CodecError> {
    let mut size = 8usize;
    body(&mut |bytes| {
        size = size
            .checked_add(bytes.len())
            .ok_or(CodecError::LengthOverflow)?;
        Ok(())
    })?;
    header(8, size, out)?;
    out(&u64::try_from(count)
        .map_err(|_| CodecError::LengthOverflow)?
        .to_be_bytes())?;
    body(out)
}

fn name(operation: &CoreMutation<'_>) -> &'static str {
    match operation {
        CoreMutation::Create { .. } => "workflow.create",
        CoreMutation::Transition { .. } => "workflow.transition",
        CoreMutation::Complete { .. } => "workflow.complete",
        CoreMutation::Fail { .. } => "workflow.fail",
        CoreMutation::Retry { .. } => "workflow.retry",
        CoreMutation::ExtendLease { .. } => "workflow.extend_lease",
        CoreMutation::Cancel { .. } => "workflow.cancel",
    }
}

fn canonical(
    tenant: &[u8],
    caller: &NormalizedMutation<'_>,
    out: &mut dyn FnMut(&[u8]) -> Result<(), CodecError>,
) -> Result<(), CodecError> {
    out(b"WF-INTENT\0V1\0")?;
    map(5, out, |out| {
        scalar(5, b"args", out)?;
        let mut count = 0;
        for key in KEYS {
            count += usize::from(argument(caller, key, &mut |_| Ok(()))?);
        }
        map(count, out, |out| {
            for key in KEYS {
                argument(caller, key, out)?;
            }
            Ok(())
        })?;
        for (key, value) in [
            ("normalization_version", "1"),
            ("operation", name(caller.operation())),
            ("schema_version", "1"),
        ] {
            scalar(5, key.as_bytes(), out)?;
            scalar(5, value.as_bytes(), out)?;
        }
        scalar(5, b"tenant_id", out)?;
        scalar(6, tenant, out)
    })
}

fn bytes(
    key: &str,
    value: Option<&[u8]>,
    out: &mut dyn FnMut(&[u8]) -> Result<(), CodecError>,
) -> Result<bool, CodecError> {
    if let Some(value) = value {
        scalar(5, key.as_bytes(), out)?;
        scalar(6, value, out)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn number(
    key: &str,
    value: Option<u64>,
    out: &mut dyn FnMut(&[u8]) -> Result<(), CodecError>,
) -> Result<bool, CodecError> {
    if let Some(value) = value {
        scalar(5, key.as_bytes(), out)?;
        scalar(3, &value.to_be_bytes(), out)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn argument(
    caller: &NormalizedMutation<'_>,
    key: &str,
    out: &mut dyn FnMut(&[u8]) -> Result<(), CodecError>,
) -> Result<bool, CodecError> {
    if matches!(key, "workflow_id" | "requested_now_ms" | "partition") {
        return common_argument(caller, key, out);
    }
    match (caller.operation(), key) {
        (CoreMutation::Create { workflow_type, .. }, "workflow_type") => {
            bytes(key, Some(workflow_type), out)
        }
        (CoreMutation::Create { state, .. }, "initial_state") => bytes(key, Some(state), out),
        (CoreMutation::Create { priority, .. }, "priority") => {
            number(key, Some(u64::from(*priority)), out)
        }
        (
            CoreMutation::Create { run_at_ms, .. }
            | CoreMutation::Transition { run_at_ms, .. }
            | CoreMutation::Retry { run_at_ms, .. },
            "run_at_ms",
        ) => number(key, *run_at_ms, out),
        (
            CoreMutation::Create { payload, .. }
            | CoreMutation::Transition { payload, .. }
            | CoreMutation::Complete { payload, .. }
            | CoreMutation::Fail { payload, .. }
            | CoreMutation::Retry { payload, .. },
            "payload",
        ) => bytes(key, *payload, out),
        (CoreMutation::Transition { from, .. }, "from_state") => bytes(key, Some(from), out),
        (CoreMutation::Transition { to, .. }, "to_state") => bytes(key, Some(to), out),
        (
            CoreMutation::Transition { fence, .. }
            | CoreMutation::Complete { fence, .. }
            | CoreMutation::Fail { fence, .. }
            | CoreMutation::Retry { fence, .. }
            | CoreMutation::ExtendLease { fence, .. }
            | CoreMutation::Cancel { fence, .. },
            "fencing_token",
        ) => number(key, Some(*fence), out),
        (
            CoreMutation::Transition { lease_token, .. } | CoreMutation::Cancel { lease_token, .. },
            "lease_token",
        ) => bytes(key, *lease_token, out),
        (
            CoreMutation::Complete { lease_token, .. }
            | CoreMutation::Fail { lease_token, .. }
            | CoreMutation::Retry { lease_token, .. }
            | CoreMutation::ExtendLease { lease_token, .. },
            "lease_token",
        ) => bytes(key, Some(lease_token), out),
        (CoreMutation::Complete { value, .. }, "result")
        | (CoreMutation::Fail { value, .. }, "error") => bytes(key, *value, out),
        (CoreMutation::Retry { error, .. }, "error") => bytes(key, *error, out),
        (CoreMutation::ExtendLease { lease_ms, .. }, "lease_ms") => {
            number(key, Some(*lease_ms), out)
        }
        (CoreMutation::Cancel { expected_state, .. }, "expected_state") => {
            bytes(key, *expected_state, out)
        }
        (
            CoreMutation::Cancel {
                expected_version, ..
            },
            "expected_version",
        ) => number(key, *expected_version, out),
        (CoreMutation::Cancel { reason, .. }, "reason") => bytes(key, *reason, out),
        (
            CoreMutation::Retry {
                retry: Some(retry), ..
            },
            "retry",
        ) => {
            scalar(5, key.as_bytes(), out)?;
            retry_map(retry, out)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn retry_map(
    retry: &RetryOverride,
    out: &mut dyn FnMut(&[u8]) -> Result<(), CodecError>,
) -> Result<(), CodecError> {
    let count = usize::from(retry.backoff.is_some())
        + usize::from(retry.exhausted_to.is_some())
        + usize::from(retry.max_retries.is_some());
    map(count, out, |out| {
        if let Some(backoff) = retry.backoff {
            scalar(5, b"backoff", out)?;
            map(4, out, |out| {
                number("base_ms", Some(backoff.base_ms), out)?;
                number("jitter_pct", Some(u64::from(backoff.jitter_pct)), out)?;
                scalar(5, b"kind", out)?;
                scalar(
                    5,
                    match backoff.kind {
                        BackoffKind::None => b"none",
                        BackoffKind::Fixed => b"fixed",
                        BackoffKind::Linear => b"linear",
                        BackoffKind::Exponential => b"exponential",
                    },
                    out,
                )?;
                number("max_ms", Some(backoff.max_ms), out)?;
                Ok(())
            })?;
        }
        bytes("exhausted_to", retry.exhausted_to.as_deref(), out)?;
        number("max_retries", retry.max_retries.map(u64::from), out)?;
        Ok(())
    })
}

fn common_argument(
    caller: &NormalizedMutation<'_>,
    key: &str,
    out: &mut dyn FnMut(&[u8]) -> Result<(), CodecError>,
) -> Result<bool, CodecError> {
    if key == "workflow_id" {
        return bytes(key, Some(caller.id()), out);
    }
    if key == "requested_now_ms" {
        return number(key, caller.requested_now_ms(), out);
    }
    if key == "partition" {
        scalar(5, key.as_bytes(), out)?;
        map(
            if caller.partition().is_some() { 2 } else { 1 },
            out,
            |out| {
                if let Some(partition) = caller.partition() {
                    scalar(5, b"key", out)?;
                    scalar(6, partition, out)?;
                }
                scalar(5, b"mode", out)?;
                scalar(
                    5,
                    if caller.partition().is_some() {
                        b"only"
                    } else {
                        b"auto"
                    },
                    out,
                )
            },
        )?;
        return Ok(true);
    }
    Ok(false)
}
