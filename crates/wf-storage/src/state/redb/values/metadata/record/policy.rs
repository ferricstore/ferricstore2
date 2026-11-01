//! One committed snapshot for the actual core and current policy.
use super::read::{self, RecordReadContext};
use crate::state::redb::values::{
    budget::{product, total},
    read::scope_bytes,
};
use crate::state::{
    key,
    redb::{RedbStateStore, admission, original_policy},
};
use std::mem::size_of;
use wf_domain::policy::complete::{
    PolicySchemaCharge, PolicySchemaLimits, complete_policy_value_owned_bytes,
};
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{
    StateTransactionError as Error, StoredPolicyValue,
    WorkflowRecordPolicyObservation as Observation, WorkflowRecordPolicyReadLimits,
    WorkflowRecordPolicyReadPort, WorkflowRecordPolicyReadSnapshot, WorkflowRecordReadQuery,
    WorkflowRecordReadSnapshot,
};
use wf_types::identity::{PartitionId, RecordId};

impl WorkflowRecordPolicyReadPort for RedbStateStore {
    fn read_workflow_record_with_policy(
        &self,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        query: WorkflowRecordReadQuery<'_>,
        limits: WorkflowRecordPolicyReadLimits,
    ) -> Result<WorkflowRecordPolicyReadSnapshot, Error> {
        observe(self, scope, partition, id, query, limits).map_err(|error| self.failed(error))
    }
}

fn observe(
    store: &RedbStateStore,
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    query: WorkflowRecordReadQuery<'_>,
    limits: WorkflowRecordPolicyReadLimits,
) -> Result<WorkflowRecordPolicyReadSnapshot, Error> {
    let (mut context, control, capture) =
        read::begin(store, scope, partition, id, query, limits.record)?;
    context.returned(
        size_of::<WorkflowRecordPolicyReadSnapshot>() - size_of::<WorkflowRecordReadSnapshot>(),
    )?;
    let current_policy = match &capture {
        Some(record) => capture_policy(
            &mut context,
            scope,
            record.record.record.workflow_type(),
            limits.schema,
        )?,
        None => Observation::NotRead,
    };
    Ok(WorkflowRecordPolicyReadSnapshot {
        record: read::finish(context, control, capture),
        current_policy,
    })
}

pub(super) fn capture_policy(
    context: &mut RecordReadContext,
    scope: &RequestScope,
    kind: &[u8],
    requested: PolicySchemaLimits,
) -> Result<Observation, Error> {
    let ceiling = context.store_limits.schema();
    if requested.policy_limits().max_states() > ceiling.policy_limits().max_states()
        || requested.max_work_bytes() > ceiling.max_work_bytes()
    {
        return Err(Error::InvalidChanges);
    }
    let bound = total(64, [product(total(scope_bytes(scope), [kind.len()])?, 2)?])?;
    context.charge.temporary(context.live(bound)?)?;
    let key = key::encode_policy_key(scope, kind, bound).map_err(admission::value_error)?;
    let live = context.live(key.len())?;
    let Some(raw) = read::probe(&context.rows, &mut context.charge, &key, live)? else {
        return Ok(Observation::Absent);
    };
    let live = total(live, [raw.value().len()])?;
    let remaining = u64::try_from(context.charge.remaining_work()).map_err(|_| Error::Limit)?;
    let schema = PolicySchemaLimits::new(
        requested.policy_limits(),
        requested.max_work_bytes().min(remaining),
    )
    .map_err(|_| Error::Limit)?;
    let mut schema_charge = PolicySchemaCharge::new(schema);
    let (value, _) = original_policy::decode_stored_admitted(
        raw.value(),
        kind,
        schema,
        &mut schema_charge,
        context.facts.layout,
        |owner| context.charge.temporary(total(live, [owner])?),
    )?;
    let returned = match &value {
        StoredPolicyValue::Complete(value) => {
            complete_policy_value_owned_bytes(value, schema, &mut schema_charge).map_err(
                |error| {
                    admission::policy_error(
                        wf_domain::policy::complete::CompletePolicyEncodingError::Schema(error),
                    )
                },
            )?
        }
        StoredPolicyValue::Original(bytes) => bytes.len(),
    };
    context
        .charge
        .work(usize::try_from(schema_charge.total()).map_err(|_| Error::Limit)?)?;
    context.charge.temporary(total(live, [returned])?)?;
    drop(raw);
    context.returned(returned)?;
    Ok(Observation::Present(value))
}
