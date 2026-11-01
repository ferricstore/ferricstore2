//! Application query contract over real consumed storage and a controlled barrier.
#![forbid(unsafe_code)]
mod fixtures;
mod ports;
mod resources;
mod validation;
use fixtures as f;
use fixtures::*;
use wf_application::queries::{
    StrongWorkflowRecordError as E, query_strong_workflow_record as query,
};
use wf_ports::state::WorkflowReadPosition;
use wf_types::{
    identity::RecordId,
    version::{AppliedPosition, TimestampMillis},
};
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn strong_record_query_reads_actual_create_with_scoped_applied_entry_zero() -> TestResult {
    let mut fixture = QueryFixture::new("strong-query-create")?;
    fixture.create()?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let result = query(
        &mut fixture,
        request(&scope, &id, None),
        &context(),
        limits(),
    )?;
    assert_eq!(fixture.reads, 1);
    assert_eq!(result.scope(), &scope);
    assert_eq!(
        result.position().ok_or("position")?.index,
        AppliedPosition::new(0)
    );
    let record = result.record().ok_or("record")?;
    assert_eq!(record.native_record().record.id(), &id);
    assert_eq!(record.native_record().record.state(), b"q");
    assert_eq!(record.logical_partition(), Some(&b"p"[..]));
    assert_eq!(record.scope_prefix(), None);
    assert_eq!(
        record.value_binding().lifecycle.birth.retention_ttl_ms(),
        10
    );
    fixture.finish()
}

#[test]
fn strong_record_query_hides_actual_terminal_at_the_server_deadline() -> TestResult {
    let mut fixture = QueryFixture::new("strong-query-terminal")?;
    fixture.create()?;
    fixture.cancel()?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    fixture.now = TimestampMillis::new(119);
    let live = query(
        &mut fixture,
        request(&scope, &id, None),
        &context(),
        limits(),
    )?;
    assert_eq!(
        live.record()
            .ok_or("terminal")?
            .native_record()
            .record
            .state(),
        b"cancelled"
    );
    fixture.now = TimestampMillis::new(120);
    let expired = query(
        &mut fixture,
        request(&scope, &id, None),
        &context(),
        limits(),
    )?;
    assert!(expired.record().is_none());
    assert_eq!(
        expired.position().ok_or("position")?.index,
        AppliedPosition::new(1)
    );
    fixture.finish()
}

#[test]
fn strong_record_query_refuses_foreign_recovery_before_any_authority_read() -> TestResult {
    let mut fixture = QueryFixture::new("strong-query-position")?;
    fixture.create()?;
    let scope = f::scope()?;
    let other = f::scope_for(b"t", b"g", b"old")?;
    let id = RecordId::new(b"i".to_vec())?;
    let after = WorkflowReadPosition {
        group: other.group(),
        recovery: other.recovery(),
        index: AppliedPosition::new(0),
    };
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &id, Some(after)),
            &context(),
            limits()
        ),
        Err(E::IncompatiblePosition)
    ));
    assert_eq!(fixture.reads, 0);
    fixture.finish()
}
