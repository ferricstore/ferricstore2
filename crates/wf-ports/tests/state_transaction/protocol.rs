//! A finite recording contract double. Its configured reads are not evidence
//! that an engine provides complete authority, atomic publication or durability.

use std::cell::{Cell, RefCell};

use super::{TestResult, clock, sample_record, scope};
use wf_domain::claim::apply::{ClaimQuery, ClaimStates};
use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey, LaneOrder, PartitionFilter};
use wf_domain::policy::complete::{CompletePolicyValue, PolicySchemaCharge, PolicySchemaLimits};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::request::{RequestScope, RetainedOutcome};
use wf_ports::state::{
    DueCursorStep, GroupApplyChanges, GroupApplyStore, GroupApplyTransaction, GroupDueCursor,
    StateTransactionError, StoredWorkflowRecord,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};

struct RecordingGroupStore<'input> {
    scope: &'input RequestScope,
    control: ControlState,
    records: &'input [StoredWorkflowRecord],
    outcomes: &'input [RetainedOutcome],
    rows: &'input [DueOrder<'input>],
    calls: RefCell<Vec<&'static str>>,
    committed: Cell<bool>,
}

struct RecordingGroupTransaction<'store, 'input> {
    store: &'store mut RecordingGroupStore<'input>,
    group: &'store GroupId,
    recovery: &'store RecoveryId,
    maximum_steps: usize,
    maximum_bytes: usize,
    steps: Cell<usize>,
    bytes: Cell<usize>,
}

struct RecordingDueCursor<'tx, 'store, 'input> {
    transaction: &'tx RecordingGroupTransaction<'store, 'input>,
    query: ClaimQuery<'tx>,
    index: usize,
    position: Option<DueOrder<'input>>,
    maximum_rows: usize,
    maximum_bytes: usize,
    returned: usize,
    bytes: usize,
    terminal: Option<bool>,
}

impl<'input> GroupApplyStore for RecordingGroupStore<'input> {
    type Transaction<'s>
        = RecordingGroupTransaction<'s, 'input>
    where
        Self: 's;

    fn begin_apply<'s>(
        &'s mut self,
        group: &'s GroupId,
        recovery: &'s RecoveryId,
        maximum_read_steps: usize,
        maximum_read_bytes: usize,
        maximum_temporary_bytes: usize,
    ) -> Result<RecordingGroupTransaction<'s, 'input>, StateTransactionError> {
        if maximum_read_steps == 0 || maximum_read_bytes == 0 || maximum_temporary_bytes == 0 {
            return Err(StateTransactionError::Limit);
        }
        if self.scope.group() != group || self.scope.recovery() != recovery {
            return Err(StateTransactionError::Corrupt);
        }
        self.calls.borrow_mut().push("begin");
        Ok(RecordingGroupTransaction {
            store: self,
            group,
            recovery,
            maximum_steps: maximum_read_steps,
            maximum_bytes: maximum_read_bytes,
            steps: Cell::new(0),
            bytes: Cell::new(0),
        })
    }
}

impl RecordingGroupTransaction<'_, '_> {
    fn step(&self, name: &'static str) -> Result<(), StateTransactionError> {
        let next = self
            .steps
            .get()
            .checked_add(1)
            .ok_or(StateTransactionError::Limit)?;
        if next > self.maximum_steps {
            return Err(StateTransactionError::Limit);
        }
        self.steps.set(next);
        self.store.calls.borrow_mut().push(name);
        Ok(())
    }

    fn admit(&self, bytes: usize) -> Result<(), StateTransactionError> {
        let next = self
            .bytes
            .get()
            .checked_add(bytes)
            .ok_or(StateTransactionError::Limit)?;
        if next > self.maximum_bytes {
            return Err(StateTransactionError::Limit);
        }
        self.bytes.set(next);
        Ok(())
    }

    fn copy(
        &self,
        owner: &StoredWorkflowRecord,
    ) -> Result<StoredWorkflowRecord, StateTransactionError> {
        self.admit(record_bytes(owner))?;
        Ok(owner.clone())
    }
}

impl<'store, 'input> GroupApplyTransaction for RecordingGroupTransaction<'store, 'input> {
    type DueCursor<'tx>
        = RecordingDueCursor<'tx, 'store, 'input>
    where
        Self: 'tx;

    fn placement(
        &self,
        _partition: &PartitionId,
    ) -> Result<Option<GroupId>, StateTransactionError> {
        Err(StateTransactionError::Unavailable)
    }

    fn installed_policy(
        &self,
        _tenant: &TenantId,
        _workflow_type: &[u8],
        _limits: PolicySchemaLimits,
        _charge: &mut PolicySchemaCharge,
    ) -> Result<Option<CompletePolicyValue>, StateTransactionError> {
        Err(StateTransactionError::Unavailable)
    }

    fn control(&self) -> Result<ControlState, StateTransactionError> {
        self.step("control")?;
        self.admit(512)?;
        Ok(self.store.control)
    }

    fn outcome(
        &self,
        tenant: &TenantId,
        request: &RequestId,
    ) -> Result<Option<RetainedOutcome>, StateTransactionError> {
        self.step("outcome-miss")?;
        self.store
            .outcomes
            .iter()
            .find(|outcome| {
                outcome.scope().tenant() == tenant
                    && outcome.scope().group() == self.group
                    && outcome.scope().recovery() == self.recovery
                    && outcome.request() == request
            })
            .map(|outcome| {
                self.admit(
                    512 + outcome.scope().tenant().as_bytes().len()
                        + outcome.scope().group().as_bytes().len()
                        + outcome.scope().recovery().as_bytes().len()
                        + outcome.request().as_bytes().len()
                        + outcome.response().len(),
                )?;
                Ok(outcome.clone())
            })
            .transpose()
    }

    fn retained_nonempty(&self, tenant: &TenantId) -> Result<bool, StateTransactionError> {
        self.step("retained-open")?;
        self.step("retained-first")?;
        Ok(self.store.outcomes.iter().any(|outcome| {
            outcome.scope().tenant() == tenant
                && outcome.scope().group() == self.group
                && outcome.scope().recovery() == self.recovery
        }))
    }

    fn record(
        &self,
        tenant: &TenantId,
        partition: &PartitionId,
        id: &RecordId,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.step("record")?;
        self.store
            .records
            .iter()
            .find(|owner| {
                owner.tenant == *tenant && owner.partition == *partition && owner.record.id() == id
            })
            .map(|owner| self.copy(owner))
            .transpose()
    }

    fn parallel_member(
        &self,
        tenant: &TenantId,
        due: DueOrder<'_>,
    ) -> Result<bool, StateTransactionError> {
        self.step("parallel")?;
        Ok(tenant == self.store.scope.tenant()
            && self.store.rows.contains(&due)
            && (due.workflow_type != b"r" || due.state != b"q"))
    }

    fn fifo_member(
        &self,
        tenant: &TenantId,
        lane: LaneKey<'_>,
        member: LaneOrder<'_>,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.step("fifo-member")?;
        self.store
            .records
            .iter()
            .find(|owner| {
                let entry = entry(owner);
                owner.tenant == *tenant
                    && LaneKey::for_record(&entry) == Some(lane)
                    && LaneOrder::for_record(&entry) == member
            })
            .map(|owner| self.copy(owner))
            .transpose()
    }

    fn fifo_head(
        &self,
        tenant: &TenantId,
        lane: LaneKey<'_>,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.step("fifo-head")?;
        self.store
            .records
            .iter()
            .filter(|owner| {
                owner.tenant == *tenant && LaneKey::for_record(&entry(owner)) == Some(lane)
            })
            .min_by_key(|owner| LaneOrder::for_record(&entry(owner)))
            .map(|owner| self.copy(owner))
            .transpose()
    }

    fn due_cursor<'tx>(
        &'tx self,
        tenant: &'tx TenantId,
        query: ClaimQuery<'tx>,
        after: Option<DueOrder<'tx>>,
        maximum_rows: usize,
        maximum_bytes: usize,
    ) -> Result<RecordingDueCursor<'tx, 'store, 'input>, StateTransactionError> {
        self.step("open")?;
        if tenant != self.store.scope.tenant() {
            return Err(StateTransactionError::Corrupt);
        }
        if maximum_rows == 0 || maximum_bytes == 0 {
            return Err(StateTransactionError::Limit);
        }
        // Finite double's seek position; physical ordered-key design is deferred.
        let index = self
            .store
            .rows
            .partition_point(|row| after.is_some_and(|last| *row <= last));
        Ok(RecordingDueCursor {
            transaction: self,
            query,
            index,
            position: None,
            maximum_rows,
            maximum_bytes,
            returned: 0,
            bytes: 0,
            terminal: None,
        })
    }

    fn commit(self, changes: &GroupApplyChanges<'_, '_>) -> Result<(), StateTransactionError> {
        if changes.scope().group() != self.group
            || changes.scope().recovery() != self.recovery
            || changes.expected() != self.store.control
        {
            return Err(StateTransactionError::KnownAborted);
        }
        self.store.calls.borrow_mut().push("commit");
        self.store.committed.set(true);
        Ok(())
    }
}

impl GroupDueCursor for RecordingDueCursor<'_, '_, '_> {
    fn next(&mut self) -> Result<DueCursorStep<'_>, StateTransactionError> {
        if let Some(range_end) = self.terminal {
            return Ok(if range_end {
                DueCursorStep::RangeEnd
            } else {
                DueCursorStep::BudgetEnd
            });
        }
        loop {
            if self.returned == self.maximum_rows || self.transaction.step("next").is_err() {
                self.terminal = Some(false);
                return Ok(DueCursorStep::BudgetEnd);
            }
            let Some(row) = self.transaction.store.rows.get(self.index) else {
                self.terminal = Some(true);
                return Ok(DueCursorStep::RangeEnd);
            };
            let bytes = 128
                + row.id.len()
                + row.partition_key.len()
                + row.state.len()
                + row.workflow_type.len();
            if self
                .bytes
                .checked_add(bytes)
                .is_none_or(|next| next > self.maximum_bytes)
                || self.transaction.admit(bytes).is_err()
            {
                self.terminal = Some(false);
                return Ok(DueCursorStep::BudgetEnd);
            }
            self.bytes += bytes;
            self.index += 1;
            self.position = Some(*row);
            if matches_query(*row, self.query) {
                self.returned += 1;
                return Ok(DueCursorStep::Row(*row));
            }
        }
    }

    fn position(&self) -> Option<DueOrder<'_>> {
        self.position
    }
}

fn matches_query(row: DueOrder<'_>, query: ClaimQuery<'_>) -> bool {
    row.workflow_type == query.workflow_type
        && query
            .priority
            .is_none_or(|priority| row.priority == priority)
        && match query.partitions {
            PartitionFilter::Any => true,
            PartitionFilter::Only(values) => values.contains(&row.partition_key),
        }
        && match query.states {
            ClaimStates::Any => true,
            ClaimStates::Only(values) => values.contains(&row.state),
        }
        && !query.exclude_states.contains(&row.state)
}

fn entry(owner: &StoredWorkflowRecord) -> ClaimRecord<'_> {
    ClaimRecord {
        record: &owner.record,
        partition_key: owner.partition.as_bytes(),
        priority: owner.ordering.priority,
        state_enter_seq: owner.ordering.state_enter_seq,
    }
}

fn record_bytes(owner: &StoredWorkflowRecord) -> usize {
    let record = &owner.record;
    512 + owner.tenant.as_bytes().len()
        + owner.partition.as_bytes().len()
        + record.id().as_bytes().len()
        + record.workflow_type().len()
        + record.state().len()
        + record.run_state().map_or(0, <[u8]>::len)
        + record
            .lease()
            .map_or(0, |lease| lease.worker.len() + lease.token.as_bytes().len())
        + record.payload().map_or(0, <[u8]>::len)
        + record.result().map_or(0, <[u8]>::len)
        + record.error().map_or(0, <[u8]>::len)
}

fn query() -> ClaimQuery<'static> {
    ClaimQuery {
        workflow_type: b"r",
        partitions: PartitionFilter::Any,
        states: ClaimStates::Any,
        exclude_states: &[],
        priority: None,
        worker: b"worker",
        lease_ms: 10,
        limit: 10,
    }
}

pub(super) fn commit_after_observations(changes: &GroupApplyChanges<'_, '_>) -> TestResult {
    let owners = [sample_record(b"b", b"r", b"q")?];
    let rows = [DueOrder::for_record(&entry(&owners[0])).ok_or("due absent")?];
    let mut store = RecordingGroupStore {
        scope: changes.scope(),
        control: changes.expected(),
        records: &owners,
        outcomes: &[],
        rows: &rows,
        calls: RefCell::new(Vec::new()),
        committed: Cell::new(false),
    };
    let tx = store.begin_apply(
        changes.scope().group(),
        changes.scope().recovery(),
        10,
        10_000,
        10_000,
    )?;
    {
        let mut cursor = tx.due_cursor(changes.scope().tenant(), query(), None, 1, 1_000)?;
        assert!(matches!(cursor.next()?, DueCursorStep::Row(_)));
        assert_eq!(tx.control()?, changes.expected());
    }
    tx.commit(changes)?;
    assert!(store.committed.get());
    assert_eq!(
        *store.calls.borrow(),
        ["begin", "open", "next", "control", "commit"]
    );
    assert_retained_probe(changes)?;
    Ok(())
}

#[test]
fn same_transaction_binds_points_heads_and_range_budget() -> TestResult {
    let scope = scope()?;
    let owners = [
        sample_record(b"a", b"skip", b"q")?,
        sample_record(b"b", b"r", b"q")?,
        sample_record(b"c", b"r", b"z")?,
    ];
    let rows: Vec<_> = owners
        .iter()
        .map(|owner| DueOrder::for_record(&entry(owner)).ok_or("due absent"))
        .collect::<Result<_, _>>()?;
    let mut store = RecordingGroupStore {
        scope: &scope,
        control: ControlState::initial(clock(0)?),
        records: &owners,
        outcomes: &[],
        rows: &rows,
        calls: RefCell::new(Vec::new()),
        committed: Cell::new(false),
    };
    {
        let tx = store.begin_apply(scope.group(), scope.recovery(), 20, 10_000, 10_000)?;
        assert_eq!(tx.control()?, ControlState::initial(clock(0)?));
        {
            let mut cursor = tx.due_cursor(scope.tenant(), query(), None, 1, 1_000)?;
            let DueCursorStep::Row(row) = cursor.next()? else {
                return Err("expected b".into());
            };
            assert_eq!(row.id, b"b");
            assert_eq!(cursor.position(), Some(rows[1]));
            assert_eq!(
                tx.record(scope.tenant(), &owners[1].partition, owners[1].record.id())?,
                Some(owners[1].clone())
            );
            assert!(matches!(cursor.next()?, DueCursorStep::BudgetEnd));
            let spent = tx.steps.get();
            assert!(matches!(cursor.next()?, DueCursorStep::BudgetEnd));
            assert_eq!(tx.steps.get(), spent);
        }
        {
            let mut resumed = tx.due_cursor(scope.tenant(), query(), Some(rows[1]), 2, 1_000)?;
            assert!(matches!(resumed.next()?, DueCursorStep::Row(row) if row.id == b"c"));
            assert!(matches!(resumed.next()?, DueCursorStep::RangeEnd));
            assert!(matches!(resumed.next()?, DueCursorStep::RangeEnd));
        }
        assert_eq!(
            tx.record(
                scope.tenant(),
                &owners[0].partition,
                &RecordId::new(b"missing".to_vec())?
            )?,
            None
        );
        assert_eq!(
            tx.outcome(scope.tenant(), &RequestId::new(b"missing".to_vec())?)?,
            None
        );
        assert!(tx.parallel_member(scope.tenant(), rows[0])?);
        assert!(!tx.parallel_member(scope.tenant(), rows[1])?);
        let lane = LaneKey::for_record(&entry(&owners[1])).ok_or("lane absent")?;
        assert_eq!(
            tx.fifo_member(
                scope.tenant(),
                lane,
                LaneOrder::for_record(&entry(&owners[1]))
            )?,
            Some(owners[1].clone())
        );
        assert_eq!(tx.fifo_head(scope.tenant(), lane)?, Some(owners[1].clone()));
        assert_eq!(tx.steps.get(), 14);
        assert_eq!(tx.bytes.get(), 512 + 135 + 132 + 132 + 3 * 517);
    }
    assert_filtered_continuation(&mut store, &scope, &rows)?;
    {
        let tx = store.begin_apply(scope.group(), scope.recovery(), 2, 516, 10_000)?;
        assert_eq!(
            tx.record(scope.tenant(), &owners[1].partition, owners[1].record.id()),
            Err(StateTransactionError::Limit)
        );
        assert_eq!(tx.steps.get(), 1);
        assert_eq!(tx.bytes.get(), 0);
    }
    assert!(!store.committed.get());
    Ok(())
}

fn assert_filtered_continuation(
    store: &mut RecordingGroupStore<'_>,
    scope: &RequestScope,
    rows: &[DueOrder<'_>],
) -> TestResult {
    {
        let tx = store.begin_apply(scope.group(), scope.recovery(), 2, 1, 10_000)?;
        assert!(!tx.retained_nonempty(scope.tenant())?);
        assert_eq!((tx.steps.get(), tx.bytes.get()), (2, 0));
    }
    {
        let tx = store.begin_apply(scope.group(), scope.recovery(), 2, 10_000, 10_000)?;
        let mut cursor = tx.due_cursor(scope.tenant(), query(), None, 5, 1_000)?;
        assert!(matches!(cursor.next()?, DueCursorStep::BudgetEnd));
        assert_eq!(cursor.position(), Some(rows[0]));
        assert_eq!(tx.steps.get(), 2);
        assert_eq!(tx.bytes.get(), 135);
    }
    {
        let tx = store.begin_apply(scope.group(), scope.recovery(), 3, 10_000, 10_000)?;
        let mut resumed = tx.due_cursor(scope.tenant(), query(), Some(rows[0]), 5, 1_000)?;
        assert!(matches!(resumed.next()?, DueCursorStep::Row(row) if row.id == b"b"));
    }
    let tx = store.begin_apply(scope.group(), scope.recovery(), 10, 10_000, 10_000)?;
    let mut cursor = tx.due_cursor(scope.tenant(), query(), None, 5, 266)?;
    assert!(matches!(cursor.next()?, DueCursorStep::BudgetEnd));
    assert_eq!(cursor.position(), Some(rows[0]));
    assert_eq!(tx.bytes.get(), 135);
    assert_eq!(tx.steps.get(), 3);
    Ok(())
}

fn assert_retained_probe(changes: &GroupApplyChanges<'_, '_>) -> TestResult {
    let outcome = changes.outcome().ok_or("fresh outcome absent")?;
    let mut store = RecordingGroupStore {
        scope: changes.scope(),
        control: changes.plan().control().next(),
        records: &[],
        outcomes: std::slice::from_ref(outcome),
        rows: &[],
        calls: RefCell::new(Vec::new()),
        committed: Cell::new(false),
    };
    {
        let tx = store.begin_apply(
            changes.scope().group(),
            changes.scope().recovery(),
            1,
            1,
            10_000,
        )?;
        assert_eq!(
            tx.retained_nonempty(changes.scope().tenant()),
            Err(StateTransactionError::Limit)
        );
        assert_eq!((tx.steps.get(), tx.bytes.get()), (1, 0));
        assert_eq!(&*tx.store.calls.borrow(), &["begin", "retained-open"]);
    }
    {
        let tx = store.begin_apply(
            changes.scope().group(),
            changes.scope().recovery(),
            4,
            1_035,
            10_000,
        )?;
        assert_eq!(tx.control()?, changes.plan().control().next());
        assert!(tx.retained_nonempty(changes.scope().tenant())?);
        assert_eq!(
            tx.outcome(changes.scope().tenant(), outcome.request())?,
            Some(outcome.clone())
        );
        assert_eq!((tx.steps.get(), tx.bytes.get()), (4, 1_035));
    }
    Ok(())
}
