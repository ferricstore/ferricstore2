//! Public caller V1 mathematics and one verified individual committed profile.

use super::{
    CodecError, common, decode, keys,
    reader::{self, Reader},
    scan,
};
use crate::consensus::{PreparedBatchItem, PreparedOperation};
use wf_domain::workflow::caller::{
    MutationFieldValue as V, NormalizedMutation, PublicMutationEvidence,
};
use wf_domain::workflow::request::ClientIntent;
use wf_domain::workflow::{CommittedCommand, Operation};
use wf_types::identity::TenantId;
use wf_types::identity::{PartitionId, RecordId};

use super::RecordBatchApplyError;

mod apply;
mod codec;
mod intent;
pub(in crate::log) mod response;

pub use apply::{
    decide_fresh_public_mutation_touched, inspect_public_mutation_point, prepare_public_mutation,
};
pub use codec::{encode_public_mutation, measure_public_mutation};
pub use response::{public_mutation_response_matches, public_mutation_retained_status};

/// Streams the exact seven-kind public V1 registry without a canonical buffer.
/// Mathematical caller data grants no authorization, submission or origin proof.
/// # Errors
/// Rejects checked canonical size or the selected caller schema.
pub fn prepare_public_mutation_intent(
    tenant: &TenantId,
    caller: &NormalizedMutation<'_>,
    maximum_bytes: usize,
) -> Result<ClientIntent, RecordBatchApplyError> {
    Ok(intent::prepare(tenant.as_bytes(), caller, maximum_bytes)?)
}

pub(in crate::log) use codec::auto_partition;
pub(super) use codec::{decoded_intent, write_owned};
pub(in crate::log) use intent::canonical;

// A bounded borrowed adapter, used only inside this frame's verifier/writer.
// Re-normalization detects default/presence contradictions before byte binding.
pub(in crate::log) fn with_caller<T>(
    item: &PreparedBatchItem,
    evidence: &PublicMutationEvidence,
    limits: super::CodecLimits,
    inspect: impl FnOnce(&NormalizedMutation<'_>) -> Result<T, super::CodecError>,
) -> Result<T, super::CodecError> {
    use wf_domain::workflow::caller::{
        MutationFieldValue as V, MutationInput, normalize_core_mutation,
    };
    let mut fields = [("", V::Nil); 8];
    let mut used = 0;
    let mut put = |key, value| {
        fields[used] = (key, value);
        used += 1;
    };
    let operation = effect_fields(item, evidence, &mut put)?;
    if let Some(now) = evidence.requested_now_ms {
        put("now_ms", V::Integer(i128::from(now)));
    }
    if evidence.retry_override_present && operation != "retry" {
        return Err(super::CodecError::Malformed);
    }
    let caller = normalize_core_mutation(
        MutationInput {
            operation,
            id: item.command.id.as_bytes(),
            fields: &fields[..used],
        },
        &[],
        if evidence.auto_partition {
            None
        } else {
            Some(item.partition.as_bytes())
        },
        limits.admission(),
        usize::MAX,
    )
    .map_err(|_| super::CodecError::Malformed)?;
    inspect(&caller)
}

fn initial_fields<'a>(
    item: &'a PreparedBatchItem,
    put: &mut impl FnMut(&'static str, V<'a>),
) -> Result<&'static str, CodecError> {
    Ok(match &item.command.operation {
        Operation::Create(value) => {
            put("type", V::Bytes(&value.workflow_type));
            put("state", V::Bytes(&value.state));
            put("priority", V::Integer(i128::from(item.create_priority)));
            if let Some(time) = value.run_at {
                put("run_at_ms", V::Integer(i128::from(time.get())));
            }
            if let Some(payload) = &value.payload {
                put("payload", V::Bytes(payload));
            }
            "create"
        }
        Operation::Transition(value) => {
            put("from", V::Bytes(&value.from));
            put("to", V::Bytes(&value.to));
            put("fencing_token", V::Integer(i128::from(value.fence.get())));
            if let Some(token) = &value.lease_token {
                put("lease_token", V::Bytes(token.as_bytes()));
            }
            if let Some(time) = value.run_at {
                put("run_at_ms", V::Integer(i128::from(time.get())));
            }
            if let Some(payload) = &value.payload {
                put("payload", V::Bytes(payload));
            }
            "transition"
        }
        _ => return Err(CodecError::UnsupportedTag),
    })
}

fn effect_fields<'a>(
    item: &'a PreparedBatchItem,
    evidence: &PublicMutationEvidence,
    put: &mut impl FnMut(&'static str, V<'a>),
) -> Result<&'static str, CodecError> {
    Ok(match &item.command.operation {
        Operation::Create(_) | Operation::Transition(_) => return initial_fields(item, put),
        Operation::Complete(value) | Operation::Fail(value) => {
            put("lease_token", V::Bytes(value.guard.token.as_bytes()));
            put(
                "fencing_token",
                V::Integer(i128::from(value.guard.fence.get())),
            );
            if let Some(payload) = &value.payload {
                put("payload", V::Bytes(payload));
            }
            let complete = matches!(item.command.operation, Operation::Complete(_));
            if let Some(value) = &value.value {
                put(if complete { "result" } else { "error" }, V::Bytes(value));
            }
            if complete { "complete" } else { "fail" }
        }
        Operation::RetryWithPolicy(value) => {
            put("lease_token", V::Bytes(value.guard.token.as_bytes()));
            put(
                "fencing_token",
                V::Integer(i128::from(value.guard.fence.get())),
            );
            if let Some(time) = value.run_at {
                put("run_at_ms", V::Integer(i128::from(time.get())));
            }
            if evidence.retry_override_present {
                put("retry", V::Retry(&value.policy_override));
            } else if value.policy_override != wf_domain::policy::RetryOverride::default() {
                return Err(super::CodecError::Malformed);
            }
            if let Some(payload) = &value.payload {
                put("payload", V::Bytes(payload));
            }
            if let Some(error) = &value.error {
                put("error", V::Bytes(error));
            }
            "retry"
        }
        Operation::ExtendLease(value) => {
            put("lease_token", V::Bytes(value.guard.token.as_bytes()));
            put(
                "fencing_token",
                V::Integer(i128::from(value.guard.fence.get())),
            );
            put("lease_ms", V::Integer(i128::from(value.lease_ms)));
            "extend_lease"
        }
        Operation::Cancel(value) => {
            put("fencing_token", V::Integer(i128::from(value.fence.get())));
            if let Some(token) = &value.lease_token {
                put("lease_token", V::Bytes(token.as_bytes()));
            }
            if let Some(state) = &value.expected_state {
                put("expect_state", V::Bytes(state));
            }
            if let Some(version) = value.expected_version {
                put("expected_version", V::Integer(i128::from(version)));
            }
            if let Some(reason) = &value.error {
                put("reason", V::Bytes(reason));
            }
            "cancel"
        }
        _ => return Err(super::CodecError::UnsupportedTag),
    })
}

fn evidence(reader: &mut Reader<'_>) -> Result<PublicMutationEvidence, CodecError> {
    if reader.u16()? != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    let flags = reader.byte()?;
    if flags & !7 != 0 {
        return Err(CodecError::UnsupportedTag);
    }
    Ok(PublicMutationEvidence {
        requested_now_ms: if flags & 2 != 0 {
            Some(reader.u64()?)
        } else {
            None
        },
        auto_partition: flags & 1 != 0,
        retry_override_present: flags & 4 != 0,
    })
}

fn check_tag(tag: u8, priority: u8, evidence: PublicMutationEvidence) -> Result<(), CodecError> {
    if !matches!(tag, 1 | 4 | 5 | 6 | 8 | 9 | 10)
        || (tag != 1 && priority != 0)
        || (tag != 8 && evidence.retry_override_present)
    {
        return Err(CodecError::UnsupportedTag);
    }
    Ok(())
}

pub(in crate::log) fn preflight(
    reader: &mut Reader<'_>,
    header: &common::RawHeader<'_>,
) -> Result<(), CodecError> {
    if header.intent.version() != 1 {
        return Err(CodecError::UnsupportedVersion);
    }
    let evidence = evidence(reader)?;
    let item = common::item(reader)?;
    check_tag(item.tag, item.priority, evidence)?;
    if item.now.get()
        != evidence
            .requested_now_ms
            .unwrap_or(header.stamp.hlc().physical().get())
    {
        return Err(CodecError::Malformed);
    }
    let scope = keys::scope_length(header.scope())?;
    let mut derived = scope;
    keys::charge(
        &mut derived,
        keys::state_length(scope, item.partition.len(), item.id.len())?,
        reader.limits.admission.decision_bytes(),
    )?;
    scan::operation(reader, item.tag, true)
}

pub(in crate::log) fn read(
    reader: &mut Reader<'_>,
    isolation: &[u8],
    state_keys: &mut Vec<Vec<u8>>,
) -> Result<PreparedOperation, CodecError> {
    let evidence = evidence(reader)?;
    let raw = common::item(reader)?;
    check_tag(raw.tag, raw.priority, evidence)?;
    state_keys
        .try_reserve_exact(1)
        .map_err(|_| CodecError::Allocation)?;
    state_keys.push(keys::state(isolation, raw.partition, raw.id)?);
    let item = Box::new(PreparedBatchItem {
        partition: PartitionId::new(reader::copy(raw.partition)?)
            .map_err(|_| CodecError::Malformed)?,
        command: CommittedCommand {
            id: RecordId::new(reader::copy(raw.id)?).map_err(|_| CodecError::Malformed)?,
            now: raw.now,
            operation: decode::operation(reader, raw.tag, true)?,
        },
        create_priority: raw.priority,
    });
    Ok(PreparedOperation::PublicMutation { item, evidence })
}
