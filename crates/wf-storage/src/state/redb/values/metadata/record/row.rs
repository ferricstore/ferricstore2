//! Borrowed companion/core association is validated before final field copies.

use std::mem::size_of;
use wf_domain::workflow::request::RequestScope;
use wf_ports::{
    blob::WorkflowValueBinding,
    state::{StateTransactionError as Error, StoredWorkflowRecordCapture},
};
use wf_types::identity::{PartitionId, RecordId};

use super::read::{self, RecordReadContext};
use crate::state::redb::{
    admission,
    values::{
        binding,
        budget::{product, total},
    },
};
use crate::state::{key, value};

pub(super) fn capture(
    context: &mut RecordReadContext,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    history: Option<&str>,
) -> Result<Option<StoredWorkflowRecordCapture>, Error> {
    context.charge.temporary(context.live(context.key_bound)?)?;
    let mut key = key::encode_binding_key(scope, partition, id, history, context.key_bound)
        .map_err(read::query_error)?;
    let live = context.live(context.key_bound)?;
    let raw = read::probe(&context.rows, &mut context.charge, &key, live)?;
    key[6] = if history.is_some() { 5 } else { 1 };
    let held = total(live, [raw.as_ref().map_or(0, |raw| raw.value().len())])?;
    let source = read::probe(&context.rows, &mut context.charge, &key, held)?;
    let (raw, source) = match (raw, source) {
        (None, None) => return Ok(None),
        (Some(raw), Some(source)) => (raw, source),
        _ => return Err(Error::Corrupt),
    };
    let data = value::decode_metadata_binding(raw.value(), context.facts.defaults)
        .map_err(admission::value_error)?;
    let record = value::scan_metadata_source(
        &data,
        source.value(),
        scope,
        (partition, id),
        history.map(str::as_bytes),
        context.store_limits.admission(),
        context.store_limits.maximum_recovery_bytes(),
    )
    .map_err(admission::value_error)?;
    if data.binding.versions.map(|v| v.is_some()) != record.value_fields().map(|v| v.is_some())
        || data
            .binding
            .versions
            .iter()
            .flatten()
            .any(|v| *v > record.version())
    {
        return Err(Error::Corrupt);
    }
    let core_owned = record.owned_bytes();
    let binding_owned = total(
        size_of::<WorkflowValueBinding>(),
        [product(
            data.binding.versions.iter().flatten().count(),
            total(partition.as_bytes().len(), [id.as_bytes().len()])?,
        )?],
    )?;
    let owned = total(core_owned, [data.metadata.len(), binding_owned])?;
    context.next_returned(owned)?;
    // Every final owner is admitted before materialization. Guards, escaped key,
    // new field owners and binding scratch may coexist throughout this phase.
    let live = context.live(total(
        context.key_bound,
        [raw.value().len(), source.value().len()],
    )?)?;
    context.charge.temporary(total(live, [owned])?)?;
    context.charge.work(owned)?;
    let values = binding::materialize(data.binding, partition, id, &mut context.charge, live)?;
    let metadata = admission::copy_bytes(data.metadata)?;
    let record = record
        .into_owned(core_owned)
        .map_err(admission::value_error)?;
    drop(raw);
    drop(source);
    context.returned(owned)?;
    Ok(Some(StoredWorkflowRecordCapture {
        record,
        values,
        metadata,
    }))
}
