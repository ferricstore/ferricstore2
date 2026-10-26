//! Fresh target guards and exact installed backing, before reducer ownership moves.

use std::mem::size_of;

use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    OriginalPolicyReadError, OriginalPolicyReadValue, PolicySchemaCharge, StoredPolicyValue,
    canonical_original_etf_safety, decode_original_complete_policy_entry_portable,
};
use wf_domain::workflow::control::PreparedDigest;
use wf_ports::consensus::{
    WorkflowValueDefaults, WorkflowValueItemInput, WorkflowValueTargetGuard,
};
use wf_ports::log::{DecodedRequest, prepare_original_complete_policy_reference};
use wf_ports::state::StateTransactionError;
use wf_types::version::TimestampMillis;

use crate::state::redb::values::budget::{ValueCharge, product, total};
use crate::state::value::ValueBindingData;
use crate::transaction::load::LoadedRecordBatch;

use super::{
    WorkflowValueTransactionError as Error, WorkflowValueTransactionLimits, record_error,
    storage_error,
};

#[derive(Clone, Copy)]
pub(super) struct BoundValuePolicy {
    pub(super) loaded_index: usize,
    pub(super) profile: u8,
    pub(super) generation: u64,
    pub(super) digest: PreparedDigest,
}

pub(super) struct ValueInputs {
    pub(super) prepared: PreparedDigest,
    pub(super) mode: u8,
    pub(super) defaults: WorkflowValueDefaults,
    pub(super) inputs: Vec<WorkflowValueItemInput>,
    pub(super) policies: Vec<BoundValuePolicy>,
}

pub(super) fn targets(
    decoded: &DecodedRequest<'_>,
    loaded: &LoadedRecordBatch,
    bindings: &[Option<ValueBindingData>],
    execution: TimestampMillis,
    charge: &mut ValueCharge,
) -> Result<(), Error> {
    charge.helper().map_err(storage_error)?;
    let metadata = decoded
        .workflow_value_metadata()
        .ok_or(Error::UnsupportedProfile)?;
    let items = decoded
        .request()
        .operation
        .mutation_items()
        .ok_or(Error::UnsupportedProfile)?;
    if items.len() != metadata.inputs.len() || bindings.len() != loaded.queries.len() {
        return Err(Error::InvalidInput);
    }
    charge
        .work(product(items.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    for (item, input) in items.iter().zip(metadata.inputs) {
        charge.candidate().map_err(storage_error)?;
        let index = query(loaded, &item.partition, &item.command.id, charge)?;
        let current = loaded.records[index].as_ref();
        let binding = bindings[index];
        let witness = metadata
            .policies
            .get(input.policy_index as usize)
            .ok_or(Error::InvalidInput)?;
        let same_type = if let Some(current) = current {
            charge
                .compare(
                    current.record.workflow_type().len(),
                    witness.workflow_type.len(),
                )
                .map_err(storage_error)?;
            current.record.workflow_type() == witness.workflow_type
        } else {
            true
        };
        let matches = match input.target {
            WorkflowValueTargetGuard::Unspecified => true,
            WorkflowValueTargetGuard::Absent => current.is_none(),
            WorkflowValueTargetGuard::Present { incarnation } => {
                same_type && binding.is_some_and(|value| value.lifecycle.incarnation == incarnation)
            }
        };
        if !matches {
            return Err(Error::StalePolicyTarget);
        }
        if binding.is_some_and(|value| {
            value
                .lifecycle
                .terminal_deadline
                .is_some_and(|end| end <= execution)
        }) {
            return Err(Error::UnsupportedProfile);
        }
    }
    match metadata.policy_references_captured {
        None => Err(Error::PolicyReferenceRequired),
        Some(false) => Err(Error::InvalidInput),
        Some(true) => Ok(()),
    }
}

pub(super) fn bind(
    decoded: &DecodedRequest<'_>,
    loaded: &LoadedRecordBatch,
    defaults: WorkflowValueDefaults,
    limits: &WorkflowValueTransactionLimits,
    schema: &mut PolicySchemaCharge,
    charge: &mut ValueCharge,
) -> Result<ValueInputs, Error> {
    charge.helper().map_err(storage_error)?;
    let metadata = decoded
        .workflow_value_metadata()
        .ok_or(Error::UnsupportedProfile)?;
    if metadata.defaults != defaults {
        return Err(Error::InvalidInput);
    }
    let fixed = total(
        size_of::<ValueInputs>(),
        [
            size_of::<PolicySchemaCharge>(),
            size_of::<ValueCharge>(),
            size_of::<Vec<Option<ValueBindingData>>>(),
            product(loaded.queries.len(), size_of::<Option<ValueBindingData>>())
                .map_err(storage_error)?,
        ],
    )
    .map_err(storage_error)?;
    let live = total(
        fixed,
        [
            product(metadata.inputs.len(), size_of::<WorkflowValueItemInput>())
                .map_err(storage_error)?,
            product(metadata.policies.len(), size_of::<BoundValuePolicy>())
                .map_err(storage_error)?,
        ],
    )
    .map_err(storage_error)?;
    charge.temporary(live).map_err(storage_error)?;
    let mut inputs = charge
        .reserve(metadata.inputs.len(), fixed)
        .map_err(storage_error)?;
    charge
        .work(product(metadata.inputs.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    for input in metadata.inputs {
        charge.candidate().map_err(storage_error)?;
        inputs.push(*input);
    }
    let input_live = total(
        fixed,
        [
            product(metadata.inputs.len(), size_of::<WorkflowValueItemInput>())
                .map_err(storage_error)?,
        ],
    )
    .map_err(storage_error)?;
    let mut policies = charge
        .reserve(metadata.policies.len(), input_live)
        .map_err(storage_error)?;
    charge
        .work(product(metadata.policies.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    for witness in metadata.policies {
        charge.candidate().map_err(storage_error)?;
        let index = policy_index(loaded, &witness.workflow_type, charge)?;
        let (profile, generation, digest) = backing(
            loaded.policies.get(index).and_then(Option::as_ref),
            &witness.workflow_type,
            limits,
            schema,
            charge,
            live,
        )?;
        if witness.generation > generation {
            return Err(Error::PolicyGenerationNotApplied);
        }
        if witness.generation < generation {
            return Err(Error::StalePolicyGeneration);
        }
        if witness.profile != profile || witness.digest != digest {
            return Err(Error::ConflictingPolicyGeneration);
        }
        policies.push(BoundValuePolicy {
            loaded_index: index,
            profile,
            generation,
            digest,
        });
    }
    Ok(ValueInputs {
        prepared: decoded.digest(),
        mode: metadata.profile,
        defaults,
        inputs,
        policies,
    })
}

fn backing(
    policy: Option<&StoredPolicyValue>,
    kind: &[u8],
    limits: &WorkflowValueTransactionLimits,
    schema: &mut PolicySchemaCharge,
    charge: &mut ValueCharge,
    live: usize,
) -> Result<(u8, u64, PreparedDigest), Error> {
    charge.helper().map_err(storage_error)?;
    match policy {
        None => {
            charge.work(kind.len()).map_err(storage_error)?;
            charge
                .temporary(
                    total(
                        live,
                        [65_536, product(kind.len(), 256).map_err(storage_error)?],
                    )
                    .map_err(storage_error)?,
                )
                .map_err(storage_error)?;
            let reference = prepare_original_complete_policy_reference(
                OriginalPolicyReadValue::Absent,
                kind,
                &canonical_original_etf_safety(),
                limits.record_batch.schema,
                schema,
            )
            .map_err(|error| match error {
                OriginalPolicyReadError::Encoding(error) => record_error(error.into()),
                _ => storage_error(StateTransactionError::Corrupt),
            })?;
            Ok((0, reference.generation(), reference.digest()))
        }
        Some(StoredPolicyValue::Complete(value)) => {
            charge.work(value.encoded().len()).map_err(storage_error)?;
            Ok((
                1,
                value.generation(),
                PreparedDigest::new(Sha256::digest(value.encoded()).into()),
            ))
        }
        Some(StoredPolicyValue::Original(bytes)) => {
            charge.work(bytes.len()).map_err(storage_error)?;
            charge
                .temporary(
                    total(
                        live,
                        [65_536, product(bytes.len(), 256).map_err(storage_error)?],
                    )
                    .map_err(storage_error)?,
                )
                .map_err(storage_error)?;
            let entry = decode_original_complete_policy_entry_portable(
                bytes,
                limits.record_batch.schema,
                schema,
            )
            .map_err(|error| record_error(error.into()))?
            .ok_or(Error::UnsupportedProfile)?;
            Ok((
                2,
                entry.generation(),
                PreparedDigest::new(Sha256::digest(bytes).into()),
            ))
        }
    }
}

pub(super) fn query(
    loaded: &LoadedRecordBatch,
    partition: &wf_types::identity::PartitionId,
    id: &wf_types::identity::RecordId,
    charge: &mut ValueCharge,
) -> Result<usize, Error> {
    charge
        .work(product(loaded.queries.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    for (index, (candidate_partition, candidate_id)) in loaded.queries.iter().enumerate() {
        charge.candidate().map_err(storage_error)?;
        charge
            .compare(
                candidate_partition.as_bytes().len(),
                partition.as_bytes().len(),
            )
            .map_err(storage_error)?;
        charge
            .compare(candidate_id.as_bytes().len(), id.as_bytes().len())
            .map_err(storage_error)?;
        if candidate_partition == partition && candidate_id == id {
            return Ok(index);
        }
    }
    Err(storage_error(StateTransactionError::Corrupt))
}

fn policy_index(
    loaded: &LoadedRecordBatch,
    kind: &[u8],
    charge: &mut ValueCharge,
) -> Result<usize, Error> {
    charge
        .work(product(loaded.policy_types.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    for (index, candidate) in loaded.policy_types.iter().enumerate() {
        charge.candidate().map_err(storage_error)?;
        charge
            .compare(candidate.len(), kind.len())
            .map_err(storage_error)?;
        if candidate == kind {
            return Ok(index);
        }
    }
    Err(storage_error(StateTransactionError::Corrupt))
}

pub(super) fn types(
    decoded: &DecodedRequest<'_>,
    loaded: &mut LoadedRecordBatch,
    charge: &mut ValueCharge,
    maximum: usize,
) -> Result<(), Error> {
    charge.helper().map_err(storage_error)?;
    let metadata = decoded
        .workflow_value_metadata()
        .ok_or(Error::UnsupportedProfile)?;
    let slots =
        total(loaded.policy_types.len(), [metadata.policies.len()]).map_err(storage_error)?;
    let spine = product(slots, size_of::<Vec<u8>>()).map_err(storage_error)?;
    crate::transaction::load::debit(&mut loaded.staging_used, spine, maximum)
        .map_err(record_error)?;
    charge
        .work(product(loaded.policy_types.len(), size_of::<Vec<u8>>()).map_err(storage_error)?)
        .map_err(storage_error)?;
    loaded
        .policy_types
        .try_reserve_exact(metadata.policies.len())
        .map_err(|_| storage_error(StateTransactionError::Allocation))?;
    charge
        .work(product(metadata.policies.len(), 128).map_err(storage_error)?)
        .map_err(storage_error)?;
    for witness in metadata.policies {
        charge.candidate().map_err(storage_error)?;
        let mut found = false;
        charge
            .work(product(loaded.policy_types.len(), 128).map_err(storage_error)?)
            .map_err(storage_error)?;
        for current in &loaded.policy_types {
            charge.candidate().map_err(storage_error)?;
            charge
                .compare(current.len(), witness.workflow_type.len())
                .map_err(storage_error)?;
            if *current == witness.workflow_type {
                found = true;
                break;
            }
        }
        if found {
            continue;
        }
        crate::transaction::load::debit(
            &mut loaded.staging_used,
            witness.workflow_type.len(),
            maximum,
        )
        .map_err(record_error)?;
        charge
            .work(witness.workflow_type.len())
            .map_err(storage_error)?;
        let mut kind = Vec::new();
        kind.try_reserve_exact(witness.workflow_type.len())
            .map_err(|_| storage_error(StateTransactionError::Allocation))?;
        kind.extend_from_slice(&witness.workflow_type);
        loaded.policy_types.push(kind);
    }
    Ok(())
}
