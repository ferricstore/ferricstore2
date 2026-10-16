//! A-N1: owned materialization only after the complete borrowed structural pass.

use wf_domain::claim::Claim;
use wf_domain::lease::{ExtendLease, LeaseGuard, LeaseToken};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retry::{JitterSample, PolicyRetry, Retry, RetryTime};
use wf_domain::workflow::request::RequestScope;
use wf_domain::workflow::{Cancel, CommittedCommand, Create, Operation, Terminal, Transition};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{Fence, TimestampMillis};

use crate::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};

use super::{
    CodecError, CodecLimits, DecodedRequest, claim, common, digest, keys, policy,
    reader::{self, Reader},
    record_batch,
};

pub(super) fn read(encoded: &[u8], limits: CodecLimits) -> Result<DecodedRequest<'_>, CodecError> {
    let mut reader = Reader::new(encoded, limits);
    let header = common::header(&mut reader)?;
    let isolation = keys::scope(header.scope())?;
    let scope = RequestScope::new(
        TenantId::new(reader::copy(header.tenant)?).map_err(|_| CodecError::Malformed)?,
        GroupId::new(reader::copy(header.group)?).map_err(|_| CodecError::Malformed)?,
        RecoveryId::new(reader::copy(header.recovery)?).map_err(|_| CodecError::Malformed)?,
    );
    let request =
        RequestId::new(reader::copy(header.request)?).map_err(|_| CodecError::Malformed)?;
    let count = if header.operation_tag == 1 {
        common::count(&mut reader)?
    } else {
        0
    };
    let mut items = Vec::new();
    let mut state_keys = Vec::new();
    items
        .try_reserve_exact(count)
        .map_err(|_| CodecError::Allocation)?;
    state_keys
        .try_reserve_exact(count)
        .map_err(|_| CodecError::Allocation)?;
    for _ in 0..count {
        let item = common::item(&mut reader)?;
        state_keys.push(keys::state(&isolation, item.partition, item.id)?);
        items.push(PreparedBatchItem {
            partition: PartitionId::new(reader::copy(item.partition)?)
                .map_err(|_| CodecError::Malformed)?,
            command: CommittedCommand {
                id: RecordId::new(reader::copy(item.id)?).map_err(|_| CodecError::Malformed)?,
                now: item.now,
                operation: operation(&mut reader, item.tag, false)?,
            },
            create_priority: item.priority,
        });
    }
    let mut claim_filters = None;
    let operation = if header.operation_tag == 2 {
        policy::read(&mut reader)?
    } else if header.operation_tag == 3 {
        let (phase, filters) = claim::read(&mut reader)?;
        claim_filters = Some(filters);
        PreparedOperation::ClaimPhase(phase)
    } else if header.operation_tag == 4 {
        super::public_mutation::read(&mut reader, &isolation, &mut state_keys)?
    } else {
        PreparedOperation::Batch(items)
    };
    reader.finish()?;
    let request = PreparedRequest {
        scope,
        request,
        intent: header.intent,
        stamp: header.stamp,
        operation,
    };
    let record_batch_intent =
        if header.operation_tag == 1 && matches!(header.intent.version(), 5 | 6) {
            record_batch::decoded_intent(&request)
        } else if header.operation_tag == 4 {
            super::public_mutation::decoded_intent(&request, encoded, limits)
        } else {
            Err(CodecError::UnsupportedVersion)
        };
    Ok(DecodedRequest {
        request,
        encoded,
        digest: digest(encoded),
        isolation,
        state_keys,
        claim_filters,
        limits,
        record_batch_intent,
    })
}

fn token(reader: &mut Reader<'_>) -> Result<LeaseToken, CodecError> {
    let raw = reader.nonempty(reader.limits.admission.inline_bytes())?;
    LeaseToken::new(reader::copy(raw)?).map_err(|_| CodecError::Malformed)
}
fn guard(reader: &mut Reader<'_>) -> Result<LeaseGuard, CodecError> {
    Ok(LeaseGuard {
        token: token(reader)?,
        fence: Fence::new(reader.u64()?),
    })
}
fn time(reader: &mut Reader<'_>) -> Result<Option<TimestampMillis>, CodecError> {
    reader.optional(|reader| reader.u64().map(TimestampMillis::new))
}
fn bytes(reader: &mut Reader<'_>) -> Result<Option<Vec<u8>>, CodecError> {
    reader.optional(Reader::owned)
}
fn claim(reader: &mut Reader<'_>) -> Result<Claim, CodecError> {
    Ok(Claim {
        expected_state: reader.owned()?,
        worker: reader.owned()?,
        token: token(reader)?,
        lease_ms: reader.u64()?,
    })
}
fn terminal(reader: &mut Reader<'_>) -> Result<Terminal, CodecError> {
    Ok(Terminal {
        guard: guard(reader)?,
        payload: bytes(reader)?,
        value: bytes(reader)?,
    })
}
fn retry_override(reader: &mut Reader<'_>) -> Result<RetryOverride, CodecError> {
    let max_retries = reader.optional(Reader::u16)?;
    let backoff = reader.optional(|reader| {
        Ok(Backoff {
            kind: match reader.byte()? {
                0 => BackoffKind::None,
                1 => BackoffKind::Fixed,
                2 => BackoffKind::Linear,
                3 => BackoffKind::Exponential,
                _ => return Err(CodecError::UnsupportedTag),
            },
            base_ms: reader.u64()?,
            max_ms: reader.u64()?,
            jitter_pct: reader.byte()?,
        })
    })?;
    Ok(RetryOverride {
        max_retries,
        backoff,
        exhausted_to: bytes(reader)?,
    })
}
fn retry(reader: &mut Reader<'_>) -> Result<Retry, CodecError> {
    let guard = guard(reader)?;
    let tag = reader.byte()?;
    let run_at = TimestampMillis::new(reader.u64()?);
    Ok(Retry {
        guard,
        run_at: match tag {
            0 => RetryTime::Explicit(run_at),
            1 => RetryTime::PolicyResolved(run_at),
            _ => return Err(CodecError::UnsupportedTag),
        },
        payload: bytes(reader)?,
        error: bytes(reader)?,
    })
}
fn policy_retry(reader: &mut Reader<'_>, public: bool) -> Result<PolicyRetry, CodecError> {
    Ok(PolicyRetry {
        guard: guard(reader)?,
        run_at: time(reader)?,
        policy_override: retry_override(reader)?,
        jitter: JitterSample::new(if public { 0 } else { reader.u32()? }),
        payload: bytes(reader)?,
        error: bytes(reader)?,
    })
}
fn transition(reader: &mut Reader<'_>) -> Result<Transition, CodecError> {
    Ok(Transition {
        from: reader.owned()?,
        to: reader.owned()?,
        fence: Fence::new(reader.u64()?),
        lease_token: reader.optional(token)?,
        run_at: time(reader)?,
        payload: bytes(reader)?,
    })
}
fn cancel(reader: &mut Reader<'_>) -> Result<Cancel, CodecError> {
    Ok(Cancel {
        fence: Fence::new(reader.u64()?),
        lease_token: reader.optional(token)?,
        expected_state: bytes(reader)?,
        expected_version: reader.optional(Reader::u64)?,
        error: bytes(reader)?,
    })
}
pub(super) fn operation(
    reader: &mut Reader<'_>,
    tag: u8,
    public: bool,
) -> Result<Operation, CodecError> {
    Ok(match tag {
        1 => Operation::Create(Create {
            workflow_type: reader.owned()?,
            state: reader.owned()?,
            run_at: time(reader)?,
            payload: bytes(reader)?,
        }),
        2 => Operation::Claim(claim(reader)?),
        3 => Operation::Reclaim(claim(reader)?),
        4 => Operation::Transition(transition(reader)?),
        5 => Operation::Complete(terminal(reader)?),
        6 => Operation::Fail(terminal(reader)?),
        7 => Operation::Retry(retry(reader)?),
        8 => Operation::RetryWithPolicy(policy_retry(reader, public)?),
        9 => Operation::ExtendLease(ExtendLease {
            guard: guard(reader)?,
            lease_ms: reader.u64()?,
        }),
        10 => Operation::Cancel(cancel(reader)?),
        _ => return Err(CodecError::UnsupportedTag),
    })
}
