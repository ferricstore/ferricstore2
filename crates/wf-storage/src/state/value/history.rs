//! Complete history values with borrowed validation before owned field copies.

use wf_domain::history::{HistoryEntry, RetryHistory};
use wf_domain::policy::{Backoff, BackoffKind, RetryPolicy, validate_retry};
use wf_domain::retry::RetryDecision;
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::{Fence, TimestampMillis};

use super::record::{event, event_tag};
use super::{
    StateValueError, add, allocate, canonical_history_position, copy, finish, flag, put_raw, raw,
    raw_size, read_u8, read_u16, read_u64, start,
};

/// Encodes a validated history snapshot, preserving original event ID and clock.
///
/// # Errors
/// Rejects invalid IDs/policies, zero or exceeded caps, sizes or allocation.
pub fn encode_history(
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    history: &HistoryEntry,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    if maximum_bytes == 0 {
        return Err(StateValueError::InvalidValue);
    }
    if canonical_history_position(history.event_id.as_bytes())?.1 != history.version {
        return Err(StateValueError::InvalidValue);
    }
    let tag = event_tag(history.kind)?;
    let size = history_size(scope, partition, id, history)?;
    let mut output = allocate(size, maximum_bytes, *b"FWSH")?;
    put_history_body(&mut output, scope, partition, id, history, tag)?;
    Ok(output)
}

pub(super) fn put_history_body(
    output: &mut Vec<u8>,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    history: &HistoryEntry,
    tag: u8,
) -> Result<(), StateValueError> {
    for field in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        partition.as_bytes(),
        id.as_bytes(),
        history.event_id.as_bytes(),
    ] {
        put_raw(output, field)?;
    }
    output.push(tag);
    for value in [
        history.occurred_at.get(),
        history.version,
        history.fence.get(),
        history.attempts,
    ] {
        output.extend_from_slice(&value.to_be_bytes());
    }
    put_raw(output, &history.state)?;
    put_time(output, history.next_run_at);
    output.extend_from_slice(&history.lease_deadline.get().to_be_bytes());
    output.push(u8::from(history.worker.is_some()));
    if let Some(worker) = &history.worker {
        put_raw(output, worker)?;
    }
    output.push(u8::from(history.retry.is_some()));
    if let Some(retry) = &history.retry {
        put_retry(output, retry)?;
    }
    Ok(())
}

pub(super) fn history_size(
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    history: &HistoryEntry,
) -> Result<usize, StateValueError> {
    // Header, seven RAW lengths, kind, four scalars, lease and three flags.
    let mut size = 78;
    for field in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        partition.as_bytes(),
        id.as_bytes(),
        history.event_id.as_bytes(),
        &history.state,
    ] {
        raw_size(field)?;
        add(&mut size, field.len())?;
    }
    if history.next_run_at.is_some() {
        add(&mut size, 8)?;
    }
    if let Some(worker) = &history.worker {
        add(&mut size, raw_size(worker)?)?;
    }
    if let Some(retry) = &history.retry {
        validate_retry(
            Some(retry.policy.max_retries),
            Some(retry.policy.backoff),
            Some(&retry.policy.exhausted_to),
        )
        .map_err(|_| StateValueError::InvalidValue)?;
        add(&mut size, 22)?;
        add(&mut size, raw_size(&retry.run_state)?)?;
        add(&mut size, raw_size(&retry.policy.exhausted_to)?)?;
        if retry.next_run_at.is_some() {
            add(&mut size, 8)?;
        }
    }
    Ok(size)
}

fn put_time(output: &mut Vec<u8>, value: Option<TimestampMillis>) {
    output.push(u8::from(value.is_some()));
    if let Some(value) = value {
        output.extend_from_slice(&value.get().to_be_bytes());
    }
}

pub(super) fn read_time(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Option<TimestampMillis>, StateValueError> {
    if flag(bytes, offset)? {
        Ok(Some(TimestampMillis::new(read_u64(bytes, offset)?)))
    } else {
        Ok(None)
    }
}

fn put_retry(output: &mut Vec<u8>, retry: &RetryHistory) -> Result<(), StateValueError> {
    output.push(u8::from(retry.decision == RetryDecision::Exhausted));
    put_raw(output, &retry.run_state)?;
    put_time(output, retry.next_run_at);
    output.extend_from_slice(&retry.policy.max_retries.to_be_bytes());
    output.push(match retry.policy.backoff.kind {
        BackoffKind::None => 0,
        BackoffKind::Fixed => 1,
        BackoffKind::Linear => 2,
        BackoffKind::Exponential => 3,
    });
    output.extend_from_slice(&retry.policy.backoff.base_ms.to_be_bytes());
    output.extend_from_slice(&retry.policy.backoff.max_ms.to_be_bytes());
    output.push(retry.policy.backoff.jitter_pct);
    put_raw(output, &retry.policy.exhausted_to)
}

pub(super) fn decision(bytes: &[u8], offset: &mut usize) -> Result<RetryDecision, StateValueError> {
    match read_u8(bytes, offset)? {
        0 => Ok(RetryDecision::Scheduled),
        1 => Ok(RetryDecision::Exhausted),
        _ => Err(StateValueError::UnsupportedTag),
    }
}

pub(super) fn policy_fields<'a>(
    bytes: &'a [u8],
    offset: &mut usize,
) -> Result<(u16, Backoff, &'a [u8]), StateValueError> {
    let maximum = read_u16(bytes, offset)?;
    let kind = match read_u8(bytes, offset)? {
        0 => BackoffKind::None,
        1 => BackoffKind::Fixed,
        2 => BackoffKind::Linear,
        3 => BackoffKind::Exponential,
        _ => return Err(StateValueError::UnsupportedTag),
    };
    let backoff = Backoff {
        kind,
        base_ms: read_u64(bytes, offset)?,
        max_ms: read_u64(bytes, offset)?,
        jitter_pct: read_u8(bytes, offset)?,
    };
    Ok((maximum, backoff, raw(bytes, offset)?))
}

fn validate_history(
    bytes: &[u8],
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    maximum: usize,
) -> Result<(), StateValueError> {
    let mut offset = start(bytes, *b"FWSH", maximum)?;
    let namespace = [
        raw(bytes, &mut offset)?,
        raw(bytes, &mut offset)?,
        raw(bytes, &mut offset)?,
        raw(bytes, &mut offset)?,
        raw(bytes, &mut offset)?,
    ];
    let event_id = raw(bytes, &mut offset)?;
    event(read_u8(bytes, &mut offset)?)?;
    read_u64(bytes, &mut offset)?;
    let version = read_u64(bytes, &mut offset)?;
    read_u64(bytes, &mut offset)?;
    read_u64(bytes, &mut offset)?;
    raw(bytes, &mut offset)?;
    read_time(bytes, &mut offset)?;
    read_u64(bytes, &mut offset)?;
    if flag(bytes, &mut offset)? {
        raw(bytes, &mut offset)?;
    }
    let policy = if flag(bytes, &mut offset)? {
        decision(bytes, &mut offset)?;
        raw(bytes, &mut offset)?;
        read_time(bytes, &mut offset)?;
        Some(policy_fields(bytes, &mut offset)?)
    } else {
        None
    };
    finish(bytes, offset)?;
    if namespace
        != [
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            partition.as_bytes(),
            id.as_bytes(),
        ]
        || canonical_history_position(event_id)?.1 != version
    {
        return Err(StateValueError::InvalidValue);
    }
    if let Some((maximum, backoff, target)) = policy {
        validate_retry(Some(maximum), Some(backoff), Some(target))
            .map_err(|_| StateValueError::InvalidValue)?;
    }
    Ok(())
}

/// Decodes complete history after checking framing, namespace, IDs and policy.
///
/// The returned owner borrows nothing from the input or expected namespace.
///
/// # Errors
/// Rejects malformed, unsupported, foreign, invalid, oversized or unallocatable data.
pub fn decode_history(
    bytes: &[u8],
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    maximum_bytes: usize,
) -> Result<HistoryEntry, StateValueError> {
    validate_history(bytes, scope, partition, id, maximum_bytes)?;
    let mut offset = 6;
    for _ in 0..5 {
        raw(bytes, &mut offset)?;
    }
    let event_id = String::from_utf8(copy(raw(bytes, &mut offset)?)?)
        .map_err(|_| StateValueError::InvalidValue)?;
    let kind = event(read_u8(bytes, &mut offset)?)?;
    let occurred_at = TimestampMillis::new(read_u64(bytes, &mut offset)?);
    let version = read_u64(bytes, &mut offset)?;
    let fence = Fence::new(read_u64(bytes, &mut offset)?);
    let attempts = read_u64(bytes, &mut offset)?;
    let state = copy(raw(bytes, &mut offset)?)?;
    let next_run_at = read_time(bytes, &mut offset)?;
    let lease_deadline = TimestampMillis::new(read_u64(bytes, &mut offset)?);
    let worker = if flag(bytes, &mut offset)? {
        Some(copy(raw(bytes, &mut offset)?)?)
    } else {
        None
    };
    let retry = if flag(bytes, &mut offset)? {
        Some(owned_retry(bytes, &mut offset)?)
    } else {
        None
    };
    finish(bytes, offset)?;
    Ok(HistoryEntry {
        event_id,
        kind,
        occurred_at,
        version,
        fence,
        attempts,
        state,
        next_run_at,
        lease_deadline,
        worker,
        retry,
    })
}

fn owned_retry(bytes: &[u8], offset: &mut usize) -> Result<RetryHistory, StateValueError> {
    let decision = decision(bytes, offset)?;
    let run_state = copy(raw(bytes, offset)?)?;
    let next_run_at = read_time(bytes, offset)?;
    let (max_retries, backoff, target) = policy_fields(bytes, offset)?;
    Ok(RetryHistory {
        decision,
        run_state,
        next_run_at,
        policy: RetryPolicy {
            max_retries,
            backoff,
            exhausted_to: copy(target)?,
        },
    })
}
