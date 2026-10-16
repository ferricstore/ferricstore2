use super::*;
use wf_domain::claim::ClaimRecord;
use wf_domain::history::EventKind;
use wf_domain::workflow::Cancel;
use wf_domain::workflow::indexed::OrderingMetadata;
use wf_types::version::Fence;

fn entry(owner: &StoredWorkflowRecord) -> ClaimRecord<'_> {
    ClaimRecord {
        record: &owner.record,
        partition_key: owner.partition.as_bytes(),
        priority: owner.ordering.priority,
        state_enter_seq: owner.ordering.state_enter_seq,
    }
}

impl GroupApplyTransaction for ObservedTransaction<'_> {
    type DueCursor<'tx>
        = ObservedCursor
    where
        Self: 'tx;

    fn control(&self) -> Result<ControlState, StateTransactionError> {
        self.store.observe("control")?;
        Ok(self.store.control)
    }

    fn retained_nonempty(&self, tenant: &TenantId) -> Result<bool, StateTransactionError> {
        self.store.observe("retained")?;
        if tenant != self.store.scope.tenant() {
            return Err(StateTransactionError::Corrupt);
        }
        Ok(self.store.retained_present)
    }

    fn outcome(
        &self,
        tenant: &TenantId,
        request: &RequestId,
    ) -> Result<Option<RetainedOutcome>, StateTransactionError> {
        self.store.observe("outcome")?;
        if tenant != self.store.scope.tenant() || request.as_bytes() != b"q" {
            return Err(StateTransactionError::Corrupt);
        }
        // Transfer the actual fixture owner so pointer tests detect driver clones.
        Ok(self.store.retained.borrow_mut().take())
    }

    fn placement(&self, partition: &PartitionId) -> Result<Option<GroupId>, StateTransactionError> {
        self.store.observe("placement")?;
        if partition.as_bytes() != b"p" {
            return Err(StateTransactionError::Corrupt);
        }
        Ok(self.store.placement.clone())
    }

    fn installed_policy(
        &self,
        tenant: &TenantId,
        workflow_type: &[u8],
        limits: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<CompletePolicyValue>, StateTransactionError> {
        self.store.observe("policy")?;
        self.store
            .queried_types
            .borrow_mut()
            .push(workflow_type.to_vec());
        if tenant != self.store.scope.tenant() {
            return Err(StateTransactionError::Corrupt);
        }
        self.store
            .policy_bytes
            .iter()
            .find(|(kind, _)| kind == workflow_type)
            .map(|(_, bytes)| {
                let value = decode_complete_policy_value_v2(bytes, limits, charge)
                    .map_err(|_| StateTransactionError::Corrupt)?;
                if value.data().workflow_type != workflow_type {
                    return Err(StateTransactionError::Corrupt);
                }
                Ok(value)
            })
            .transpose()
    }

    fn record(
        &self,
        tenant: &TenantId,
        partition: &PartitionId,
        id: &RecordId,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.store.observe("record")?;
        Ok(self
            .store
            .records
            .iter()
            .find(|row| {
                row.tenant == *tenant && row.partition == *partition && row.record.id() == id
            })
            .cloned())
    }

    fn parallel_member(
        &self,
        tenant: &TenantId,
        due: DueOrder<'_>,
    ) -> Result<bool, StateTransactionError> {
        self.store.observe("parallel")?;
        Ok(self.store.parallel
            && self
                .store
                .records
                .iter()
                .any(|row| row.tenant == *tenant && DueOrder::for_record(&entry(row)) == Some(due)))
    }

    fn fifo_member(
        &self,
        tenant: &TenantId,
        lane: LaneKey<'_>,
        member: LaneOrder<'_>,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.store.observe("member")?;
        Ok(self
            .store
            .records
            .iter()
            .find(|row| {
                row.tenant == *tenant
                    && LaneKey::for_record(&entry(row)) == Some(lane)
                    && LaneOrder::for_record(&entry(row)) == member
            })
            .cloned())
    }

    fn fifo_head(
        &self,
        tenant: &TenantId,
        lane: LaneKey<'_>,
    ) -> Result<Option<StoredWorkflowRecord>, StateTransactionError> {
        self.store.observe("head")?;
        if self.store.parallel {
            return Ok(None);
        }
        Ok(self
            .store
            .records
            .iter()
            .filter(|row| row.tenant == *tenant && LaneKey::for_record(&entry(row)) == Some(lane))
            .min_by_key(|row| LaneOrder::for_record(&entry(row)))
            .cloned())
    }

    fn due_cursor<'tx>(
        &'tx self,
        _tenant: &'tx TenantId,
        _query: ClaimQuery<'tx>,
        _after: Option<DueOrder<'tx>>,
        _maximum_rows: usize,
        _maximum_bytes: usize,
    ) -> Result<Self::DueCursor<'tx>, StateTransactionError> {
        self.store.observe("cursor")?;
        Err(StateTransactionError::Unavailable)
    }

    fn commit(self, changes: &GroupApplyChanges<'_, '_>) -> Result<(), StateTransactionError> {
        self.store.observe("commit")?;
        if changes.expected() != self.store.control
            || changes.scope().group() != self.group
            || changes.scope().recovery() != self.recovery
        {
            return Err(StateTransactionError::KnownAborted);
        }
        self.store.next_control = changes.control();
        self.store.written_outcome = changes.outcome().cloned();
        if let Some(business) = changes.business() {
            for item in business.batch().items() {
                self.store.writes.extend(item.plan().write().cloned());
                self.store.histories.extend(item.plan().history().cloned());
            }
        }
        independent::record_publication(self.store, changes)?;
        self.store.committed = true;
        Ok(())
    }
}

fn sample_record(id: &[u8], sequence: u128) -> TestResult<StoredWorkflowRecord> {
    let command = CommittedCommand {
        id: RecordId::new(id.to_vec())?,
        now: TimestampMillis::new(1),
        operation: create(b"r"),
    };
    let record = reduce(None, &command)?
        .into_parts()
        .1
        .ok_or("write absent")?
        .record;
    Ok(StoredWorkflowRecord {
        tenant: scope()?.tenant().clone(),
        partition: PartitionId::new(b"p".to_vec())?,
        record,
        ordering: OrderingMetadata {
            priority: 0,
            state_enter_seq: sequence,
        },
    })
}

fn cancel() -> Operation {
    Operation::Cancel(Cancel {
        fence: Fence::new(0),
        lease_token: None,
        expected_state: Some(b"q".to_vec()),
        expected_version: Some(1),
        error: Some(b"cancelled".to_vec()),
    })
}

fn assert_retained(result: RecordBatchTransactionOutcome) -> TestResult<Box<RetainedOutcome>> {
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("retained outcome absent".into());
    };
    Ok(outcome)
}

fn check_create() -> TestResult {
    let mut store = observed_store()?;
    let scope = scope()?;
    let bytes = frame(1, vec![(b"i", create(b"r"))])?;
    assert_eq!(bytes, hex(MIN_FRAME)?);
    let result = apply_record_batch_transaction(&mut store, &scope, &bytes, log(0), limits())?;
    assert!(store.committed);
    assert_eq!(
        &store.calls.borrow()[..4],
        ["begin", "control", "retained", "outcome"]
    );
    assert_eq!(store.calls.borrow().last(), Some(&"commit"));
    assert_eq!(store.writes.len(), 1);
    assert_eq!(store.writes[0].expected_version, None);
    assert_eq!(store.writes[0].record.state(), b"q");
    assert_eq!(store.writes[0].record.version(), 1);
    assert_eq!(store.histories.len(), 1);
    assert_eq!(store.histories[0].event_id, "1-1");
    assert_eq!(store.histories[0].kind, EventKind::Created);
    assert_eq!(*store.queried_types.borrow(), [b"r".to_vec()]);
    assert_eq!(
        store
            .written_outcome
            .as_ref()
            .ok_or("outcome not committed")?
            .response(),
        [0]
    );
    drop(store);
    drop(scope);
    drop(bytes);
    let outcome = assert_retained(result)?;
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.original_position(), log(0));
    assert_eq!(outcome.original_time(), TimestampMillis::new(1));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(1_001));
    assert_eq!(outcome.response(), [0]);
    Ok(())
}

fn check_existing(fifo: bool) -> TestResult {
    let mut store = observed_store()?;
    store.control = processed()?;
    store.records = vec![sample_record(b"a", 1)?, sample_record(b"i", 3)?];
    store.parallel = !fifo;
    if fifo {
        store.policy_bytes.push((b"r".to_vec(), hex(RICH)?));
        store.failure = Some(("head", StateTransactionError::Corrupt));
    }
    let bytes = frame(2, vec![(b"i", cancel())])?;
    let result = apply_record_batch_transaction(&mut store, &scope()?, &bytes, log(1), limits())?;
    assert_eq!(assert_retained(result)?.status(), RetainedStatus::Accepted);
    assert_eq!(store.writes.len(), 1);
    assert_eq!(store.writes[0].expected_version, Some(1));
    assert_eq!(store.writes[0].record.state(), b"cancelled");
    assert_eq!(store.writes[0].record.version(), 2);
    assert_eq!(store.histories[0].event_id, "2-2");
    assert_eq!(store.histories[0].kind, EventKind::Cancelled);
    assert_eq!(store.calls.borrow().contains(&"member"), fifo);
    assert_eq!(store.calls.borrow().contains(&"head"), !fifo);
    if fifo {
        assert_eq!(store.policy_bytes[0].1, hex(RICH)?);
    }
    Ok(())
}

#[test]
fn record_batch_transaction_loads_facts_and_commits_before_reply() -> TestResult {
    check_create()?;
    check_existing(false)?;
    check_existing(true)?;
    for failure in [
        StateTransactionError::KnownAborted,
        StateTransactionError::Uncertain,
    ] {
        let mut store = observed_store()?;
        store.failure = Some(("commit", failure));
        let result = apply_record_batch_transaction(
            &mut store,
            &scope()?,
            &hex(MIN_FRAME)?,
            log(0),
            limits(),
        );
        assert!(
            matches!(result, Err(RecordBatchTransactionError::Transaction(error)) if error == failure)
        );
        assert!(!store.committed);
        assert!(store.written_outcome.is_none());
        assert_eq!(store.writes, [] as [RecordWrite; 0]);
    }
    Ok(())
}

pub(super) fn retained(result: RecordBatchTransactionOutcome) -> TestResult<Box<RetainedOutcome>> {
    assert_retained(result)
}
