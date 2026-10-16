//! Recording contracts only; the separate real backend proves atomic publication.

use wf_domain::claim::Claim;
use wf_domain::history::EventKind;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::control::{EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::indexed::{IndexRejection, OrderingMetadata};
use wf_domain::workflow::{Cancel, Terminal};
use wf_ports::log::{CodecError, encode_independent_record_batch};
use wf_storage::transaction::apply_independent_record_batch_transaction;
use wf_types::version::Fence;

use super::{
    AdmissionLimits, AppliedPosition, CodecLimits, CommittedCommand, ControlState,
    GroupApplyChanges, HistoryEntry, MIB, ObservedStore, Operation, PartitionId, PreparedBatchItem,
    PreparedOperation, PreparedRequest, RecordBatchApplyError, RecordBatchTransactionError,
    RecordBatchTransactionLimits, RecordBatchTransactionOutcome, RecordId, RecordWrite, RequestId,
    RetainedMetadata, RetainedOutcome, RetainedStatus, StateTransactionError, StoredWorkflowRecord,
    TestResult, TimestampMillis, clock, create, limits, load, log, observed_store, reduce, scope,
};

const MIN_V6: &str = "4657435000010100000001740000000167000000017a00000001710006a5babedcc9c403578eb46ceeee55389f7f05dc3927076e09db38987bab787de1000000000000000100000000000000000001000000010000000170000000016900000000000000000101000000017200000001710000";
const RESPONSE: [u8; 12] = [2, 0, 0, 0, 5, 0, 1, 0, 4, 0, 0, 0];

pub(super) fn record_publication(
    store: &mut ObservedStore,
    changes: &GroupApplyChanges<'_, '_>,
) -> Result<(), StateTransactionError> {
    let Some(business) = changes.independent_business() else {
        return Ok(());
    };
    let writes = business
        .final_indices()
        .map(|index| {
            business
                .results()
                .get(index)
                .and_then(|result| result.as_ref().ok())
                .and_then(|item| item.plan().write())
                .cloned()
                .ok_or(StateTransactionError::InvalidChanges)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let histories = business
        .history_plans()
        .map(|item| {
            item.plan()
                .history()
                .cloned()
                .ok_or(StateTransactionError::InvalidChanges)
        })
        .collect::<Result<Vec<_>, _>>()?;
    store.writes.extend(writes);
    store.histories.extend(histories);
    Ok(())
}

fn previous() -> TestResult<ControlState> {
    Ok(ControlState::restored(
        clock(1_000)?,
        EntryIdentity::new(log(1), PreparedDigest::new([7; 32])),
        EntryStamp::Stamped(clock(1_000)?),
    )?)
}

fn frame(now: u64, entries: Vec<(&[u8], Operation)>) -> TestResult<Vec<u8>> {
    let items = entries
        .into_iter()
        .map(|(id, operation)| {
            Ok(PreparedBatchItem {
                partition: PartitionId::new(b"p".to_vec())?,
                command: CommittedCommand {
                    id: RecordId::new(id.to_vec())?,
                    now: TimestampMillis::new(now),
                    operation,
                },
                create_priority: 0,
            })
        })
        .collect::<TestResult<Vec<_>>>()?;
    let encoded = encode_independent_record_batch(
        PreparedRequest {
            scope: scope()?,
            request: RequestId::new(b"q".to_vec())?,
            intent: super::ClientIntent::new(1, [0; 32])?,
            stamp: clock(now)?,
            operation: PreparedOperation::Batch(items),
        },
        limits().codec,
        limits().workspace_bytes,
    )?;
    Ok(encoded.as_bytes().to_vec())
}

fn complete(token: &[u8]) -> TestResult<Operation> {
    Ok(Operation::Complete(Terminal {
        guard: LeaseGuard {
            token: LeaseToken::new(token.to_vec())?,
            fence: Fence::new(1),
        },
        payload: None,
        value: None,
    }))
}

fn mixed_frame(now: u64) -> TestResult<Vec<u8>> {
    frame(
        now,
        vec![
            (
                b"b",
                Operation::Retry(Retry {
                    guard: LeaseGuard {
                        token: LeaseToken::new(b"k".to_vec())?,
                        fence: Fence::new(1),
                    },
                    run_at: RetryTime::Explicit(TimestampMillis::new(3_000)),
                    payload: None,
                    error: None,
                }),
            ),
            (b"a", complete(b"wrong")?),
            (b"a", complete(b"k")?),
            (
                b"b",
                Operation::Cancel(Cancel {
                    fence: Fence::new(1),
                    lease_token: None,
                    expected_state: None,
                    expected_version: None,
                    error: None,
                }),
            ),
            (b"a", complete(b"k")?),
        ],
    )
}

fn claimed(id: &[u8]) -> TestResult<StoredWorkflowRecord> {
    let id = RecordId::new(id.to_vec())?;
    let initial = reduce(
        None,
        &CommittedCommand {
            id: id.clone(),
            now: TimestampMillis::new(1),
            operation: create(b"r"),
        },
    )?
    .into_parts()
    .1
    .ok_or("create absent")?
    .record;
    let claimed = reduce(
        Some(&initial),
        &CommittedCommand {
            id,
            now: TimestampMillis::new(1_000),
            operation: Operation::Claim(Claim {
                expected_state: b"q".to_vec(),
                worker: b"w".to_vec(),
                token: LeaseToken::new(b"k".to_vec())?,
                lease_ms: 30_000,
            }),
        },
    )?
    .into_parts()
    .1
    .ok_or("claim absent")?
    .record;
    assert_eq!(claimed.version(), 2);
    Ok(StoredWorkflowRecord {
        tenant: scope()?.tenant().clone(),
        partition: PartitionId::new(b"p".to_vec())?,
        record: claimed,
        ordering: OrderingMetadata {
            priority: 0,
            state_enter_seq: 99,
        },
    })
}

fn fresh_store() -> TestResult<ObservedStore> {
    let mut store = observed_store()?;
    store.control = previous()?;
    store.records = vec![claimed(b"b")?, claimed(b"a")?];
    Ok(store)
}

fn apply(
    store: &mut ObservedStore,
    bytes: &[u8],
    index: u64,
    profile: RecordBatchTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, RecordBatchTransactionError> {
    let expected_scope = store.scope.clone();
    apply_independent_record_batch_transaction(store, &expected_scope, bytes, log(index), profile)
}

fn reply(
    result: RecordBatchTransactionOutcome,
) -> TestResult<(ControlState, Box<RetainedOutcome>)> {
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result else {
        return Err("retained reply absent".into());
    };
    Ok((control, outcome))
}

fn assert_no_publication(store: &ObservedStore) {
    assert!(!store.committed);
    assert_eq!(store.writes, [] as [RecordWrite; 0]);
    assert_eq!(store.histories, [] as [HistoryEntry; 0]);
    assert!(store.written_outcome.is_none());
    assert!(store.next_control.is_none());
}

fn assert_fresh(store: &ObservedStore, outcome: &RetainedOutcome) -> TestResult {
    assert!(store.committed);
    assert_eq!(
        &store.calls.borrow()[..4],
        ["begin", "control", "retained", "outcome"]
    );
    assert_eq!(store.calls.borrow().last(), Some(&"commit"));
    assert_eq!(
        store
            .calls
            .borrow()
            .iter()
            .filter(|name| **name == "record")
            .count(),
        2
    );
    assert_eq!(*store.queried_types.borrow(), [b"r".to_vec()]);
    assert_eq!(store.writes.len(), 2);
    assert_eq!(store.writes[0].record.id().as_bytes(), b"a");
    assert_eq!(store.writes[0].expected_version, Some(2));
    assert_eq!(store.writes[0].record.version(), 3);
    assert_eq!(store.writes[0].record.state(), b"completed");
    assert_eq!(store.writes[1].record.id().as_bytes(), b"b");
    assert_eq!(store.writes[1].expected_version, Some(3));
    assert_eq!(store.writes[1].record.version(), 4);
    assert_eq!(store.writes[1].record.state(), b"cancelled");
    assert_eq!(
        store
            .histories
            .iter()
            .map(|event| (event.kind, event.version, event.event_id.as_str()))
            .collect::<Vec<_>>(),
        [
            (EventKind::Retry, 3, "2000-3"),
            (EventKind::Completed, 3, "2000-3"),
            (EventKind::Cancelled, 4, "2000-4"),
        ]
    );
    assert_eq!(
        store.written_outcome.as_ref().ok_or("outcome absent")?,
        outcome
    );
    assert_eq!(outcome.intent().version(), 6);
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.response(), RESPONSE);
    assert_eq!(outcome.original_position(), log(2));
    assert_eq!(outcome.original_time(), TimestampMillis::new(2_000));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(3_000));
    Ok(())
}

fn retained_store(control: ControlState, outcome: RetainedOutcome) -> TestResult<ObservedStore> {
    let mut store = observed_store()?;
    store.control = control;
    store.retained_present = true;
    *store.retained.borrow_mut() = Some(outcome);
    store.failure = Some(("placement", StateTransactionError::Unavailable));
    Ok(store)
}

fn substitute_response(
    original: &RetainedOutcome,
    response: Vec<u8>,
) -> TestResult<RetainedOutcome> {
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope: original.scope().clone(),
            request: original.request().clone(),
            intent: original.intent(),
            original_position: original.original_position(),
            original_time: original.original_time(),
            expires_at: original.expires_at(),
            status: original.status(),
        },
        original.response_version(),
        response,
        AdmissionLimits::default(),
    )?)
}

fn complete_branches(control: ControlState, original: &RetainedOutcome) -> TestResult {
    for kind in ["conflict", "expired", "log"] {
        // Typed fixtures exercise gate ordering. Real byte scanning/failure
        // precedence belongs to the separately selected native oracle.
        let bad = substitute_response(original, vec![0xff])?;
        let mut store = retained_store(control, bad)?;
        let bytes = match kind {
            "conflict" => frame(2_001, vec![(b"a", complete(b"different")?)])?,
            "expired" => mixed_frame(3_000)?,
            _ => mixed_frame(2_000)?,
        };
        let at = if kind == "log" { 2 } else { 3 };
        let result = apply(&mut store, &bytes, at, limits())?;
        assert!(matches!(
            (kind, result),
            ("conflict", RecordBatchTransactionOutcome::IntentConflict(_))
                | ("expired", RecordBatchTransactionOutcome::Expired(_))
                | ("log", RecordBatchTransactionOutcome::LogReplay(_))
        ));
        assert_eq!(
            *store.calls.borrow(),
            ["begin", "control", "retained", "outcome", "commit"]
        );
        assert_eq!(store.writes, [] as [RecordWrite; 0]);
        assert_eq!(store.histories, [] as [HistoryEntry; 0]);
        assert!(store.written_outcome.is_none());
    }
    Ok(())
}

fn count_mismatch(control: ControlState, original: &RetainedOutcome) -> TestResult {
    let bad = substitute_response(original, vec![2, 0, 0, 0, 4, 0, 0, 0, 0])?;
    let mut store = retained_store(control, bad)?;
    assert!(apply(&mut store, &mixed_frame(2_001)?, 3, limits()).is_err());
    assert_eq!(
        *store.calls.borrow(),
        ["begin", "control", "retained", "outcome"]
    );
    assert_no_publication(&store);
    Ok(())
}

fn whole_rejection_replays() -> TestResult {
    let mut profile = limits();
    profile.codec = CodecLimits::new(AdmissionLimits::new(256, 10_000, 5)?);
    let bytes = mixed_frame(2_000)?;
    let mut store = fresh_store()?;
    let (control, outcome) = reply(apply(&mut store, &bytes, 2, profile)?)?;
    assert_eq!(
        outcome.status(),
        RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Admission(
            wf_domain::workflow::admission::AdmissionError::DecisionBytes
        )))
    );
    assert_eq!(outcome.response(), [1, 1, 4]);
    assert_eq!(store.writes, [] as [RecordWrite; 0]);
    assert_eq!(store.histories, [] as [HistoryEntry; 0]);
    let expected = outcome.clone();
    let mut store = retained_store(control, *outcome)?;
    let (_, replayed) = reply(apply(&mut store, &bytes, 3, profile)?)?;
    assert_eq!(replayed, expected);
    assert_eq!(
        *store.calls.borrow(),
        ["begin", "control", "retained", "outcome", "commit"]
    );
    Ok(())
}

#[test]
fn driver_gate_owned_reply() -> TestResult {
    let bytes = mixed_frame(2_000)?;
    let mut store = fresh_store()?;
    let mut profile = limits();
    profile.codec = CodecLimits::new(AdmissionLimits::new(256, 16 * MIB, 5)?);
    let (control, outcome) = reply(apply(&mut store, &bytes, 2, profile)?)?;
    assert_fresh(&store, &outcome)?;
    assert_eq!(store.captured_admission, Some(profile.codec.admission()));
    assert_eq!(control.position(), AppliedPosition::new(2));
    complete_branches(control, &outcome)?;
    count_mismatch(control, &outcome)?;
    let address = outcome.response().as_ptr();
    let expected = outcome.clone();
    let mut store = retained_store(control, *outcome)?;
    let expected_scope = scope()?;
    let replay_bytes = mixed_frame(2_001)?;
    let (next, replayed) = reply(apply_independent_record_batch_transaction(
        &mut store,
        &expected_scope,
        &replay_bytes,
        log(3),
        profile,
    )?)?;
    assert!(store.committed);
    assert_eq!(next.position(), AppliedPosition::new(3));
    assert_eq!(
        *store.calls.borrow(),
        ["begin", "control", "retained", "outcome", "commit"]
    );
    assert!(store.retained.borrow().is_none());
    assert!(store.written_outcome.is_none());
    drop(store);
    drop(expected_scope);
    drop(bytes);
    drop(replay_bytes);
    assert_eq!(replayed, expected);
    assert_eq!(replayed.response().as_ptr(), address);
    assert_eq!(replayed.response(), RESPONSE);
    whole_rejection_replays()
}

fn prospective_limits() -> TestResult {
    let bytes = frame(1, vec![(b"i", create(b"r"))])?;
    assert_eq!(bytes, super::hex(MIN_V6)?);
    let mut profile = limits();
    profile.workspace_bytes = 184_552_907;
    let mut store = observed_store()?;
    assert!(matches!(
        apply(&mut store, &bytes, 0, profile),
        Err(RecordBatchTransactionError::Apply(
            RecordBatchApplyError::Workspace {
                attempted: 16_780_748,
                maximum: 16_780_747,
            }
        ))
    ));
    assert!(store.calls.borrow().is_empty());
    assert_no_publication(&store);
    let mut profile = limits();
    profile.maximum_staging_bytes = 1_023;
    let mut store = observed_store()?;
    assert!(matches!(
        apply(&mut store, &bytes, 0, profile),
        Err(RecordBatchTransactionError::Transaction(
            StateTransactionError::Limit
        ))
    ));
    assert_eq!(
        *store.calls.borrow(),
        ["begin", "control", "retained", "outcome"]
    );
    assert_no_publication(&store);
    let mut profile = limits();
    profile.codec = CodecLimits::new(AdmissionLimits::new(256, 16 * MIB, 1)?);
    let mut store = fresh_store()?;
    assert!(matches!(
        apply(&mut store, &mixed_frame(2_000)?, 2, profile),
        Err(RecordBatchTransactionError::Apply(
            RecordBatchApplyError::Encoding(CodecError::ItemLimit)
        ))
    ));
    assert!(store.calls.borrow().is_empty());
    assert_no_publication(&store);
    Ok(())
}

fn operational_failures() -> TestResult {
    let bytes = mixed_frame(2_000)?;
    for stage in [
        "begin",
        "control",
        "retained",
        "outcome",
        "placement",
        "record",
        "policy",
        "parallel",
        "head",
    ] {
        for error in [
            StateTransactionError::Corrupt,
            StateTransactionError::Unavailable,
            StateTransactionError::Limit,
        ] {
            let mut store = fresh_store()?;
            store.failure = Some((stage, error));
            assert!(matches!(apply(&mut store, &bytes, 2, limits()),
                Err(RecordBatchTransactionError::Transaction(actual)) if actual == error));
            assert_eq!(store.calls.borrow().last(), Some(&stage));
            assert_no_publication(&store);
        }
    }
    for error in [
        StateTransactionError::KnownAborted,
        StateTransactionError::Uncertain,
    ] {
        let mut store = fresh_store()?;
        store.failure = Some(("commit", error));
        assert!(matches!(apply(&mut store, &bytes, 2, limits()),
            Err(RecordBatchTransactionError::Transaction(actual)) if actual == error));
        assert_eq!(store.calls.borrow().last(), Some(&"commit"));
        assert_no_publication(&store);
    }
    Ok(())
}

fn empty_type_is_business() -> TestResult {
    let mut store = observed_store()?;
    let result = apply(
        &mut store,
        &frame(1, vec![(b"i", create(b""))])?,
        0,
        limits(),
    )?;
    let outcome = load::retained(result)?;
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(outcome.response(), [2, 0, 0, 0, 1, 1, 0, 8, 0]);
    assert_eq!(*store.queried_types.borrow(), [Vec::<u8>::new()]);
    assert!(store.committed);
    assert_eq!(store.writes, [] as [RecordWrite; 0]);
    assert_eq!(store.histories, [] as [HistoryEntry; 0]);
    assert!(store.written_outcome.is_some());
    Ok(())
}

#[test]
fn driver_limits_commit_refuse_without_publication() -> TestResult {
    prospective_limits()?;
    operational_failures()?;
    empty_type_is_business()
}
