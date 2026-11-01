//! Typed native query data; native sequences never enter legacy covering framing.
mod fields;
mod metadata;
mod model;
mod routing;
use super::budget::Budget;
use crate::state::{WorkflowRecordPolicyObservation as Policy, WorkflowRecordPolicyReadSnapshot};
pub use model::{
    NativeQueryPreparationError, NativeQueryPreparationLimits, NativeQueryRecord,
    NativeQuerySnapshot,
};
use std::mem::size_of;
use wf_domain::value::OriginalValueContext;
use wf_domain::{
    metadata::record::decode_original_record_metadata_bounded,
    policy::complete::{PolicySchemaCharge, PolicySchemaLimits, with_stored_policy_indexed_fields},
};

/// Prepare coherent native data without granting service or source-origin authority.
/// # Errors
/// Preserve resource, metadata, routing and policy failures without partial output.
pub fn prepare_native_query_snapshot(
    source: WorkflowRecordPolicyReadSnapshot,
    context: &OriginalValueContext<'_>,
    limits: NativeQueryPreparationLimits,
) -> Result<NativeQuerySnapshot, NativeQueryPreparationError> {
    use NativeQueryPreparationError as E;
    let (mut b, input_owned) = start(&source, limits)?;
    let prior_peak = source.record.temporary_bytes;
    let Some(capture) = source.record.capture else {
        if !matches!(source.current_policy, Policy::NotRead) {
            return Err(E::InvalidCapture);
        }
        b.output(input_owned).map_err(map_query)?;
        return Ok(NativeQuerySnapshot {
            control: source.record.control,
            record: None,
            work: b.work,
            temporary: b.peak.max(prior_peak),
            owned: input_owned,
        });
    };
    let policy = match &source.current_policy {
        Policy::NotRead => return Err(E::InvalidCapture),
        Policy::Absent => None,
        Policy::Present(p) => Some(p),
    };
    finish(
        capture,
        policy,
        (source.record.control, prior_peak, input_owned),
        context,
        limits,
        &mut b,
    )
}

fn start(
    source: &WorkflowRecordPolicyReadSnapshot,
    limits: NativeQueryPreparationLimits,
) -> Result<(Budget, usize), NativeQueryPreparationError> {
    use NativeQueryPreparationError as E;
    let minimum = fields::minimum(source).map_err(map_query)?;
    if source.record.returned_bytes < minimum {
        return Err(E::InvalidCapture);
    }
    let prior_peak = source.record.temporary_bytes;
    let prior_work = source.record.work_units;
    let input_owned = source
        .record
        .returned_bytes
        .checked_sub(size_of::<WorkflowRecordPolicyReadSnapshot>())
        .and_then(|n| n.checked_add(size_of::<NativeQuerySnapshot>()))
        .ok_or(E::Limit)?;
    let mut b = Budget::new(limits.query).map_err(map_query)?;
    b.spend(prior_work).map_err(map_query)?;
    b.admit(input_owned).map_err(map_query)?;
    if prior_peak > limits.query.maximum_temporary_bytes {
        return Err(E::Limit);
    }
    Ok((b, input_owned))
}

fn finish(
    mut capture: crate::state::StoredWorkflowRecordCapture,
    policy: Option<&wf_domain::policy::complete::StoredPolicyValue>,
    (control, prior_peak, input_owned): (wf_domain::workflow::control::ControlState, usize, usize),
    context: &OriginalValueContext<'_>,
    limits: NativeQueryPreparationLimits,
    b: &mut Budget,
) -> Result<NativeQuerySnapshot, NativeQueryPreparationError> {
    use NativeQueryPreparationError as E;
    let mut decode_limits = limits.metadata;
    decode_limits.max_work_units = decode_limits.max_work_units.min(remaining(b)?);
    let spare = limits
        .query
        .maximum_temporary_bytes
        .checked_sub(b.live())
        .ok_or(E::Limit)?;
    let (metadata, peak) = decode_original_record_metadata_bounded(
        std::mem::take(&mut capture.metadata),
        context,
        decode_limits,
        spare,
    )
    .map_err(map_metadata)?;
    b.peer(metadata.work_units(), peak).map_err(map_query)?;
    let descriptors = metadata.descriptor_bytes().ok_or(E::Limit)?;
    b.admit(descriptors).map_err(map_query)?;
    let (partition, scope) = routing::validate(&metadata, &capture.record.partition, b)?;
    fields::validate(&metadata, b).map_err(map_query)?;
    let schema = PolicySchemaLimits::new(
        limits.policy.policy_limits(),
        limits
            .policy
            .max_work_bytes()
            .min(u64::try_from(remaining(b)?).map_err(|_| E::Limit)?),
    )
    .map_err(|_| E::Limit)?;
    let mut charge = PolicySchemaCharge::with_scratch_limit(
        schema,
        u64::try_from(
            limits
                .query
                .maximum_temporary_bytes
                .checked_sub(b.live())
                .ok_or(E::Limit)?,
        )
        .map_err(|_| E::Limit)?,
    );
    let result = with_stored_policy_indexed_fields(
        policy,
        capture.record.record.workflow_type(),
        schema,
        &mut charge,
        |view| {
            b.peer(
                usize::try_from(view.work_units).map_err(|_| E::Limit)?,
                usize::try_from(view.temporary_bytes).map_err(|_| E::Limit)?,
            )
            .map_err(map_query)?;
            b.admit(usize::try_from(view.temporary_bytes).map_err(|_| E::Limit)?)
                .map_err(map_query)?;
            let final_owned = input_owned
                .checked_sub(view.owned_bytes)
                .and_then(|n| n.checked_add(descriptors))
                .ok_or(E::Limit)?;
            fields::configuration(&view, final_owned, b).map_err(map_query)
        },
    )
    .map_err(E::Policy)??;
    Ok(NativeQuerySnapshot {
        control,
        work: b.work,
        temporary: b.peak.max(prior_peak),
        owned: result.owned,
        record: Some(NativeQueryRecord {
            native: capture.record,
            values: capture.values,
            metadata,
            partition,
            scope,
            indexed: result.attributes,
            state_index: result.state,
            policy_profile: result.profile,
            policy_generation: result.generation,
        }),
    })
}

fn remaining(b: &Budget) -> Result<usize, NativeQueryPreparationError> {
    b.limits
        .maximum_work_units
        .checked_sub(b.work)
        .filter(|n| *n > 0)
        .ok_or(NativeQueryPreparationError::Limit)
}
fn map_query(e: super::OriginalQueryRecordError) -> NativeQueryPreparationError {
    match e {
        super::OriginalQueryRecordError::InvalidLimits => {
            NativeQueryPreparationError::InvalidLimits
        }
        super::OriginalQueryRecordError::Limit => NativeQueryPreparationError::Limit,
        e => NativeQueryPreparationError::Query(e),
    }
}
fn map_metadata(
    e: wf_domain::metadata::record::OriginalRecordMetadataError,
) -> NativeQueryPreparationError {
    use wf_domain::{
        metadata::record::OriginalRecordMetadataError as M, value::OriginalFlowValueError as V,
    };
    match e {
        M::InvalidLimits | M::Decoding(V::InvalidLimits) => {
            NativeQueryPreparationError::InvalidLimits
        }
        M::Limit | M::Decoding(V::Limit) => NativeQueryPreparationError::Limit,
        e => NativeQueryPreparationError::Metadata(e),
    }
}
