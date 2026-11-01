//! Actual empty/stored/expired values and explicitly captured read-failure controls.
use super::*;
use wf_application::queries::StrongWorkflowRecordValueError as E;
use wf_ports::{
    blob::{HistoryValueReadConversionError as C, WorkflowValueReadFailure},
    state::StrongWorkflowRecordReadError as R,
};

#[test]
fn record_values_empty_binary_is_decoded_at_zero_maximum() -> TestResult {
    let mut fixture = HydrationFixture::new("record-values-empty", Some(b""))?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let result = hydrated(
        &mut fixture,
        request_values(&scope, &id, true, 0)?,
        &context(),
        value_limits(),
    )?;
    assert_eq!(result.value_writes().len(), 2);
    assert!(
        matches!(&result.value_writes()[0].value,HistoryValue::Decoded(value) if value.encoded().is_empty())
    );
    assert!(matches!(
        result.value_writes()[1].value,
        HistoryValue::StoredSize(0)
    ));
    fixture.finish()
}

#[test]
fn record_values_no_refs_skip_unused_read_and_projection_profiles() -> TestResult {
    let mut fixture = HydrationFixture::new("record-values-no-refs", None)?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut selected = value_limits();
    selected.values.maximum_work = 0;
    selected.conversion.maximum_work = 0;
    selected.projection.maximum_work_units = 0;
    let result = hydrated(
        &mut fixture,
        request_values(&scope, &id, true, 8)?,
        &context(),
        selected,
    )?;
    assert!(result.record().record().is_some());
    assert!(result.value_writes().is_empty());
    assert_eq!(fixture.value_reads, 0);
    fixture.finish()
}

#[test]
fn record_values_missing_and_expired_records_do_not_read_values() -> TestResult {
    let mut fixture = HydrationFixture::new("record-values-missing-record", Some(b"data"))?;
    let scope = f::scope()?;
    let missing = RecordId::new(b"missing".to_vec())?;
    assert!(
        hydrated(
            &mut fixture,
            request_values(&scope, &missing, true, 8)?,
            &context(),
            value_limits()
        )?
        .record()
        .record()
        .is_none()
    );
    fixture.base.cancel()?;
    fixture.base.now = TimestampMillis::new(120);
    let id = RecordId::new(b"i".to_vec())?;
    assert!(
        hydrated(
            &mut fixture,
            request_values(&scope, &id, true, 8)?,
            &context(),
            value_limits()
        )?
        .record()
        .record()
        .is_none()
    );
    assert_eq!(fixture.value_reads, 0);
    fixture.finish()
}

#[test]
fn record_values_rechecks_authorization_after_record_read_and_preserves_unavailability()
-> TestResult {
    let mut fixture = HydrationFixture::new("record-values-auth", Some(b"data"))?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    fixture.fault = HydrationFault::RevokeAfterRecord;
    assert!(matches!(
        hydrated(
            &mut fixture,
            request_values(&scope, &id, true, 8)?,
            &context(),
            value_limits()
        ),
        Err(E::Read(R::Unauthorized))
    ));
    fixture.fault = HydrationFault::Unavailable;
    assert!(matches!(
        hydrated(
            &mut fixture,
            request_values(&scope, &id, true, 8)?,
            &context(),
            value_limits()
        ),
        Err(E::Read(R::Unavailable))
    ));
    assert_eq!(fixture.value_reads, 2);
    fixture.finish()
}

#[test]
fn record_values_captured_missing_and_storage_failure_produce_distinct_source_fields() -> TestResult
{
    let mut fixture = HydrationFixture::new("record-values-captured-errors", Some(b"data"))?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    fixture.fault = HydrationFault::Missing;
    let missing = hydrated(
        &mut fixture,
        request_values(&scope, &id, true, 8)?,
        &context(),
        value_limits(),
    )?;
    assert_eq!(missing.value_writes()[0].field, F::Value(K::Payload));
    assert!(matches!(missing.value_writes()[0].value, HistoryValue::Nil));
    assert_eq!(missing.value_writes()[1].field, F::Missing(K::Payload));
    fixture.fault = HydrationFault::ReadFailure;
    let failed = hydrated(
        &mut fixture,
        request_values(&scope, &id, true, 8)?,
        &context(),
        value_limits(),
    )?;
    assert_eq!(failed.value_writes().len(), 1);
    assert_eq!(failed.value_writes()[0].field, F::Error(K::Payload));
    assert!(matches!(
        failed.value_writes()[0].value,
        HistoryValue::Error("ERR storage read failed")
    ));
    fixture.finish()
}

#[test]
fn record_values_refuses_malformed_batches_and_unsafe_drift_without_missing_flags() -> TestResult {
    let mut fixture = HydrationFixture::new("record-values-bad-observation", Some(b"data"))?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    fixture.fault = HydrationFault::Mismatch;
    assert!(matches!(
        hydrated(
            &mut fixture,
            request_values(&scope, &id, true, 8)?,
            &context(),
            value_limits()
        ),
        Err(E::Conversion(C::InvalidBatch))
    ));
    fixture.fault = HydrationFault::UnsafeDrift;
    assert!(matches!(
        hydrated(
            &mut fixture,
            request_values(&scope, &id, true, 8)?,
            &context(),
            value_limits()
        ),
        Err(E::Conversion(C::Unprojectable {
            failure: WorkflowValueReadFailure::UnsafeDrift,
            ..
        }))
    ));
    fixture.finish()
}

#[test]
fn record_values_reads_actual_result_after_atomic_start_and_complete() -> TestResult {
    use wf_domain::{
        lease::LeaseGuard,
        workflow::{Operation, Terminal},
    };
    let mut fixture = HydrationFixture::started("record-values-result", b"data")?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let current = query(
        &mut fixture.base,
        request(&scope, &id, None),
        &context(),
        limits(),
    )?;
    let core = &current.record().ok_or("started")?.native_record().record;
    let guard = LeaseGuard {
        token: core.lease().ok_or("lease")?.token.clone(),
        fence: core.fence(),
    };
    fixture.base.apply(
        Operation::Complete(Terminal {
            guard,
            payload: None,
            value: Some(b"result".to_vec()),
        }),
        1,
        110,
    )?;
    fixture.base.now = TimestampMillis::new(111);
    let result = hydrated(
        &mut fixture,
        request_values(&scope, &id, true, 8)?,
        &context(),
        value_limits(),
    )?;
    assert_eq!(result.value_writes().len(), 4);
    assert_eq!(result.value_writes()[2].field, F::Value(K::Result));
    assert!(
        matches!(&result.value_writes()[2].value,HistoryValue::Decoded(value) if value.encoded()==b"result")
    );
    assert!(matches!(
        result.value_writes()[3].value,
        HistoryValue::StoredSize(6)
    ));
    fixture.finish()
}

#[test]
fn record_values_actual_error_can_expire_between_record_and_value_reads() -> TestResult {
    use wf_domain::workflow::{Cancel, Operation};
    use wf_types::version::Fence;
    let mut fixture = HydrationFixture::new("record-values-expired-error", Some(b"data"))?;
    fixture.base.apply(
        Operation::Cancel(Cancel {
            fence: Fence::new(0),
            lease_token: None,
            expected_state: None,
            expected_version: None,
            error: Some(b"bad".to_vec()),
        }),
        1,
        110,
    )?;
    fixture.base.now = TimestampMillis::new(110);
    fixture.value_now = Some(TimestampMillis::new(121));
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let result = hydrated(
        &mut fixture,
        request_values(&scope, &id, true, 8)?,
        &context(),
        value_limits(),
    )?;
    assert!(result.record().record().is_some());
    assert_eq!(result.value_writes().len(), 4);
    assert_eq!(result.value_writes()[2].field, F::Value(K::Error));
    assert!(matches!(result.value_writes()[2].value, HistoryValue::Nil));
    assert_eq!(result.value_writes()[3].field, F::Missing(K::Error));
    fixture.finish()
}

#[test]
fn record_values_reads_actual_scoped_refs_after_reopen() -> TestResult {
    let mut fixture = HydrationFixture::scoped("record-values-scoped", Some(b"data"), Some(b"ns"))?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut input = request_values(&scope, &id, true, 8)?;
    input.record.scope_prefix = Some(b"ns");
    let first = hydrated(&mut fixture, input, &context(), value_limits())?;
    assert_eq!(first.value_writes().len(), 2);
    let path = fixture.base.directory.join("state.redb");
    drop(fixture.base.store);
    fixture.base.store = wf_storage::state::RedbStateStore::open(
        &path,
        scope.group().clone(),
        scope.recovery().clone(),
        wf_storage::state::StateStoreLimits::default(),
    )?;
    let second = hydrated(&mut fixture, input, &context(), value_limits())?;
    assert!(
        matches!(&second.value_writes()[0].value,HistoryValue::Decoded(value) if value.encoded()==b"data")
    );
    fixture.finish()
}
