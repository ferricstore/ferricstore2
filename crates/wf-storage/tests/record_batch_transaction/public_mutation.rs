//! Recording contracts only; actual retained bytes and durability use native tests.

use wf_domain::claim::Claim;
use wf_domain::history::EventKind;
use wf_domain::lease::LeaseToken;
use wf_domain::workflow::DomainError;
use wf_domain::workflow::admission::AdmissionError;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::caller::{MutationFieldValue, MutationInput, normalize_core_mutation};
use wf_domain::workflow::control::{EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::indexed::{IndexRejection, OrderingMetadata};
use wf_ports::log::{encode_public_mutation, prepare_public_mutation};
use wf_storage::transaction::apply_public_mutation_transaction;

use super::*;

const PUBLIC_FRAME: &str = "4657435000010400000001740000000167000000017a00000001710001ad27fb04f2617b292c86a42ad3e31ef6346e62938f4586d81588f1c76ad5846600000000000007d0000000000000000007d0000100000000017000000001690000000000000007d005000000016c00000000000000010000";

fn previous() -> TestResult<ControlState> {
    Ok(ControlState::restored(
        clock(1_000)?,
        EntryIdentity::new(log(1), PreparedDigest::new([7; 32])),
        EntryStamp::Stamped(clock(1_000)?),
    )?)
}

fn encoded(now: u64, token: &[u8]) -> TestResult<Vec<u8>> {
    let fields = [
        ("lease_token", MutationFieldValue::Bytes(token)),
        ("fencing_token", MutationFieldValue::Integer(1)),
    ];
    let caller = normalize_core_mutation(
        MutationInput {
            operation: "complete",
            id: b"i",
            fields: &fields,
        },
        &[],
        Some(b"p"),
        AdmissionLimits::default(),
        16 * MIB,
    )?;
    Ok(encode_public_mutation(
        &scope()?,
        &RequestId::new(b"q".to_vec())?,
        &caller,
        clock(now)?,
        limits().codec,
        limits().workspace_bytes,
    )?
    .0
    .as_bytes()
    .to_vec())
}

fn running() -> TestResult<StoredWorkflowRecord> {
    let id = RecordId::new(b"i".to_vec())?;
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
    let record = reduce(
        Some(&initial),
        &CommittedCommand {
            id,
            now: TimestampMillis::new(1_000),
            operation: Operation::Claim(Claim {
                expected_state: b"q".to_vec(),
                worker: b"w".to_vec(),
                token: LeaseToken::new(b"l".to_vec())?,
                lease_ms: 30_000,
            }),
        },
    )?
    .into_parts()
    .1
    .ok_or("claim absent")?
    .record;
    assert_eq!(record.version(), 2);
    Ok(StoredWorkflowRecord {
        tenant: scope()?.tenant().clone(),
        partition: PartitionId::new(b"p".to_vec())?,
        record,
        ordering: OrderingMetadata {
            priority: 0,
            state_enter_seq: 99,
        },
    })
}

fn fresh_store(present: bool) -> TestResult<ObservedStore> {
    let mut store = observed_store()?;
    store.control = previous()?;
    if present {
        store.records.push(running()?);
    }
    Ok(store)
}

fn apply(
    store: &mut ObservedStore,
    bytes: &[u8],
    index: u64,
    profile: RecordBatchTransactionLimits,
) -> Result<RecordBatchTransactionOutcome, RecordBatchTransactionError> {
    let expected_scope = store.scope.clone();
    apply_public_mutation_transaction(store, &expected_scope, bytes, log(index), profile)
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

fn fresh_publication() -> TestResult {
    let bytes = encoded(2_000, b"l")?;
    // Independent fixed-width pack, not an encoder-derived expected value.
    assert_eq!(bytes, hex(PUBLIC_FRAME)?);
    assert_eq!(bytes.len(), 117);
    for fifo in [false, true] {
        let mut store = fresh_store(true)?;
        let policy = hex(RICH)?;
        if fifo {
            store.parallel = false;
            store.policy_bytes.push((b"r".to_vec(), policy.clone()));
        }
        let mut profile = limits();
        profile.codec = CodecLimits::new(AdmissionLimits::new(256, 16 * MIB, 1)?);
        let (control, outcome) = reply(apply(&mut store, &bytes, 2, profile)?)?;
        assert!(store.committed);
        assert_eq!(store.captured_admission, Some(profile.codec.admission()));
        assert_eq!(
            *store.calls.borrow(),
            [
                "begin",
                "control",
                "retained",
                "outcome",
                "placement",
                "record",
                "policy",
                "parallel",
                if fifo { "member" } else { "head" },
                "commit"
            ]
        );
        assert_eq!(*store.queried_types.borrow(), [b"r".to_vec()]);
        assert_eq!(store.writes.len(), 1);
        let write = &store.writes[0];
        assert_eq!(write.expected_version, Some(2));
        assert_eq!(write.record.id().as_bytes(), b"i");
        assert_eq!(write.record.version(), 3);
        assert_eq!(write.record.state(), b"completed");
        assert_eq!(store.histories.len(), 1);
        assert_eq!(
            (store.histories[0].kind, store.histories[0].version),
            (EventKind::Completed, 3)
        );
        assert_eq!(store.histories[0].event_id, "2000-3");
        assert_eq!(store.written_outcome.as_ref(), Some(outcome.as_ref()));
        assert_eq!(control.position(), AppliedPosition::new(2));
        assert_eq!(outcome.intent().version(), 1);
        assert_eq!(
            outcome.intent().digest().as_slice(),
            hex("ad27fb04f2617b292c86a42ad3e31ef6346e62938f4586d81588f1c76ad58466")?
        );
        assert_eq!(outcome.response_version(), 2);
        assert_eq!(outcome.response(), [0]);
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        assert_eq!(outcome.original_position(), log(2));
        assert_eq!(outcome.original_time(), TimestampMillis::new(2_000));
        assert_eq!(outcome.expires_at(), TimestampMillis::new(3_000));
        if fifo {
            assert_eq!(store.policy_bytes, [(b"r".to_vec(), policy)]);
        }
    }
    Ok(())
}

fn missing_rejection() -> TestResult<(ControlState, Box<RetainedOutcome>)> {
    let mut store = fresh_store(false)?;
    let result = reply(apply(&mut store, &encoded(2_000, b"l")?, 2, limits())?)?;
    assert!(store.committed);
    assert_eq!(
        *store.calls.borrow(),
        [
            "begin",
            "control",
            "retained",
            "outcome",
            "placement",
            "record",
            "commit"
        ]
    );
    assert!(store.queried_types.borrow().is_empty());
    assert_eq!(store.writes, [] as [RecordWrite; 0]);
    assert_eq!(store.histories, [] as [HistoryEntry; 0]);
    assert_eq!(result.1.response(), [2, 0, 0]);
    assert_eq!(
        result.1.status(),
        RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
            DomainError::NotFound
        )))
    );
    Ok(result)
}

fn retained_store(control: ControlState, original: RetainedOutcome) -> TestResult<ObservedStore> {
    let mut store = fresh_store(false)?;
    store.control = control;
    store.retained_present = true;
    *store.retained.borrow_mut() = Some(original);
    store.failure = Some(("placement", StateTransactionError::Unavailable));
    Ok(store)
}

fn altered(
    original: &RetainedOutcome,
    status: RetainedStatus,
    body: Vec<u8>,
) -> TestResult<RetainedOutcome> {
    Ok(RetainedOutcome::new(
        RetainedMetadata {
            scope: original.scope().clone(),
            request: original.request().clone(),
            intent: original.intent(),
            original_position: original.original_position(),
            original_time: original.original_time(),
            expires_at: original.expires_at(),
            status,
        },
        2,
        body,
        AdmissionLimits::default(),
    )?)
}

fn complete_and_replay() -> TestResult {
    let (control, original) = missing_rejection()?;
    for kind in ["conflict", "expired", "log"] {
        // Typed retained corruption checks pure gate precedence, not physical scan priority.
        let malformed = altered(&original, original.status(), vec![0xff])?;
        let mut store = retained_store(control, malformed)?;
        let bytes = match kind {
            "conflict" => encoded(2_001, b"different")?,
            "expired" => encoded(3_000, b"l")?,
            _ => encoded(2_000, b"l")?,
        };
        let result = apply(
            &mut store,
            &bytes,
            if kind == "log" { 2 } else { 3 },
            limits(),
        )?;
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
        assert!(store.committed);
        assert_eq!(store.next_control.is_none(), kind == "log");
        assert!(store.written_outcome.is_none());
        assert_eq!(store.writes, [] as [RecordWrite; 0]);
        assert_eq!(store.histories, [] as [HistoryEntry; 0]);
    }
    for (status, body) in [
        (original.status(), vec![0xff]),
        (RetainedStatus::Accepted, vec![2, 0, 0]),
    ] {
        let mut store = retained_store(control, altered(&original, status, body)?)?;
        assert!(apply(&mut store, &encoded(2_001, b"l")?, 3, limits()).is_err());
        assert_eq!(
            *store.calls.borrow(),
            ["begin", "control", "retained", "outcome"]
        );
        assert_no_publication(&store);
    }
    let address = original.response().as_ptr();
    let expected = original.clone();
    let mut store = retained_store(control, *original)?;
    let expected_scope = scope()?;
    let bytes = encoded(2_001, b"l")?;
    let (next, replayed) = reply(apply_public_mutation_transaction(
        &mut store,
        &expected_scope,
        &bytes,
        log(3),
        limits(),
    )?)?;
    assert_eq!(
        *store.calls.borrow(),
        ["begin", "control", "retained", "outcome", "commit"]
    );
    assert!(store.retained.borrow().is_none());
    assert!(store.written_outcome.is_none());
    assert_eq!(next.position(), AppliedPosition::new(3));
    drop(store);
    drop(expected_scope);
    drop(bytes);
    assert_eq!(replayed, expected);
    assert_eq!(replayed.response().as_ptr(), address);
    assert_eq!(replayed.original_position(), log(2));
    assert_eq!(replayed.original_time(), TimestampMillis::new(2_000));
    assert_eq!(replayed.expires_at(), TimestampMillis::new(3_000));
    Ok(())
}

fn whole_refusal_replays() -> TestResult {
    let mut profile = limits();
    profile.codec = CodecLimits::new(AdmissionLimits::new(256, 2_800, 1)?);
    let bytes = encoded(2_000, b"l")?;
    let mut store = fresh_store(true)?;
    let (control, outcome) = reply(apply(&mut store, &bytes, 2, profile)?)?;
    assert!(store.committed);
    assert_eq!(outcome.response(), [1, 1, 4]);
    assert_eq!(
        outcome.status(),
        RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Admission(
            AdmissionError::DecisionBytes
        )))
    );
    assert_eq!(store.writes, [] as [RecordWrite; 0]);
    assert_eq!(store.histories, [] as [HistoryEntry; 0]);
    let expected = outcome.clone();
    let mut store = retained_store(control, *outcome)?;
    let (_, replayed) = reply(apply(&mut store, &encoded(2_001, b"l")?, 3, profile)?)?;
    assert_eq!(replayed, expected);
    assert_eq!(
        *store.calls.borrow(),
        ["begin", "control", "retained", "outcome", "commit"]
    );
    Ok(())
}

fn workspace_limits() -> TestResult {
    let bytes = hex(PUBLIC_FRAME)?;
    // E117,n1,B16MiB: D16,780,756; F33,558,420; L=R64+S64+C32MiB.
    for (workspace, attempted, maximum, opened) in [
        (184_552_915, 16_780_756, 16_780_755, false),
        (184_552_916, 201_330_580, 184_552_916, true),
        (201_330_579, 201_330_580, 201_330_579, true),
    ] {
        let mut store = fresh_store(false)?;
        let mut profile = limits();
        profile.workspace_bytes = workspace;
        assert!(matches!(apply(&mut store, &bytes, 2, profile),
            Err(RecordBatchTransactionError::Apply(RecordBatchApplyError::Workspace { attempted: a, maximum: m }))
                if (a,m)==(attempted,maximum)));
        assert_eq!(!store.calls.borrow().is_empty(), opened);
        if opened {
            assert_eq!(store.calls.borrow().last(), Some(&"record"));
        }
        assert_no_publication(&store);
    }
    let mut store = fresh_store(false)?;
    let mut profile = limits();
    profile.workspace_bytes = 201_330_580;
    assert_eq!(
        reply(apply(&mut store, &bytes, 2, profile)?)?.1.response(),
        [2, 0, 0]
    );
    assert!(store.committed);
    Ok(())
}

fn staging_limits() -> TestResult {
    let bytes = hex(PUBLIC_FRAME)?;
    // S=1024 +512 query slots +1024 type slots +128 set +2 key bytes
    // +512 record slots +258 placement +1536 index slots +128 current fact.
    for (cap, read) in [(1_023, false), (5_123, true)] {
        let mut store = fresh_store(false)?;
        let mut profile = limits();
        profile.maximum_staging_bytes = cap;
        assert!(matches!(
            apply(&mut store, &bytes, 2, profile),
            Err(RecordBatchTransactionError::Transaction(
                StateTransactionError::Limit
            ))
        ));
        assert_eq!(
            *store.calls.borrow(),
            if read {
                vec![
                    "begin",
                    "control",
                    "retained",
                    "outcome",
                    "placement",
                    "record",
                ]
            } else {
                vec!["begin", "control", "retained", "outcome"]
            }
        );
        assert_no_publication(&store);
    }
    let mut store = fresh_store(false)?;
    let mut profile = limits();
    profile.maximum_staging_bytes = 5_124;
    assert_eq!(
        reply(apply(&mut store, &bytes, 2, profile)?)?.1.response(),
        [2, 0, 0]
    );
    assert!(store.committed);
    Ok(())
}

fn preparation_refusals() -> TestResult {
    let bytes = hex(PUBLIC_FRAME)?;
    let private = super::frame(2_000, vec![(b"i", create(b"r"))])?;
    for bad in [&[][..], &bytes[..bytes.len() - 1], private.as_slice()] {
        let mut store = fresh_store(false)?;
        assert!(apply(&mut store, bad, 2, limits()).is_err());
        assert!(store.calls.borrow().is_empty());
        assert_no_publication(&store);
    }
    for field in 0..3 {
        let expected = RequestScope::new(
            TenantId::new(if field == 0 {
                b"other".to_vec()
            } else {
                b"t".to_vec()
            })?,
            GroupId::new(if field == 1 {
                b"other".to_vec()
            } else {
                b"g".to_vec()
            })?,
            RecoveryId::new(if field == 2 {
                b"other".to_vec()
            } else {
                b"z".to_vec()
            })?,
        );
        let mut store = fresh_store(false)?;
        assert!(
            apply_public_mutation_transaction(&mut store, &expected, &bytes, log(2), limits())
                .is_err()
        );
        assert!(store.calls.borrow().is_empty());
        assert_no_publication(&store);
    }
    let decoded = prepare_public_mutation(&bytes, limits().codec, limits().workspace_bytes)?;
    assert_eq!(decoded.request().intent.version(), 1);
    assert_eq!(decoded.request().request.as_bytes(), b"q");
    Ok(())
}

fn operational_errors() -> TestResult {
    let bytes = hex(PUBLIC_FRAME)?;
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
            let mut store = fresh_store(true)?;
            store.failure = Some((stage, error));
            assert!(matches!(apply(&mut store, &bytes, 2, limits()),
                Err(RecordBatchTransactionError::Transaction(actual)) if actual==error));
            assert_eq!(store.calls.borrow().last(), Some(&stage));
            assert_no_publication(&store);
        }
    }
    for error in [
        StateTransactionError::KnownAborted,
        StateTransactionError::Uncertain,
    ] {
        let mut store = fresh_store(true)?;
        store.failure = Some(("commit", error));
        assert!(matches!(apply(&mut store, &bytes, 2, limits()),
            Err(RecordBatchTransactionError::Transaction(actual)) if actual==error));
        assert_eq!(store.calls.borrow().last(), Some(&"commit"));
        assert_no_publication(&store);
    }
    // Existing initial outcome read still precedes even exact-log classification.
    let (control, original) = missing_rejection()?;
    let mut store = retained_store(control, *original)?;
    store.failure = Some(("outcome", StateTransactionError::Corrupt));
    assert!(matches!(
        apply(&mut store, &bytes, 2, limits()),
        Err(RecordBatchTransactionError::Transaction(
            StateTransactionError::Corrupt
        ))
    ));
    assert_no_publication(&store);
    Ok(())
}

#[test]
fn public_item_driver_preserves_gate_and_owned_result() -> TestResult {
    fresh_publication()?;
    complete_and_replay()?;
    whole_refusal_replays()?;
    workspace_limits()?;
    staging_limits()?;
    preparation_refusals()?;
    operational_errors()
}
