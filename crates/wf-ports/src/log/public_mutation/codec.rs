//! Direct borrowed emission and post-admission single-item materialization.

use wf_domain::policy::{BackoffKind, RetryOverride};
use wf_domain::workflow::caller::{CoreMutation, NormalizedMutation};
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_types::identity::RequestId;
use wf_types::version::CommittedTime;

use super::super::{
    CodecError, CodecLimits, PreparedBytes, RecordBatchApplyError, digest, keys, writer::Writer,
};
use crate::consensus::PreparedRequest;

/// Measures the actual public frame/canonical schema before hashing or effects.
/// # Errors
/// Rejects unsupported caller/profile, field/key/canonical or frame bounds.
pub fn measure_public_mutation(
    scope: &RequestScope,
    request: &RequestId,
    caller: &NormalizedMutation<'_>,
    limits: CodecLimits,
) -> Result<usize, RecordBatchApplyError> {
    super::intent::measure(scope.tenant().as_bytes(), caller, canonical_cap(limits)?)?;
    let mut writer = Writer::measure(limits.admission.decision_bytes());
    write(
        scope,
        request,
        caller,
        None,
        ClientIntent::new(1, [0; 32]).map_err(|_| CodecError::UnsupportedVersion)?,
        &mut writer,
        limits,
    )?;
    Ok(writer.len())
}

/// Encodes directly from borrowed normalized caller bytes after measured admission.
/// # Errors
/// Rejects schema/caps or prospective workspace before hashing/frame reservation.
pub fn encode_public_mutation(
    scope: &RequestScope,
    request: &RequestId,
    caller: &NormalizedMutation<'_>,
    stamp: CommittedTime,
    limits: CodecLimits,
    workspace_limit: usize,
) -> Result<(PreparedBytes, ClientIntent), RecordBatchApplyError> {
    let size = measure_public_mutation(scope, request, caller, limits)?;
    super::super::record_batch::admit(
        size.checked_add(2048).ok_or(CodecError::LengthOverflow)?,
        workspace_limit,
    )?;
    let intent = super::intent::prepare(scope.tenant().as_bytes(), caller, canonical_cap(limits)?)?;
    let mut writer = Writer::allocate(size, limits.admission.decision_bytes())?;
    write(
        scope,
        request,
        caller,
        Some(stamp),
        intent,
        &mut writer,
        limits,
    )?;
    let bytes = writer.finish()?;
    Ok((
        PreparedBytes {
            digest: digest(&bytes),
            bytes,
        },
        intent,
    ))
}

fn canonical_cap(limits: CodecLimits) -> Result<usize, CodecError> {
    limits
        .admission
        .decision_bytes()
        .checked_add(8192)
        .ok_or(CodecError::LengthOverflow)
}

pub(in crate::log) fn auto_partition(id: &[u8], bytes: &mut [u8; 17]) -> usize {
    let mut crc = u32::MAX;
    for byte in id {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320_u32.wrapping_mul(crc & 1));
        }
    }
    let bucket = (!crc) % 256;
    bytes[..14].copy_from_slice(b"__flow_auto__:");
    let digits = if bucket >= 100 {
        3
    } else if bucket >= 10 {
        2
    } else {
        1
    };
    let mut remaining = bucket;
    for at in (14..14 + digits).rev() {
        bytes[at] = b'0' + u8::try_from(remaining % 10).unwrap_or(0);
        remaining /= 10;
    }
    14 + digits
}

fn write(
    scope: &RequestScope,
    request: &RequestId,
    caller: &NormalizedMutation<'_>,
    stamp: Option<CommittedTime>,
    intent: ClientIntent,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    if intent.version() != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    let mut auto = [0; 17];
    let auto_len = if caller.partition().is_none() {
        auto_partition(caller.id(), &mut auto)
    } else {
        0
    };
    let partition = caller.partition().unwrap_or(&auto[..auto_len]);
    let scope_bytes = [
        scope.tenant().as_bytes(),
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
    ];
    let scope_length = keys::scope_length(scope_bytes)?;
    let mut derived = scope_length;
    keys::charge(
        &mut derived,
        keys::state_length(scope_length, partition.len(), caller.id().len())?,
        limits.admission.decision_bytes(),
    )?;
    writer.write(b"FWCP")?;
    writer.u16(1)?;
    writer.byte(4)?;
    for bytes in scope_bytes {
        writer.raw(bytes, 8192)?;
    }
    writer.raw(request.as_bytes(), 256)?;
    writer.u16(1)?;
    writer.write(intent.digest())?;
    writer.u64(stamp.map_or(0, |time| time.hlc().physical().get()))?;
    writer.u16(stamp.map_or(0, |time| time.hlc().logical()))?;
    writer.u64(stamp.map_or(0, |time| time.wall().get()))?;
    writer.u16(1)?;
    let retry = matches!(
        caller.operation(),
        CoreMutation::Retry { retry: Some(_), .. }
    );
    writer.byte(
        u8::from(caller.partition().is_none())
            | (u8::from(caller.requested_now_ms().is_some()) << 1)
            | (u8::from(retry) << 2),
    )?;
    if let Some(now) = caller.requested_now_ms() {
        writer.u64(now)?;
    }
    writer.raw(partition, 65535)?;
    writer.raw(caller.id(), 65535)?;
    writer.byte(
        if let CoreMutation::Create { priority, .. } = caller.operation() {
            *priority
        } else {
            0
        },
    )?;
    writer.u64(
        caller
            .requested_now_ms()
            .unwrap_or_else(|| stamp.map_or(0, |time| time.hlc().physical().get())),
    )?;
    operation(caller.operation(), writer, limits.admission.inline_bytes())
}

fn bytes(value: Option<&[u8]>, writer: &mut Writer<'_>, maximum: usize) -> Result<(), CodecError> {
    writer.optional(value, |writer, value| writer.raw(value, maximum))
}
fn time(value: Option<u64>, writer: &mut Writer<'_>) -> Result<(), CodecError> {
    writer.optional(value.as_ref(), |writer, value| writer.u64(*value))
}
fn guard(
    token: &[u8],
    fence: u64,
    writer: &mut Writer<'_>,
    maximum: usize,
) -> Result<(), CodecError> {
    writer.raw(token, maximum)?;
    writer.u64(fence)
}
fn retry(
    value: Option<&RetryOverride>,
    writer: &mut Writer<'_>,
    maximum: usize,
) -> Result<(), CodecError> {
    let empty = RetryOverride::default();
    let value = value.unwrap_or(&empty);
    writer.optional(value.max_retries.as_ref(), |writer, value| {
        writer.u16(*value)
    })?;
    writer.optional(value.backoff.as_ref(), |writer, value| {
        writer.byte(match value.kind {
            BackoffKind::None => 0,
            BackoffKind::Fixed => 1,
            BackoffKind::Linear => 2,
            BackoffKind::Exponential => 3,
        })?;
        writer.u64(value.base_ms)?;
        writer.u64(value.max_ms)?;
        writer.byte(value.jitter_pct)
    })?;
    bytes(value.exhausted_to.as_deref(), writer, maximum)
}

fn operation(
    operation: &CoreMutation<'_>,
    writer: &mut Writer<'_>,
    maximum: usize,
) -> Result<(), CodecError> {
    match operation {
        CoreMutation::Create {
            workflow_type,
            state,
            run_at_ms,
            payload,
            ..
        } => {
            writer.byte(1)?;
            writer.raw(workflow_type, maximum)?;
            writer.raw(state, maximum)?;
            time(*run_at_ms, writer)?;
            bytes(*payload, writer, maximum)
        }
        CoreMutation::Transition {
            from,
            to,
            fence,
            lease_token,
            run_at_ms,
            payload,
        } => {
            writer.byte(4)?;
            writer.raw(from, maximum)?;
            writer.raw(to, maximum)?;
            writer.u64(*fence)?;
            bytes(*lease_token, writer, maximum)?;
            time(*run_at_ms, writer)?;
            bytes(*payload, writer, maximum)
        }
        CoreMutation::Complete {
            lease_token,
            fence,
            payload,
            value,
        }
        | CoreMutation::Fail {
            lease_token,
            fence,
            payload,
            value,
        } => {
            writer.byte(if matches!(operation, CoreMutation::Complete { .. }) {
                5
            } else {
                6
            })?;
            guard(lease_token, *fence, writer, maximum)?;
            bytes(*payload, writer, maximum)?;
            bytes(*value, writer, maximum)
        }
        CoreMutation::Retry {
            lease_token,
            fence,
            run_at_ms,
            retry: policy,
            payload,
            error,
        } => {
            writer.byte(8)?;
            guard(lease_token, *fence, writer, maximum)?;
            time(*run_at_ms, writer)?;
            retry(*policy, writer, maximum)?;
            bytes(*payload, writer, maximum)?;
            bytes(*error, writer, maximum)
        }
        CoreMutation::ExtendLease {
            lease_token,
            fence,
            lease_ms,
        } => {
            writer.byte(9)?;
            guard(lease_token, *fence, writer, maximum)?;
            writer.u64(*lease_ms)
        }
        CoreMutation::Cancel {
            fence,
            lease_token,
            expected_state,
            expected_version,
            reason,
        } => {
            writer.byte(10)?;
            writer.u64(*fence)?;
            bytes(*lease_token, writer, maximum)?;
            bytes(*expected_state, writer, maximum)?;
            time(*expected_version, writer)?;
            bytes(*reason, writer, maximum)
        }
    }
}

pub(in crate::log) fn write_owned(
    request: &PreparedRequest,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    let crate::consensus::PreparedOperation::PublicMutation { item, evidence } = &request.operation
    else {
        return Err(CodecError::UnsupportedTag);
    };
    super::with_caller(item, evidence, limits, |caller| {
        write(
            &request.scope,
            &request.request,
            caller,
            Some(request.stamp),
            request.intent,
            writer,
            limits,
        )
    })
}

pub(in crate::log) fn decoded_intent(
    request: &PreparedRequest,
    encoded: &[u8],
    limits: CodecLimits,
) -> Result<ClientIntent, CodecError> {
    let crate::consensus::PreparedOperation::PublicMutation { item, evidence } = &request.operation
    else {
        return Err(CodecError::UnsupportedTag);
    };
    super::with_caller(item, evidence, limits, |caller| {
        let mut verify = Writer::verify(encoded, limits.admission.decision_bytes())?;
        write(
            &request.scope,
            &request.request,
            caller,
            Some(request.stamp),
            request.intent,
            &mut verify,
            limits,
        )?;
        verify.finish_verification()?;
        super::intent::prepare(
            request.scope.tenant().as_bytes(),
            caller,
            canonical_cap(limits)?,
        )
    })
}
