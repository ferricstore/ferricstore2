//! Complete physical identity and mandatory current/historical companions.

use std::mem::size_of;

use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{WorkflowValueBinding, WorkflowValueBindingKnowledge};
use wf_ports::state::{StateTransactionError as Error, StoredWorkflowHistoryRow};
use wf_types::identity::{PartitionId, RecordId};

use super::super::{
    admission,
    values::{
        binding,
        budget::{product, total},
    },
};
use super::read::{self, HistoryReadContext};
use crate::state::{
    key,
    value::{self, ValueBindingData},
};

pub(super) fn current(
    context: &mut HistoryReadContext,
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
) -> Result<(Option<u64>, Option<ValueBindingData>), Error> {
    let bound = context.key_bound(scope, partition, id, None)?;
    let key =
        key::encode_record_key(scope, partition, id, bound).map_err(admission::value_error)?;
    let live = context.live(key.len())?;
    let raw = read::probe(rows, &mut context.charge, &key, live)?;
    let found = raw
        .as_ref()
        .map(|raw| {
            value::binding_source_fields(
                raw.value(),
                scope,
                (partition, id),
                None,
                context.limits.admission(),
                context.limits.maximum_recovery_bytes(),
            )
            .map(|(version, fields)| {
                (
                    version,
                    fields.map(|fields| fields.map(|field| field.is_some())),
                )
            })
            .map_err(admission::value_error)
        })
        .transpose()?;
    drop(raw);
    drop(key);
    let data = data(context, rows, scope, partition, id, None)?;
    if context.facts.is_some() {
        if found.is_some() != data.is_some() {
            return Err(Error::Corrupt);
        }
        if let (Some((version, Some(present))), Some(data)) = (found, data) {
            validate_versions(data, version)?;
            if data.versions.map(|v| v.is_some()) != present {
                return Err(Error::Corrupt);
            }
        }
    }
    Ok((found.map(|(version, _)| version), data))
}

fn data(
    context: &mut HistoryReadContext,
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    history: Option<&str>,
) -> Result<Option<ValueBindingData>, Error> {
    let Some(facts) = context.facts else {
        return Ok(None);
    };
    let bound = context.key_bound(scope, partition, id, history)?;
    let key = key::encode_binding_key(scope, partition, id, history, bound)
        .map_err(admission::value_error)?;
    let live = context.live(key.len())?;
    let raw = read::probe(rows, &mut context.charge, &key, live)?;
    raw.map(|raw| {
        value::decode_binding(raw.value(), facts.defaults).map_err(admission::value_error)
    })
    .transpose()
}

pub(super) fn materialize(
    context: &mut HistoryReadContext,
    partition: &PartitionId,
    id: &RecordId,
    data: Option<ValueBindingData>,
) -> Result<WorkflowValueBindingKnowledge, Error> {
    if context.facts.is_none() {
        return Ok(WorkflowValueBindingKnowledge::LegacyUnavailable);
    }
    let Some(data) = data else {
        return Ok(WorkflowValueBindingKnowledge::Known(None));
    };
    let fields = total(partition.as_bytes().len(), [id.as_bytes().len()])?;
    let bytes = total(
        size_of::<WorkflowValueBinding>(),
        [product(fields, data.versions.iter().flatten().count())?],
    )?;
    context.returned(bytes)?;
    let live = context.live(0)?;
    let owner = binding::materialize(data, partition, id, &mut context.charge, live)?;
    Ok(WorkflowValueBindingKnowledge::Known(Some(owner)))
}

pub(super) fn capture(
    context: &mut HistoryReadContext,
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    physical: (&[u8], &[u8]),
    current: (u64, Option<ValueBindingData>),
) -> Result<StoredWorkflowHistoryRow, Error> {
    let (key_bytes, bytes) = physical;
    let before = context.working;
    let decoded_bound = total(128, [key_bytes.len()])?;
    context.working = total(before, [key_bytes.len(), bytes.len(), decoded_bound])?;
    context.temporary(0)?;
    let decoded = key::decode_history_key(
        key_bytes,
        scope,
        partition,
        id,
        context.limits.maximum_recovery_bytes(),
        decoded_bound,
    )
    .map_err(admission::value_error)?;
    let (_, original_id) = decoded.history_position().ok_or(Error::Corrupt)?;
    context.returned(event_bytes(bytes).map_err(admission::value_error)?)?;
    let event = value::decode_history(
        bytes,
        scope,
        partition,
        id,
        context.limits.maximum_recovery_bytes(),
    )
    .map_err(admission::value_error)?;
    if event.event_id != original_id || event.version > current.0 {
        return Err(Error::Corrupt);
    }
    let companion = data(context, rows, scope, partition, id, Some(&event.event_id))?;
    if let Some(current) = current.1 {
        let found = companion.ok_or(Error::Corrupt)?;
        validate_versions(found, event.version)?;
        same_birth(found, current)?;
    }
    let values = materialize(context, partition, id, companion)?;
    context.working = before;
    Ok(StoredWorkflowHistoryRow { event, values })
}

fn validate_versions(data: ValueBindingData, maximum: u64) -> Result<(), Error> {
    if data
        .versions
        .iter()
        .flatten()
        .any(|version| *version > maximum)
    {
        return Err(Error::Corrupt);
    }
    Ok(())
}

fn same_birth(found: ValueBindingData, current: ValueBindingData) -> Result<(), Error> {
    let (found, current) = (found.lifecycle, current.lifecycle);
    if found.incarnation != current.incarnation
        || found.birth != current.birth
        || found.configuration_digest != current.configuration_digest
        || found.policy_profile != current.policy_profile
        || found.policy_generation != current.policy_generation
        || found.policy_digest != current.policy_digest
    {
        return Err(Error::Corrupt);
    }
    Ok(())
}

// Measure only the fields copied by the existing decoder. Its complete semantic
// validation still runs before copying; this pass admits prospective ownership.
fn event_bytes(bytes: &[u8]) -> Result<usize, value::StateValueError> {
    let mut at = value::start(bytes, *b"FWSH", bytes.len())?;
    for _ in 0..5 {
        value::raw(bytes, &mut at)?;
    }
    let mut copied = value::raw(bytes, &mut at)?.len();
    value::take(bytes, &mut at, 33)?;
    value::add(&mut copied, value::raw(bytes, &mut at)?.len())?;
    skip_time(bytes, &mut at)?;
    value::read_u64(bytes, &mut at)?;
    if value::flag(bytes, &mut at)? {
        value::add(&mut copied, value::raw(bytes, &mut at)?.len())?;
    }
    if value::flag(bytes, &mut at)? {
        value::read_u8(bytes, &mut at)?;
        value::add(&mut copied, value::raw(bytes, &mut at)?.len())?;
        skip_time(bytes, &mut at)?;
        value::take(bytes, &mut at, 20)?;
        value::add(&mut copied, value::raw(bytes, &mut at)?.len())?;
    }
    value::finish(bytes, at)?;
    Ok(copied)
}

fn skip_time(bytes: &[u8], at: &mut usize) -> Result<(), value::StateValueError> {
    if value::flag(bytes, at)? {
        value::read_u64(bytes, at)?;
    }
    Ok(())
}
