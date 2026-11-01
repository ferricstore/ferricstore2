//! Validated captured user fields and small owned actual-policy configuration.
use super::super::{
    OriginalQueryRecordError as E,
    budget::{Budget, add},
    writer::Writer,
};
use crate::state::{WorkflowRecordPolicyObservation as Policy, WorkflowRecordPolicyReadSnapshot};
use wf_domain::{
    metadata::record::{OriginalRecordMetadata as M, OriginalRecordMetadataField as F},
    policy::complete::BorrowedPolicyIndexedFields as Config,
};

pub(super) struct OwnedIndexedConfiguration {
    pub(super) attributes: [Option<Vec<u8>>; 3],
    pub(super) state: Option<Vec<u8>>,
    pub(super) profile: u8,
    pub(super) generation: Option<u64>,
    pub(super) owned: usize,
}

pub(super) fn minimum(source: &WorkflowRecordPolicyReadSnapshot) -> Result<usize, E> {
    let mut size = std::mem::size_of::<WorkflowRecordPolicyReadSnapshot>();
    if let Some(capture) = &source.record.capture {
        let record = &capture.record.record;
        for bytes in [
            record.id().as_bytes(),
            record.workflow_type(),
            record.state(),
            capture.record.partition.as_bytes(),
            record.payload().unwrap_or_default(),
            record.result().unwrap_or_default(),
            record.error().unwrap_or_default(),
            record.run_state().unwrap_or_default(),
            &capture.metadata,
        ] {
            size = add(size, bytes.len())?;
        }
        if let Some(lease) = record.lease() {
            size = add(size, add(lease.worker.len(), lease.token.as_bytes().len())?)?;
        }
        size = add(
            size,
            std::mem::size_of::<crate::blob::WorkflowValueBinding>(),
        )?;
        for reference in [
            &capture.values.refs.payload,
            &capture.values.refs.result,
            &capture.values.refs.error,
        ]
        .into_iter()
        .flatten()
        {
            use wf_domain::workflow::active::ActiveValueRef;
            let extra = match reference {
                ActiveValueRef::Opaque(bytes) => bytes.len(),
                ActiveValueRef::Versioned { partition, id, .. } => {
                    add(partition.as_bytes().len(), id.as_bytes().len())?
                }
            };
            size = add(size, extra)?;
        }
    }
    if let Policy::Present(value) = &source.current_policy {
        size = add(size, value.encoded().len())?;
    }
    Ok(size)
}

pub(super) fn validate(m: &M, b: &mut Budget) -> Result<(), E> {
    for (field, index) in [(F::Parent, 13), (F::Root, 14), (F::Correlation, 15)] {
        if let Some(input) = m.field_input(field) {
            let value = super::metadata::scalar(input, input.root, b)?;
            super::super::metadata::validate_builtin(index, value, b)?;
            super::super::primitive::emit(value, 32768, false, &mut Writer::measure(), b)?;
        }
    }
    super::metadata::validate(m, b)
}
pub(super) fn configuration(
    config: &Config<'_>,
    owned: usize,
    b: &mut Budget,
) -> Result<OwnedIndexedConfiguration, E> {
    let mut final_owned = owned;
    for name in config
        .attributes
        .into_iter()
        .chain([config.state_metadata])
        .flatten()
    {
        final_owned = add(final_owned, name.len())?;
    }
    b.output(final_owned)?;
    let mut indexed = std::array::from_fn(|_| None);
    for (slot, source) in indexed.iter_mut().zip(config.attributes) {
        if let Some(name) = source {
            let mut bytes = b.reserve(name.len())?;
            bytes.extend_from_slice(name);
            *slot = Some(bytes);
        }
    }
    let state = if let Some(name) = config.state_metadata {
        let mut bytes = b.reserve(name.len())?;
        bytes.extend_from_slice(name);
        Some(bytes)
    } else {
        None
    };
    Ok(OwnedIndexedConfiguration {
        attributes: indexed,
        state,
        profile: config.profile,
        generation: config.generation,
        owned: final_owned,
    })
}
