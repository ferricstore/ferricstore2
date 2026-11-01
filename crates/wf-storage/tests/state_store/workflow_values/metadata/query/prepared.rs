//! Real native captures feed typed query preparation, without legacy file locations.
use super::*;
use wf_ports::projection::query_row::{
    NativeQueryPreparationError as E, prepare_native_query_snapshot as prepare,
};
mod fixtures;
mod resources;
mod validation;
use fixtures::*;

#[test]
fn native_query_prepares_actual_core_lifecycle_and_one_moved_metadata_owner() -> TestResult {
    let (directory, store, captured) = setup("prepared-data", &data(true, b"p")?)?;
    let pointer = captured
        .record
        .capture
        .as_ref()
        .ok_or("capture")?
        .metadata
        .as_ptr();
    let control = captured.record.control;
    let result = prepare(captured, &context(), preparation_limits())?;
    assert_eq!(result.control(), control);
    let record = result.record().ok_or("prepared record")?;
    assert_eq!(record.native_record().record.state(), b"q");
    assert_eq!(
        record.captured_metadata().document().encoded().as_ptr(),
        pointer
    );
    assert_eq!(record.logical_partition(), Some(&b"p"[..]));
    assert_eq!(record.scope_prefix(), None);
    assert_eq!(record.value_binding().lifecycle.birth.max_active_ms(), None);
    assert_eq!(
        record
            .value_binding()
            .lifecycle
            .birth
            .history_hot_max_events(),
        2
    );
    assert_eq!(record.indexed_attributes().count(), 0);
    assert_eq!(record.indexed_state_metadata(), None);
    assert_eq!(record.policy_generation(), None);
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_query_refuses_unknown_routing_instead_of_inventing_an_unscoped_capture() -> TestResult {
    let (directory, store, captured) = setup("prepared-unknown", &data(false, b"p")?)?;
    assert!(matches!(
        prepare(captured, &context(), preparation_limits()),
        Err(E::UnknownRouting)
    ));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_query_charges_existing_native_read_owners_before_decoding_metadata() -> TestResult {
    let (directory, store, captured) = setup("prepared-existing-owners", &data(true, b"p")?)?;
    let mut cap = preparation_limits();
    cap.query.maximum_temporary_bytes = 1;
    assert!(matches!(prepare(captured, &context(), cap), Err(E::Limit)));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_query_uses_committed_complete_index_configuration_and_current_generation() -> TestResult {
    use wf_domain::policy::definition::{FieldPatch, PolicyPatch};
    let (directory, mut store) = setup_indexed("prepared-complete-policy")?;
    crate::workflow_values::fixtures::install_complete(
        &mut store,
        &crate::fixtures::scope()?,
        PolicyPatch {
            max_active_ms: FieldPatch::Set(9000),
            ..PolicyPatch::default()
        },
        Some(1),
        1,
        110,
    )?;
    let result = prepare(reread(&store, b"i")?, &context(), preparation_limits())?;
    let record = result.record().ok_or("prepared record")?;
    assert_eq!(record.policy_profile(), 1);
    assert_eq!(record.policy_generation(), Some(2));
    assert_eq!(
        record.indexed_attributes().collect::<Vec<_>>(),
        [&b"customer"[..], &b"region"[..]]
    );
    assert_eq!(record.indexed_state_metadata(), Some(&b"attempt"[..]));
    assert_eq!(
        record.value_binding().lifecycle.birth.max_active_ms(),
        Some(5000)
    );
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_query_uses_committed_original_policy_without_installing_raw_metadata_hints() -> TestResult
{
    let (directory, mut store, _) = setup("prepared-original-policy", &data(true, b"p")?)?;
    let policy = crate::workflow_values::fixtures::original_policy(1)?;
    crate::workflow_values::fixtures::install_original(
        &mut store,
        &crate::fixtures::scope()?,
        policy.encoded(),
        None,
        1,
        110,
    )?;
    let result = prepare(reread(&store, b"i")?, &context(), preparation_limits())?;
    let record = result.record().ok_or("prepared record")?;
    assert_eq!(record.policy_profile(), 2);
    assert_eq!(record.policy_generation(), Some(1));
    assert_eq!(record.indexed_attributes().count(), 0);
    assert_eq!(record.indexed_state_metadata(), None);
    assert_eq!(record.value_binding().lifecycle.birth.max_active_ms(), None);
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
