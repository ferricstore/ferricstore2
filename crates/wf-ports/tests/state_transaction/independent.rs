//! Mathematical Changes coherence; no commit, durability or public-I/O claim.

use std::collections::BTreeMap;

use wf_domain::claim::{Claim, ClaimRecord, DueOrder, LaneKey, ScopedRecordId};
use wf_domain::history::EventKind;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::{ApplyError, BatchRejection};
use wf_domain::workflow::control::{ControlState, EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::indexed::{
    IndependentIndexedDecision, IndependentIndexedPlan, IndexRejection, IndexRows,
    TouchedIndexedGroupView,
};
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedStatus};
use wf_domain::workflow::{
    Cancel, CommittedCommand, DomainError, Operation, Outcome, Terminal, reduce,
};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{
    CodecError, CodecLimits, RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchApplyPlan,
    RecordBatchInspection, decide_fresh_independent_record_batch_touched,
    decide_fresh_record_batch_touched, encode_independent_record_batch, encode_record_batch,
    independent_record_batch_response_matches, inspect_independent_record_batch_point,
    inspect_record_batch_point, prepare_independent_record_batch, prepare_record_batch,
    record_batch_retained_summary,
};
use wf_ports::state::{GroupApplyChanges, StateTransactionError, StoredWorkflowRecord};
use wf_types::identity::{GroupId, PartitionId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, Fence, LogIdentity, TimestampMillis};

use super::{TestResult, clock, sample_record, scope};

const WORKSPACE: usize = 64 * 1_024 * 1_024;
const RESPONSE: [u8; 12] = [2, 0, 0, 0, 5, 0, 1, 0, 4, 0, 0, 0];

fn limits() -> CodecLimits {
    CodecLimits::new(AdmissionLimits::default())
}

fn previous() -> TestResult<ControlState> {
    Ok(ControlState::restored(
        clock(1_000)?,
        EntryIdentity::new(
            LogIdentity::new(1, 7, AppliedPosition::new(1)),
            PreparedDigest::new([7; 32]),
        ),
        EntryStamp::Stamped(clock(1_000)?),
    )?)
}

fn originals() -> TestResult<Vec<StoredWorkflowRecord>> {
    let mut rows = Vec::new();
    for (partition, id) in [(b"q", b"b"), (b"p", b"a")] {
        let mut row = sample_record(id, b"r", b"q")?;
        row.partition = PartitionId::new(partition.to_vec())?;
        let claim = CommittedCommand {
            id: row.record.id().clone(),
            now: TimestampMillis::new(1_000),
            operation: Operation::Claim(Claim {
                expected_state: b"q".to_vec(),
                worker: b"w".to_vec(),
                token: LeaseToken::new(b"k".to_vec())?,
                lease_ms: 30_000,
            }),
        };
        row.record = reduce(Some(&row.record), &claim)?
            .into_parts()
            .1
            .ok_or("claim absent")?
            .record;
        row.ordering.state_enter_seq = 99;
        assert_eq!(row.record.version(), 2);
        assert_eq!(row.record.fence(), Fence::new(1));
        rows.push(row);
    }
    Ok(rows)
}

fn terminal(token: &[u8]) -> TestResult<Operation> {
    Ok(Operation::Complete(Terminal {
        guard: LeaseGuard {
            token: LeaseToken::new(token.to_vec())?,
            fence: Fence::new(1),
        },
        payload: None,
        value: None,
    }))
}

fn item(row: &StoredWorkflowRecord, operation: Operation) -> PreparedBatchItem {
    PreparedBatchItem {
        partition: row.partition.clone(),
        command: CommittedCommand {
            id: row.record.id().clone(),
            now: TimestampMillis::new(2_000),
            operation,
        },
        create_priority: 0,
    }
}

fn request(scope: &RequestScope, rows: &[StoredWorkflowRecord]) -> TestResult<PreparedRequest> {
    let retry = Operation::Retry(Retry {
        guard: LeaseGuard {
            token: LeaseToken::new(b"k".to_vec())?,
            fence: Fence::new(1),
        },
        run_at: RetryTime::Explicit(TimestampMillis::new(3_000)),
        payload: None,
        error: None,
    });
    let cancel = Operation::Cancel(Cancel {
        fence: Fence::new(1),
        lease_token: None,
        expected_state: None,
        expected_version: None,
        error: None,
    });
    Ok(PreparedRequest {
        scope: scope.clone(),
        request: RequestId::new(b"independent".to_vec())?,
        intent: ClientIntent::new(1, [0; 32])?,
        stamp: clock(2_000)?,
        operation: PreparedOperation::Batch(vec![
            item(&rows[0], retry),
            item(&rows[1], terminal(b"wrong")?),
            item(&rows[1], terminal(b"k")?),
            item(&rows[0], cancel),
            item(&rows[1], terminal(b"k")?),
        ]),
    })
}

fn with_observations<'current, T>(
    scope: &RequestScope,
    rows: &'current [StoredWorkflowRecord],
    inspect: impl FnOnce(TouchedIndexedGroupView<'_, 'current>) -> TestResult<T>,
) -> TestResult<T> {
    let placements = rows
        .iter()
        .map(|row| (row.partition.clone(), scope.group().clone()))
        .collect();
    let entries: Vec<_> = rows
        .iter()
        .map(|row| ClaimRecord {
            record: &row.record,
            partition_key: row.partition.as_bytes(),
            priority: row.ordering.priority,
            state_enter_seq: row.ordering.state_enter_seq,
        })
        .collect();
    let current = entries
        .iter()
        .map(|entry| (ScopedRecordId::for_record(entry), Some(*entry)))
        .collect();
    let parallel = entries
        .iter()
        .filter_map(|entry| DueOrder::for_record(entry).map(|due| (due, true)))
        .collect();
    let heads = entries
        .iter()
        .filter_map(|entry| LaneKey::for_record(entry).map(|lane| (lane, None)))
        .collect();
    let members = BTreeMap::new();
    let policies = BTreeMap::from([(b"r".as_slice(), None)]);
    let retries = BTreeMap::from([(b"r".as_slice(), None)]);
    inspect(TouchedIndexedGroupView {
        group: scope.group(),
        placements: &placements,
        current: &current,
        parallel: &parallel,
        fifo_members: &members,
        fifo_heads: &heads,
        policies: &policies,
        retry_policies: &retries,
    })
}

fn planned<'current>(
    scope: &RequestScope,
    rows: &'current [StoredWorkflowRecord],
    independent_wire: bool,
    independent_fresh: bool,
    inspection_limits: CodecLimits,
) -> TestResult<Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError>> {
    let request = request(scope, rows)?;
    let frame = if independent_wire {
        encode_independent_record_batch(request, limits(), WORKSPACE)?
    } else {
        encode_record_batch(request, limits(), WORKSPACE)?
    };
    let decoded = if independent_wire {
        prepare_independent_record_batch(frame.as_bytes(), limits(), WORKSPACE)?
    } else {
        prepare_record_batch(frame.as_bytes(), limits(), WORKSPACE)?
    };
    let view = PointGateView {
        scope,
        control: previous()?,
        outcome: None,
        retained_nonempty: false,
    };
    let log = LogIdentity::new(1, 7, AppliedPosition::new(2));
    let inspection = if independent_wire {
        inspect_independent_record_batch_point(decoded, log, view, inspection_limits, WORKSPACE)
    } else {
        inspect_record_batch_point(decoded, log, view, inspection_limits, WORKSPACE)
    };
    let fresh = match inspection {
        Ok(RecordBatchInspection::Fresh(fresh)) => fresh,
        Ok(RecordBatchInspection::Complete(_)) => return Err("expected Fresh".into()),
        Err(error) => return Ok(Err(error)),
    };
    with_observations(scope, rows, |view| {
        Ok(if independent_fresh {
            decide_fresh_independent_record_batch_touched(fresh, view, 0, 1_000)
        } else {
            decide_fresh_record_batch_touched(fresh, view, 0, 1_000)
        })
    })
}

fn assert_response(plan: &RecordBatchApplyPlan<'_>) -> TestResult {
    let RecordBatchApplyDecision::IndependentFresh { operation, outcome } = plan.decision() else {
        return Err("independent Fresh absent".into());
    };
    assert_eq!(outcome.intent().version(), 6);
    assert_eq!(outcome.response_version(), 1);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.response(), RESPONSE);
    assert_eq!(
        record_batch_retained_summary(outcome.intent(), 1, &RESPONSE)?,
        (RetainedStatus::Accepted, Some(5))
    );
    assert!(independent_record_batch_response_matches(
        operation, &RESPONSE
    )?);
    let all_ok = [2, 0, 0, 0, 5, 0, 0, 0, 0, 0];
    assert_eq!(
        record_batch_retained_summary(outcome.intent(), 1, &all_ok)?,
        (RetainedStatus::Accepted, Some(5))
    );
    assert!(!independent_record_batch_response_matches(
        operation, &all_ok
    )?);
    let mut wrong_business = RESPONSE;
    wrong_business[8] = 3;
    assert!(!independent_record_batch_response_matches(
        operation,
        &wrong_business
    )?);
    let four = [2, 0, 0, 0, 4, 0, 1, 0, 4, 0, 0];
    assert!(!independent_record_batch_response_matches(
        operation, &four
    )?);
    let mut trailing = RESPONSE.to_vec();
    trailing.push(0);
    assert!(independent_record_batch_response_matches(operation, &trailing).is_err());
    for malformed in [b"".as_slice(), &[2, 0, 0, 0, 0], &[2, 0, 0, 0, 1, 1, 1, 0]] {
        assert!(independent_record_batch_response_matches(operation, malformed).is_err());
    }
    Ok(())
}

fn assert_publication(
    business: &IndependentIndexedPlan<'_>,
    rows: &[StoredWorkflowRecord],
) -> TestResult {
    assert_eq!(business.final_indices().collect::<Vec<_>>(), [2, 3]);
    let originals = business.originals().collect::<Vec<_>>();
    for (index, source) in [1, 0].into_iter().enumerate() {
        let original = originals[index].ok_or("original absent")?;
        assert!(std::ptr::eq(
            original.record,
            &raw const rows[source].record
        ));
        assert_eq!(original.record.version(), 2);
        assert_eq!(original.state_enter_seq, 99);
    }
    assert_eq!(business.consumed_ordinals(), 0);
    assert_eq!(business.next_ordinal(), Some(0));
    assert_eq!(
        business
            .metadata()
            .map(|value| value.state_enter_seq)
            .collect::<Vec<_>>(),
        [99, 99]
    );
    let results = business.results();
    assert_eq!(results.len(), 5);
    assert_eq!(
        results[1],
        Err(IndexRejection::Batch(BatchRejection::Domain(
            DomainError::StaleLease
        )))
    );
    assert_eq!(
        results[4].as_ref().map(|value| value.plan().outcome()),
        Ok(Outcome::LegacyNoop)
    );
    assert!(
        results[4]
            .as_ref()
            .map_err(|_| "noop rejected")?
            .plan()
            .history()
            .is_none()
    );
    for (index, expected, final_version) in [(0, 2, 3), (2, 2, 3), (3, 3, 4)] {
        let write = results[index]
            .as_ref()
            .map_err(|_| "write rejected")?
            .plan()
            .write()
            .ok_or("write absent")?;
        assert_eq!(write.expected_version, Some(expected));
        assert_eq!(write.record.version(), final_version);
    }
    let histories = business
        .history_plans()
        .map(|value| {
            let history = value.plan().history().ok_or("history absent")?;
            Ok((
                value.partition().as_bytes(),
                history.kind,
                history.version,
                history.event_id.as_str(),
            ))
        })
        .collect::<TestResult<Vec<_>>>()?;
    assert_eq!(
        histories,
        [
            (b"q".as_slice(), EventKind::Retry, 3, "2000-3"),
            (b"p".as_slice(), EventKind::Completed, 3, "2000-3"),
            (b"q".as_slice(), EventKind::Cancelled, 4, "2000-4"),
        ]
    );
    let deltas = business.deltas().collect::<Result<Vec<_>, _>>()?;
    assert_eq!(deltas.len(), 2);
    for delta in deltas {
        let Some(IndexRows::Parallel { due }) = delta.before else {
            return Err("original parallel row absent".into());
        };
        assert_eq!(due.due, TimestampMillis::new(31_000));
        assert_eq!(delta.after, None);
    }
    Ok(())
}

fn assert_invalid_changes(scope: &RequestScope, rows: &[StoredWorkflowRecord]) -> TestResult {
    let foreign = [
        RequestScope::new(
            TenantId::new(b"other".to_vec())?,
            scope.group().clone(),
            scope.recovery().clone(),
        ),
        RequestScope::new(
            scope.tenant().clone(),
            GroupId::new(b"other".to_vec())?,
            scope.recovery().clone(),
        ),
        RequestScope::new(
            scope.tenant().clone(),
            scope.group().clone(),
            RecoveryId::new(b"other".to_vec())?,
        ),
    ];
    for foreign_scope in &foreign {
        let plan = planned(scope, rows, true, true, limits())??;
        assert!(matches!(
            GroupApplyChanges::new(foreign_scope, previous()?, plan),
            Err(StateTransactionError::InvalidChanges)
        ));
    }
    for applied in [false, true] {
        let plan = planned(scope, rows, true, true, limits())??;
        let wrong = if applied {
            plan.control().next()
        } else {
            ControlState::initial(clock(0)?)
        };
        assert!(matches!(
            GroupApplyChanges::new(scope, wrong, plan),
            Err(StateTransactionError::InvalidChanges)
        ));
    }
    Ok(())
}

fn assert_profiles(scope: &RequestScope, rows: &[StoredWorkflowRecord]) -> TestResult {
    for (wire, fresh) in [(true, false), (false, true)] {
        assert!(matches!(
            planned(scope, rows, wire, fresh, limits())?,
            Err(RecordBatchApplyError::Encoding(
                CodecError::UnsupportedVersion
            ))
        ));
    }
    let other = CodecLimits::new(AdmissionLimits::new(256, 16 * 1_024 * 1_024, 1_000)?);
    assert!(matches!(
        planned(scope, rows, true, true, other)?,
        Err(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority))
    ));
    Ok(())
}

#[test]
fn changes_bind_independent_rows() -> TestResult {
    let scope = scope()?;
    let rows = originals()?;
    assert_invalid_changes(&scope, &rows)?;
    assert_profiles(&scope, &rows)?;
    let plan = planned(&scope, &rows, true, true, limits())??;
    assert_response(&plan)?;
    let next = plan.control().next();
    let changes = GroupApplyChanges::new(&scope, previous()?, plan)?;
    assert_eq!(changes.scope(), &scope);
    assert_eq!(changes.expected(), previous()?);
    assert_eq!(changes.control(), Some(next));
    assert_eq!(next.position(), AppliedPosition::new(2));
    assert!(changes.business().is_none());
    assert_publication(
        changes.independent_business().ok_or("business absent")?,
        &rows,
    )?;
    assert_eq!(
        changes.outcome().ok_or("outcome absent")?.response(),
        RESPONSE
    );
    let (_, decision) = changes.into_plan().into_parts();
    let RecordBatchApplyDecision::IndependentFresh {
        operation: IndependentIndexedDecision::Accepted(business),
        outcome,
    } = decision
    else {
        return Err("accepted independent owner absent".into());
    };
    let results = business.into_results();
    drop(rows);
    drop(scope);
    let final_write = results[3]
        .as_ref()
        .map_err(|_| "final rejected")?
        .plan()
        .write()
        .ok_or("final absent")?;
    assert_eq!(final_write.expected_version, Some(3));
    assert_eq!(final_write.record.version(), 4);
    assert_eq!(final_write.record.state(), b"cancelled");
    assert_eq!(outcome.response(), RESPONSE);
    assert_eq!(outcome.original_time(), TimestampMillis::new(2_000));
    Ok(())
}
