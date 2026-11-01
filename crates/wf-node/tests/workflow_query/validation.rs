//! Authority failures, scoped minimums, exact barriers and physical visibility.
use super::*;
use wf_ports::state::StrongWorkflowRecordReadError as R;

#[test]
fn strong_record_query_reauthorizes_every_attempt_and_keeps_unavailable_distinct_from_missing()
-> TestResult {
    let mut fixture = QueryFixture::new("strong-query-auth")?;
    fixture.create()?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    assert!(
        query(
            &mut fixture,
            request(&scope, &id, None),
            &context(),
            limits()
        )?
        .record()
        .is_some()
    );
    fixture.fault = QueryFault::Deny;
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &id, None),
            &context(),
            limits()
        ),
        Err(E::Read(R::Unauthorized))
    ));
    fixture.fault = QueryFault::Unavailable;
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &id, None),
            &context(),
            limits()
        ),
        Err(E::Read(R::Unavailable))
    ));
    assert_eq!(fixture.reads, 3);
    fixture.finish()
}

#[test]
fn strong_record_query_validates_applied_barrier_full_identity_and_point() -> TestResult {
    let mut fixture = QueryFixture::new("strong-query-barrier")?;
    fixture.create()?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    fixture.fault = QueryFault::BarrierAhead;
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &id, None),
            &context(),
            limits()
        ),
        Err(E::Behind)
    ));
    fixture.fault = QueryFault::WrongIdentity;
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &id, None),
            &context(),
            limits()
        ),
        Err(E::InvalidObservation)
    ));
    fixture.fault = QueryFault::WrongPoint;
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &id, None),
            &context(),
            limits()
        ),
        Err(E::InvalidObservation)
    ));
    fixture.finish()
}

#[test]
fn strong_record_query_empty_genesis_cannot_satisfy_applied_entry_zero() -> TestResult {
    let mut fixture = QueryFixture::new("strong-query-genesis")?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let missing = query(
        &mut fixture,
        request(&scope, &id, None),
        &context(),
        limits(),
    )?;
    assert!(missing.record().is_none());
    assert!(missing.position().is_none());
    let after = WorkflowReadPosition {
        group: scope.group(),
        recovery: scope.recovery(),
        index: AppliedPosition::new(0),
    };
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &id, Some(after)),
            &context(),
            limits()
        ),
        Err(E::Behind)
    ));
    fixture.create()?;
    assert!(
        query(
            &mut fixture,
            request(&scope, &id, Some(after)),
            &context(),
            limits()
        )?
        .record()
        .is_some()
    );
    fixture.fault = QueryFault::EmptyControl;
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &id, Some(after)),
            &context(),
            limits()
        ),
        Err(E::Behind)
    ));
    fixture.finish()
}

#[test]
fn strong_record_query_checks_future_minimum_even_when_record_is_missing() -> TestResult {
    let mut fixture = QueryFixture::new("strong-query-missing-minimum")?;
    fixture.create()?;
    let scope = f::scope()?;
    let missing = RecordId::new(b"missing".to_vec())?;
    let after = WorkflowReadPosition {
        group: scope.group(),
        recovery: scope.recovery(),
        index: AppliedPosition::new(1),
    };
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &missing, Some(after)),
            &context(),
            limits()
        ),
        Err(E::Behind)
    ));
    assert!(
        query(
            &mut fixture,
            request(&scope, &missing, None),
            &context(),
            limits()
        )?
        .record()
        .is_none()
    );
    fixture.finish()
}

#[test]
fn strong_record_query_keeps_expired_active_deadline_visible_until_actual_transition() -> TestResult
{
    let mut fixture = QueryFixture::new("strong-query-active")?;
    fixture.active = Some(10);
    fixture.create()?;
    fixture.now = TimestampMillis::new(1000);
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let result = query(
        &mut fixture,
        request(&scope, &id, None),
        &context(),
        limits(),
    )?;
    let record = result.record().ok_or("active record")?;
    assert_eq!(record.native_record().record.state(), b"q");
    assert_eq!(
        record.value_binding().lifecycle.birth.max_active_ms(),
        Some(10)
    );
    fixture.finish()
}

#[test]
fn strong_record_query_reads_actually_scoped_native_metadata_before_and_after_reopen() -> TestResult
{
    let mut fixture = QueryFixture::scoped("strong-query-scoped", Some(b"ns"))?;
    fixture.create()?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut input = request(&scope, &id, None);
    input.scope_prefix = Some(b"ns");
    let first = query(&mut fixture, input, &context(), limits())?;
    let record = first.record().ok_or("scoped record")?;
    assert_eq!(record.scope_prefix(), Some(&b"ns"[..]));
    assert_eq!(record.logical_partition(), Some(&b"p"[..]));
    // Original physical routing is idempotent; an already physical caller value
    // must not be accepted as the captured logical partition of this record.
    let physical = record.native_record().partition.as_bytes().to_vec();
    let mut aliased = input;
    aliased.partition = &physical;
    assert!(matches!(
        query(&mut fixture, aliased, &context(), limits()),
        Err(E::InvalidObservation)
    ));
    let directory = fixture.directory.clone();
    drop(fixture.store);
    let store = wf_storage::state::RedbStateStore::open(
        &directory.join("state.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        wf_storage::state::StateStoreLimits::default(),
    )?;
    fixture.store = store;
    let second = query(&mut fixture, input, &context(), limits())?;
    assert_eq!(
        second.position().ok_or("position")?.index,
        AppliedPosition::new(0)
    );
    assert_eq!(
        second.record().ok_or("record")?.scope_prefix(),
        Some(&b"ns"[..])
    );
    fixture.finish()
}

#[test]
fn strong_record_query_preserves_decode_failure_as_an_operational_failure() -> TestResult {
    let mut fixture = QueryFixture::new("strong-query-context")?;
    fixture.create()?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut unsupported = context();
    unsupported.safe_atoms = None;
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &id, None),
            &unsupported,
            limits()
        ),
        Err(E::Preparation(_))
    ));
    fixture.finish()
}
