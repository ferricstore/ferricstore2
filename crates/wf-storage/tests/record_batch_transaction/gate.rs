use super::*;

fn retained_store() -> TestResult<ObservedStore> {
    let mut store = observed_store()?;
    store.control = processed()?;
    store.retained_present = true;
    store.retained = RefCell::new(Some(original()?));
    store.failure = Some(("placement", StateTransactionError::Unavailable));
    Ok(store)
}

#[test]
fn record_batch_transaction_gates_before_fresh_reads() -> TestResult {
    for kind in ["replay", "conflict", "expired", "log"] {
        let mut store = retained_store()?;
        let now = if kind == "expired" { 1_001 } else { 2 };
        let bytes = if kind == "log" {
            hex(MIN_FRAME)?
        } else {
            frame(
                now,
                vec![(
                    b"i",
                    create(if kind == "conflict" {
                        b"different"
                    } else {
                        b"r"
                    }),
                )],
            )?
        };
        let at = if kind == "log" { log(0) } else { log(1) };
        let result = apply_record_batch_transaction(&mut store, &scope()?, &bytes, at, limits())?;
        match (kind, result) {
            ("replay", RecordBatchTransactionOutcome::Retained { control, outcome }) => {
                assert_eq!(control.position(), AppliedPosition::new(1));
                assert_eq!(outcome.response(), [0]);
                assert_eq!(outcome.original_position(), log(0));
            }
            ("conflict", RecordBatchTransactionOutcome::IntentConflict(control))
            | ("expired", RecordBatchTransactionOutcome::Expired(control)) => {
                assert_eq!(control.position(), AppliedPosition::new(1));
            }
            ("log", RecordBatchTransactionOutcome::LogReplay(control)) => {
                assert_eq!(control, processed()?);
            }
            _ => return Err("wrong complete gate classification".into()),
        }
        assert!(store.committed);
        assert_eq!(
            *store.calls.borrow(),
            ["begin", "control", "retained", "outcome", "commit"]
        );
        assert_eq!(store.writes, [] as [RecordWrite; 0]);
        assert_eq!(store.histories, [] as [HistoryEntry; 0]);
        assert!(store.written_outcome.is_none());
        assert_eq!(store.next_control.is_none(), kind == "log");
        assert!(store.queried_types.borrow().is_empty());
    }
    Ok(())
}

#[test]
fn record_batch_transaction_returns_owned_original_replay() -> TestResult {
    let mut store = retained_store()?;
    let (response_address, expected) = {
        let original = store.retained.borrow();
        let original = original.as_ref().ok_or("original missing")?;
        (original.response().as_ptr(), original.clone())
    };
    let expected_scope = scope()?;
    let bytes = frame(9, vec![(b"i", create(b"r"))])?;
    let result =
        apply_record_batch_transaction(&mut store, &expected_scope, &bytes, log(1), limits())?;
    assert!(store.committed);
    assert!(store.retained.borrow().is_none());
    drop(store);
    drop(bytes);
    drop(expected_scope);
    let outcome = load::retained(result)?;
    assert_eq!(*outcome, expected);
    assert_eq!(outcome.response().as_ptr(), response_address);
    assert_eq!(outcome.original_time(), TimestampMillis::new(1));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(1_001));
    assert_eq!(outcome.original_position(), log(0));
    assert_eq!(outcome.response_version(), 1);
    assert_eq!(outcome.response(), [0]);
    let mut store = retained_store()?;
    store.failure = Some(("commit", StateTransactionError::Uncertain));
    assert!(matches!(
        apply_record_batch_transaction(
            &mut store,
            &scope()?,
            &frame(2, vec![(b"i", create(b"r"))])?,
            log(1),
            limits()
        ),
        Err(RecordBatchTransactionError::Transaction(
            StateTransactionError::Uncertain
        ))
    ));
    assert!(!store.committed);
    assert_eq!(store.writes, [] as [RecordWrite; 0]);
    Ok(())
}
