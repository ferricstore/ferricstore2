//! Native routing, captured field and actual policy controls.
use super::*;
use wf_domain::metadata::record::OriginalRecordMetadataField as F;

#[test]
fn native_query_keeps_actual_absence_distinct_from_a_missing_policy() -> TestResult {
    let (directory, store, _) = setup("prepared-absence", &data(true, b"p")?)?;
    let missing = reread(&store, b"missing")?;
    let result = prepare(missing, &context(), preparation_limits())?;
    assert!(result.record().is_none());
    let mut inconsistent = reread(&store, b"missing")?;
    inconsistent.current_policy = wf_ports::state::WorkflowRecordPolicyObservation::Absent;
    assert!(matches!(
        prepare(inconsistent, &context(), preparation_limits()),
        Err(E::InvalidCapture)
    ));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_query_preserves_missing_optional_user_metadata_without_empty_defaults() -> TestResult {
    let bytes = capture(&[
        (b"partition_key", binary(b"p")?),
        (b"scope_prefix", vec![119, 3, b'n', b'i', b'l']),
    ])?;
    let (directory, store, captured) = setup("prepared-optional-missing", &bytes)?;
    let result = prepare(captured, &context(), preparation_limits())?;
    let fields = result.record().ok_or("record")?.captured_metadata();
    for field in [
        F::Attributes,
        F::StateMeta,
        F::Root,
        F::Parent,
        F::Correlation,
    ] {
        assert!(fields.field(field).is_none());
    }
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_query_rejects_invalid_optional_classes_and_wrong_physical_routing() -> TestResult {
    for (index, extra) in [
        (b"attributes".as_slice(), vec![119, 3, b'n', b'i', b'l']),
        (b"state_meta".as_slice(), map(&[(b"q", vec![97, 1])])?),
        (b"parent_flow_id".as_slice(), vec![97, 1]),
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = capture(&[
            (b"partition_key", binary(b"p")?),
            (b"scope_prefix", vec![119, 3, b'n', b'i', b'l']),
            extra,
        ])?;
        let (directory, store, captured) = setup(&format!("prepared-invalid-{index}"), &bytes)?;
        assert!(prepare(captured, &context(), preparation_limits()).is_err());
        drop(store);
        std::fs::remove_dir_all(directory)?;
    }
    let (directory, store, captured) = setup("prepared-wrong-route", &data(true, b"other")?)?;
    assert!(matches!(
        prepare(captured, &context(), preparation_limits()),
        Err(E::RoutingMismatch)
    ));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_query_requires_explicit_decode_context_and_valid_policy_backing() -> TestResult {
    let (directory, store, captured) = setup("prepared-context-policy", &data(true, b"p")?)?;
    let mut missing_context = context();
    missing_context.safe_atoms = None;
    assert!(matches!(
        prepare(captured, &missing_context, preparation_limits()),
        Err(E::Metadata(_))
    ));
    let mut malformed = reread(&store, b"i")?;
    malformed.current_policy = wf_ports::state::WorkflowRecordPolicyObservation::Present(
        wf_domain::policy::complete::StoredPolicyValue::Original(vec![131, 97, 1]),
    );
    malformed.record.returned_bytes = malformed
        .record
        .returned_bytes
        .checked_add(3)
        .ok_or("bytes")?;
    assert!(matches!(
        prepare(malformed, &context(), preparation_limits()),
        Err(E::Policy(_))
    ));
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_query_ordinary_input_preserves_sequences_larger_than_legacy_u64() -> TestResult {
    let (directory, store, mut captured) = setup("prepared-wide-sequence", &data(true, b"p")?)?;
    // Ordinary typed-input control, not a claim of persisted source-origin coverage.
    let wide = u128::from(u64::MAX) + 1;
    let record = captured.record.capture.as_mut().ok_or("capture")?;
    record.record.ordering.state_enter_seq = wide;
    record.values.lifecycle.incarnation = wide;
    let result = prepare(captured, &context(), preparation_limits())?;
    let record = result.record().ok_or("record")?;
    assert_eq!(record.native_record().ordering.state_enter_seq, wide);
    assert_eq!(record.value_binding().lifecycle.incarnation, wide);
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn native_query_scoped_ordinary_input_checks_both_routing_directions() -> TestResult {
    use wf_domain::metadata::scope::{OriginalScopeLimits, original_physical_partition};
    let scope = b"ns";
    let physical = original_physical_partition(
        b"p",
        Some(scope),
        OriginalScopeLimits {
            maximum_output_bytes: 65_536,
            maximum_work_units: 1_048_576,
            maximum_temporary_bytes: 1_048_576,
        },
    )
    .map_err(|e| format!("scope preparation: {e:?}"))?
    .value
    .into_owned();
    for (label, logical, accepted) in [
        ("valid", b"p".as_slice(), true),
        ("already-scoped", physical.as_slice(), false),
    ] {
        let bytes = capture(&[
            (b"partition_key", binary(logical)?),
            (b"scope_prefix", binary(scope)?),
        ])?;
        let (directory, store, mut captured) = setup(&format!("prepared-scoped-{label}"), &bytes)?;
        // Explicit ordinary-input transformation; no native publication is claimed.
        let record = captured.record.capture.as_mut().ok_or("capture")?;
        let old = record.record.partition.as_bytes().len();
        record.record.partition = PartitionId::new(physical.clone())?;
        captured.record.returned_bytes = captured
            .record
            .returned_bytes
            .checked_sub(old)
            .and_then(|n| n.checked_add(physical.len()))
            .ok_or("owners")?;
        let result = prepare(captured, &context(), preparation_limits());
        if accepted {
            let result = result?;
            let record = result.record().ok_or("record")?;
            assert_eq!(record.logical_partition(), Some(&b"p"[..]));
            assert_eq!(record.scope_prefix(), Some(scope.as_slice()));
        } else {
            assert!(matches!(result, Err(E::RoutingMismatch)));
        }
        drop(store);
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}
