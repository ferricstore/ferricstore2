//! Separately admitted descriptors borrowing stable query and current owners.

use std::collections::BTreeMap;

use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder, ScopedRecordId};
use wf_domain::policy::InstalledRetryPolicy;
use wf_domain::policy::definition::{PolicySnapshot, StateMode};
use wf_domain::workflow::indexed::TouchedIndexedGroupView;
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{
    FreshRecordBatch, RecordBatchApplyPlan, WorkflowValueDecision,
    decide_fresh_atomic_start_touched, decide_fresh_independent_record_batch_touched,
    decide_fresh_public_mutation_touched, decide_fresh_record_batch_touched,
    decide_fresh_step_continue_touched, decide_fresh_workflow_value_batch_touched,
};
use wf_ports::state::{GroupApplyTransaction, StateTransactionError, StoredWorkflowRecord};
use wf_types::identity::{PartitionId, RecordId};

use super::load::{LoadedRecordBatch, debit, entry, reserve_slots};
use super::{RecordBatchTransactionError, RecordBatchTransactionLimits};

pub(super) struct RecordBatchObservationView<'query, 'current> {
    pub(super) queries: &'query [(PartitionId, RecordId)],
    pub(super) records: &'current [Option<StoredWorkflowRecord>],
    pub(super) placements: &'query BTreeMap<PartitionId, wf_types::identity::GroupId>,
    pub(super) policy_types: &'query [Vec<u8>],
    pub(super) projections: &'query [Option<(PolicySnapshot, InstalledRetryPolicy)>],
    pub(super) parallel: &'query [(usize, bool)],
    pub(super) members: &'query [(usize, bool)],
    pub(super) heads: &'current [(usize, Option<StoredWorkflowRecord>)],
    pub(super) staging_used: &'query mut usize,
}

pub(super) fn decide<'current>(
    fresh: FreshRecordBatch<'_>,
    scope: &RequestScope,
    view: RecordBatchObservationView<'_, 'current>,
    limits: RecordBatchTransactionLimits,
    live_bytes: usize,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchTransactionError> {
    let (plan, ordering) = decide_profile(fresh, scope, view, limits, live_bytes, false)?;
    drop(ordering);
    Ok(plan)
}

pub(super) fn decide_values<'current>(
    fresh: FreshRecordBatch<'_>,
    scope: &RequestScope,
    view: RecordBatchObservationView<'_, 'current>,
    limits: RecordBatchTransactionLimits,
    live_bytes: usize,
) -> Result<WorkflowValueDecision<'current>, RecordBatchTransactionError> {
    decide_profile(fresh, scope, view, limits, live_bytes, true)
}

fn decide_profile<'current>(
    fresh: FreshRecordBatch<'_>,
    scope: &RequestScope,
    view: RecordBatchObservationView<'_, 'current>,
    limits: RecordBatchTransactionLimits,
    live_bytes: usize,
    values: bool,
) -> Result<WorkflowValueDecision<'current>, RecordBatchTransactionError> {
    let RecordBatchObservationView {
        queries,
        records,
        placements,
        policy_types,
        projections,
        parallel,
        members,
        heads,
        staging_used,
    } = view;
    let cap = limits.maximum_staging_bytes;
    let current = current_facts(queries, records, staging_used, cap)?;
    let parallel = parallel_facts(parallel, records, staging_used, cap)?;
    let members = member_facts(members, records, staging_used, cap)?;
    let heads = head_facts(heads, records, staging_used, cap)?;
    let mut modes = BTreeMap::<&[u8], Option<&PolicySnapshot>>::new();
    let mut retries = BTreeMap::<&[u8], Option<&InstalledRetryPolicy>>::new();
    for (index, kind) in policy_types.iter().enumerate() {
        let projection = projections
            .get(index)
            .ok_or(StateTransactionError::Corrupt)?;
        debit(staging_used, 256, cap)?;
        modes.insert(kind, projection.as_ref().map(|(mode, _)| mode));
        debit(staging_used, 256, cap)?;
        retries.insert(kind, projection.as_ref().map(|(_, retry)| retry));
    }
    let view = TouchedIndexedGroupView {
        group: scope.group(),
        placements,
        current: &current,
        parallel: &parallel,
        fifo_members: &members,
        fifo_heads: &heads,
        policies: &modes,
        retry_policies: &retries,
    };
    if values {
        return Ok(decide_fresh_workflow_value_batch_touched(
            fresh,
            view,
            live_bytes,
            limits.outcome_ttl_ms,
        )?);
    }
    let plan = if matches!(
        fresh.decoded().request().operation,
        wf_ports::consensus::PreparedOperation::PublicMutation { .. }
    ) {
        decide_fresh_public_mutation_touched(fresh, view, live_bytes, limits.outcome_ttl_ms)
    } else if fresh.decoded().request().intent.version() == 6 {
        decide_fresh_independent_record_batch_touched(
            fresh,
            view,
            live_bytes,
            limits.outcome_ttl_ms,
        )
    } else if matches!(fresh.decoded().request().intent.version(), 10 | 11) {
        decide_fresh_step_continue_touched(fresh, view, live_bytes, limits.outcome_ttl_ms)
    } else if fresh.decoded().request().intent.version() == 7 {
        decide_fresh_atomic_start_touched(fresh, view, live_bytes, limits.outcome_ttl_ms)
    } else {
        decide_fresh_record_batch_touched(fresh, view, live_bytes, limits.outcome_ttl_ms)
    }?;
    Ok((plan, Vec::new()))
}

fn current_facts<'query, 'current>(
    queries: &'query [(PartitionId, RecordId)],
    records: &'current [Option<StoredWorkflowRecord>],
    used: &mut usize,
    cap: usize,
) -> Result<
    BTreeMap<ScopedRecordId<'query>, Option<ClaimRecord<'current>>>,
    RecordBatchTransactionError,
> {
    if queries.len() != records.len() {
        return Err(StateTransactionError::Corrupt.into());
    }
    let mut facts = BTreeMap::new();
    for ((partition, id), record) in queries.iter().zip(records) {
        debit(used, 128, cap)?;
        facts.insert(
            ScopedRecordId {
                partition_key: partition.as_bytes(),
                id: id.as_bytes(),
            },
            record.as_ref().map(entry),
        );
    }
    Ok(facts)
}

fn parallel_facts<'current>(
    observations: &[(usize, bool)],
    records: &'current [Option<StoredWorkflowRecord>],
    used: &mut usize,
    cap: usize,
) -> Result<BTreeMap<DueOrder<'current>, bool>, RecordBatchTransactionError> {
    let mut facts = BTreeMap::new();
    for (index, present) in observations {
        let record = current_row(records, *index)?;
        let due = DueOrder::for_record(&record).ok_or(StateTransactionError::Corrupt)?;
        debit(used, 128, cap)?;
        facts.insert(due, *present);
    }
    Ok(facts)
}

fn member_facts<'current>(
    observations: &[(usize, bool)],
    records: &'current [Option<StoredWorkflowRecord>],
    used: &mut usize,
    cap: usize,
) -> Result<
    BTreeMap<(LaneKey<'current>, LaneOrder<'current>), Option<ClaimRecord<'current>>>,
    RecordBatchTransactionError,
> {
    let mut facts = BTreeMap::new();
    for (index, present) in observations {
        let record = current_row(records, *index)?;
        let lane = LaneKey::for_record(&record).ok_or(StateTransactionError::Corrupt)?;
        debit(used, 256, cap)?;
        facts.insert(
            (lane, LaneOrder::for_record(&record)),
            present.then_some(record),
        );
    }
    Ok(facts)
}

fn head_facts<'current>(
    observations: &'current [(usize, Option<StoredWorkflowRecord>)],
    records: &'current [Option<StoredWorkflowRecord>],
    used: &mut usize,
    cap: usize,
) -> Result<
    BTreeMap<LaneKey<'current>, Option<(LaneOrder<'current>, ClaimRecord<'current>)>>,
    RecordBatchTransactionError,
> {
    let mut facts = BTreeMap::new();
    for (index, head) in observations {
        let record = current_row(records, *index)?;
        let lane = LaneKey::for_record(&record).ok_or(StateTransactionError::Corrupt)?;
        let head = head.as_ref().map(|head| {
            let row = entry(head);
            (LaneOrder::for_record(&row), row)
        });
        debit(used, 256, cap)?;
        facts.insert(lane, head);
    }
    Ok(facts)
}

fn current_row(
    records: &[Option<StoredWorkflowRecord>],
    index: usize,
) -> Result<ClaimRecord<'_>, StateTransactionError> {
    records
        .get(index)
        .and_then(Option::as_ref)
        .map(entry)
        .ok_or(StateTransactionError::Corrupt)
}

pub(super) fn load_indexes<T: GroupApplyTransaction>(
    loaded: &mut LoadedRecordBatch,
    tx: &T,
    scope: &RequestScope,
    cap: usize,
) -> Result<(), RecordBatchTransactionError> {
    let count = loaded.records.len();
    reserve_slots(&mut loaded.parallel, count, &mut loaded.staging_used, cap)?;
    reserve_slots(&mut loaded.members, count, &mut loaded.staging_used, cap)?;
    reserve_slots(&mut loaded.heads, count, &mut loaded.staging_used, cap)?;
    let mut type_positions = BTreeMap::<&[u8], usize>::new();
    for (index, kind) in loaded.policy_types.iter().enumerate() {
        debit(&mut loaded.staging_used, 128, cap)?;
        type_positions.insert(kind, index);
    }
    for (index, owner) in loaded.records.iter().enumerate() {
        let Some(owner) = owner else {
            continue;
        };
        let row = entry(owner);
        let Some(lane) = LaneKey::for_record(&row) else {
            continue;
        };
        let mode = type_positions
            .get(row.record.workflow_type())
            .and_then(|at| loaded.projections.get(*at))
            .ok_or(StateTransactionError::Corrupt)?
            .as_ref()
            .map_or(StateMode::Parallel, |(mode, _)| {
                mode.state_mode(row.logical_state())
            });
        let due = DueOrder::for_record(&row);
        if let Some(due) = due {
            loaded
                .parallel
                .push((index, tx.parallel_member(scope.tenant(), due)?));
        }
        match mode {
            StateMode::Fifo => {
                let member = tx.fifo_member(scope.tenant(), lane, LaneOrder::for_record(&row))?;
                if member.as_ref().is_some_and(|member| member != owner) {
                    return Err(StateTransactionError::Corrupt.into());
                }
                loaded.members.push((index, member.is_some()));
                drop(member);
            }
            StateMode::Parallel if due.is_some() => {
                let head = tx.fifo_head(scope.tenant(), lane)?;
                if head.as_ref().is_some_and(|head| {
                    head.tenant != *scope.tenant()
                        || LaneKey::for_record(&entry(head)) != Some(lane)
                }) {
                    return Err(StateTransactionError::Corrupt.into());
                }
                loaded.heads.push((index, head));
            }
            StateMode::Parallel => {}
        }
    }
    Ok(())
}
