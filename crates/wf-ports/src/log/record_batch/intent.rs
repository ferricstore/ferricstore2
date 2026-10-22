//! Closed caller field streams, independent of generated preparation values.

use sha2::{Digest, Sha256};
use wf_domain::lease::LeaseGuard;
use wf_domain::policy::{BackoffKind, RetryOverride};
use wf_domain::retry::RetryTime;
use wf_domain::workflow::Operation;
use wf_domain::workflow::request::ClientIntent;
use wf_types::version::TimestampMillis;

use crate::consensus::{PreparedOperation, PreparedRequest};

use super::super::CodecError;

pub(super) fn derive(
    request: &PreparedRequest,
    version: u16,
) -> Result<(ClientIntent, usize), CodecError> {
    let PreparedOperation::Batch(items) = &request.operation else {
        return Err(CodecError::UnsupportedTag);
    };
    let mut hash = Sha256::new();
    let mut bytes = 0;
    hash.update(match version {
        5 => b"WF-RECORD-BATCH\0V5\0",
        6 => b"WF-RECORD-BATCH\0V6\0",
        _ => return Err(CodecError::UnsupportedVersion),
    });
    raw(&mut hash, &mut bytes, request.scope.tenant().as_bytes())?;
    hash.update(
        u32::try_from(items.len())
            .map_err(|_| CodecError::LengthOverflow)?
            .to_be_bytes(),
    );
    for item in items {
        raw(&mut hash, &mut bytes, item.partition.as_bytes())?;
        raw(&mut hash, &mut bytes, item.command.id.as_bytes())?;
        operation(
            &mut hash,
            &mut bytes,
            &item.command.operation,
            item.create_priority,
        )?;
    }
    for field in [
        request.scope.group().as_bytes(),
        request.scope.recovery().as_bytes(),
        request.request.as_bytes(),
    ] {
        bytes = bytes
            .checked_add(field.len())
            .ok_or(CodecError::LengthOverflow)?;
    }
    let digest = hash.finalize().into();
    let intent = if version == 6 {
        ClientIntent::independent_record_batch(digest)
    } else {
        ClientIntent::record_batch(digest)
    };
    Ok((intent, bytes))
}

fn raw(hash: &mut Sha256, bytes: &mut usize, value: &[u8]) -> Result<(), CodecError> {
    let length = u32::try_from(value.len()).map_err(|_| CodecError::LengthOverflow)?;
    *bytes = bytes
        .checked_add(value.len())
        .ok_or(CodecError::LengthOverflow)?;
    hash.update(length.to_be_bytes());
    hash.update(value);
    Ok(())
}

fn optional_raw(
    hash: &mut Sha256,
    bytes: &mut usize,
    value: Option<&[u8]>,
) -> Result<(), CodecError> {
    hash.update([u8::from(value.is_some())]);
    if let Some(value) = value {
        raw(hash, bytes, value)?;
    }
    Ok(())
}

fn time(hash: &mut Sha256, value: Option<TimestampMillis>) {
    hash.update([u8::from(value.is_some())]);
    if let Some(value) = value {
        hash.update(value.get().to_be_bytes());
    }
}

fn guard(hash: &mut Sha256, bytes: &mut usize, value: &LeaseGuard) -> Result<(), CodecError> {
    raw(hash, bytes, value.token.as_bytes())?;
    hash.update(value.fence.get().to_be_bytes());
    Ok(())
}

fn retry_override(
    hash: &mut Sha256,
    bytes: &mut usize,
    retry: &RetryOverride,
) -> Result<(), CodecError> {
    hash.update([u8::from(retry.max_retries.is_some())]);
    if let Some(value) = retry.max_retries {
        hash.update(value.to_be_bytes());
    }
    hash.update([u8::from(retry.backoff.is_some())]);
    if let Some(value) = retry.backoff {
        hash.update([match value.kind {
            BackoffKind::None => 0,
            BackoffKind::Fixed => 1,
            BackoffKind::Linear => 2,
            BackoffKind::Exponential => 3,
        }]);
        hash.update(value.base_ms.to_be_bytes());
        hash.update(value.max_ms.to_be_bytes());
        hash.update([value.jitter_pct]);
    }
    optional_raw(hash, bytes, retry.exhausted_to.as_deref())
}

fn operation(
    hash: &mut Sha256,
    bytes: &mut usize,
    value: &Operation,
    priority: u8,
) -> Result<(), CodecError> {
    hash.update([match value {
        Operation::Create(_) => 1,
        Operation::Transition(_) => 4,
        Operation::Complete(_) => 5,
        Operation::Fail(_) => 6,
        Operation::Retry(_) => 7,
        Operation::RetryWithPolicy(_) => 8,
        Operation::ExtendLease(_) => 9,
        Operation::Cancel(_) => 10,
        Operation::Claim(_) | Operation::Reclaim(_) | Operation::StepContinue(_) => {
            return Err(CodecError::UnsupportedTag);
        }
    }]);
    match value {
        Operation::Create(value) => {
            hash.update([priority]);
            raw(hash, bytes, &value.workflow_type)?;
            raw(hash, bytes, &value.state)?;
            time(hash, value.run_at);
            optional_raw(hash, bytes, value.payload.as_deref())
        }
        Operation::Transition(value) => {
            raw(hash, bytes, &value.from)?;
            raw(hash, bytes, &value.to)?;
            hash.update(value.fence.get().to_be_bytes());
            optional_raw(
                hash,
                bytes,
                value
                    .lease_token
                    .as_ref()
                    .map(wf_domain::lease::LeaseToken::as_bytes),
            )?;
            time(hash, value.run_at);
            optional_raw(hash, bytes, value.payload.as_deref())
        }
        Operation::Complete(value) | Operation::Fail(value) => {
            guard(hash, bytes, &value.guard)?;
            optional_raw(hash, bytes, value.payload.as_deref())?;
            optional_raw(hash, bytes, value.value.as_deref())
        }
        Operation::Retry(value) => {
            guard(hash, bytes, &value.guard)?;
            match value.run_at {
                RetryTime::Explicit(value) => {
                    hash.update([0]);
                    hash.update(value.get().to_be_bytes());
                }
                RetryTime::PolicyResolved(_) => hash.update([1]),
            }
            optional_raw(hash, bytes, value.payload.as_deref())?;
            optional_raw(hash, bytes, value.error.as_deref())
        }
        Operation::RetryWithPolicy(value) => {
            guard(hash, bytes, &value.guard)?;
            time(hash, value.run_at);
            retry_override(hash, bytes, &value.policy_override)?;
            optional_raw(hash, bytes, value.payload.as_deref())?;
            optional_raw(hash, bytes, value.error.as_deref())
        }
        Operation::ExtendLease(value) => {
            guard(hash, bytes, &value.guard)?;
            hash.update(value.lease_ms.to_be_bytes());
            Ok(())
        }
        Operation::Cancel(value) => {
            hash.update(value.fence.get().to_be_bytes());
            optional_raw(
                hash,
                bytes,
                value
                    .lease_token
                    .as_ref()
                    .map(wf_domain::lease::LeaseToken::as_bytes),
            )?;
            optional_raw(hash, bytes, value.expected_state.as_deref())?;
            hash.update([u8::from(value.expected_version.is_some())]);
            if let Some(value) = value.expected_version {
                hash.update(value.to_be_bytes());
            }
            optional_raw(hash, bytes, value.error.as_deref())
        }
        Operation::Claim(_) | Operation::Reclaim(_) | Operation::StepContinue(_) => {
            Err(CodecError::UnsupportedTag)
        }
    }
}
