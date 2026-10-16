use super::*;
use wf_domain::value::{DomainError, InputField};
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::IndexRejection;
use wf_ports::log::CodecError;

fn assert_no_publication(store: &ObservedStore) {
    assert!(!store.committed);
    assert_eq!(store.writes, [] as [RecordWrite; 0]);
    assert_eq!(store.histories, [] as [HistoryEntry; 0]);
    assert!(store.written_outcome.is_none());
    assert!(store.next_control.is_none());
}

fn check_invalid_limits() -> TestResult {
    for field in 0..6 {
        let mut profile = limits();
        match field {
            0 => profile.maximum_read_steps = 0,
            1 => profile.maximum_read_bytes = 0,
            2 => profile.maximum_staging_bytes = 0,
            3 => profile.maximum_adapter_bytes = 0,
            4 => profile.workspace_bytes = 0,
            _ => profile.outcome_ttl_ms = 0,
        }
        let mut store = observed_store()?;
        assert!(matches!(
            apply_record_batch_transaction(
                &mut store,
                &scope()?,
                &hex(MIN_FRAME)?,
                log(0),
                profile
            ),
            Err(RecordBatchTransactionError::InvalidLimits)
        ));
        assert!(store.calls.borrow().is_empty());
        assert_no_publication(&store);
    }
    let mut profile = limits();
    profile.maximum_read_bytes = usize::MAX;
    let mut store = observed_store()?;
    assert!(matches!(
        apply_record_batch_transaction(&mut store, &scope()?, &hex(MIN_FRAME)?, log(0), profile),
        Err(RecordBatchTransactionError::Apply(
            RecordBatchApplyError::Encoding(CodecError::LengthOverflow)
        ))
    ));
    assert!(store.calls.borrow().is_empty());
    Ok(())
}

fn check_scope_and_workspace() -> TestResult {
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
        let mut store = observed_store()?;
        assert!(
            apply_record_batch_transaction(
                &mut store,
                &expected,
                &hex(MIN_FRAME)?,
                log(0),
                limits()
            )
            .is_err()
        );
        assert!(store.calls.borrow().is_empty());
    }
    let mut profile = limits();
    // Independent old D16,780,748 + configured R/S/C167,772,160, one byte short.
    profile.workspace_bytes = 184_552_907;
    let mut store = observed_store()?;
    assert!(matches!(
        apply_record_batch_transaction(&mut store, &scope()?, &hex(MIN_FRAME)?, log(0), profile),
        Err(RecordBatchTransactionError::Apply(
            RecordBatchApplyError::Workspace {
                attempted: 16_780_748,
                maximum: 16_780_747,
            }
        ))
    ));
    assert!(store.calls.borrow().is_empty());
    let mut profile = limits();
    profile.maximum_staging_bytes = 1_023;
    let mut store = observed_store()?;
    assert!(matches!(
        apply_record_batch_transaction(&mut store, &scope()?, &hex(MIN_FRAME)?, log(0), profile),
        Err(RecordBatchTransactionError::Transaction(
            StateTransactionError::Limit
        ))
    ));
    assert_no_publication(&store);
    Ok(())
}

fn check_operational_errors() -> TestResult {
    for stage in [
        "begin",
        "control",
        "retained",
        "outcome",
        "placement",
        "record",
        "policy",
    ] {
        for error in [
            StateTransactionError::Corrupt,
            StateTransactionError::Unavailable,
            StateTransactionError::Limit,
        ] {
            let mut store = observed_store()?;
            store.failure = Some((stage, error));
            assert!(
                matches!(apply_record_batch_transaction(&mut store, &scope()?,
                &hex(MIN_FRAME)?, log(0), limits()),
                Err(RecordBatchTransactionError::Transaction(actual)) if actual == error)
            );
            assert_no_publication(&store);
            assert_eq!(store.calls.borrow().last(), Some(&stage));
        }
    }
    // The physical driver's actual retained read precedes its pure exact-log gate.
    let mut store = observed_store()?;
    store.control = processed()?;
    store.retained_present = true;
    store.failure = Some(("outcome", StateTransactionError::Corrupt));
    assert!(matches!(
        apply_record_batch_transaction(&mut store, &scope()?, &hex(MIN_FRAME)?, log(0), limits()),
        Err(RecordBatchTransactionError::Transaction(
            StateTransactionError::Corrupt
        ))
    ));
    assert_no_publication(&store);
    Ok(())
}

fn check_prefetch_and_empty_type() -> TestResult {
    let bytes = frame(1, vec![(b"i", create(b"r")), (b"i", create(b"broken"))])?;
    let mut store = observed_store()?;
    store.policy_bytes.push((b"broken".to_vec(), vec![0xff]));
    assert!(matches!(
        apply_record_batch_transaction(&mut store, &scope()?, &bytes, log(0), limits()),
        Err(RecordBatchTransactionError::Transaction(
            StateTransactionError::Corrupt
        ))
    ));
    assert_no_publication(&store);
    assert!(store.queried_types.borrow().contains(&b"broken".to_vec()));
    // A healthy exact empty-type lookup reaches the existing business rejection.
    let mut store = observed_store()?;
    let result = apply_record_batch_transaction(
        &mut store,
        &scope()?,
        &frame(1, vec![(b"i", create(b""))])?,
        log(0),
        limits(),
    )?;
    let outcome = load::retained(result)?;
    assert_eq!(
        outcome.status(),
        RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
            DomainError::EmptyValue(InputField::Type)
        )))
    );
    assert_eq!(*store.queried_types.borrow(), [Vec::<u8>::new()]);
    assert!(store.committed);
    assert_eq!(store.writes, [] as [RecordWrite; 0]);
    assert_eq!(store.histories, [] as [HistoryEntry; 0]);
    assert!(store.written_outcome.is_some());
    Ok(())
}

#[test]
fn record_batch_transaction_refuses_scope_limits_and_storage_errors() -> TestResult {
    check_invalid_limits()?;
    check_scope_and_workspace()?;
    check_operational_errors()?;
    check_prefetch_and_empty_type()?;
    Ok(())
}
