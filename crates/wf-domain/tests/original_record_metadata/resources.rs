use super::{fixtures as f, *};
#[test]
fn preparation_pays_control_work_before_shared_codec_ownership() {
    let input = f::empty();
    let mut limits = f::encode_limits();
    limits.max_work_units = 1;
    assert!(matches!(
        prepare(input.input(), None, limits),
        Err(E::Limit)
    ));
}

#[test]
fn exact_combined_work_and_temporary_boundaries_are_inclusive() -> TestResult {
    let input = f::empty();
    let prepared = prepare(input.input(), None, f::encode_limits())?;
    let mut limits = f::encode_limits();
    limits.max_work_units = prepared.work_units();
    limits.max_temporary_bytes = prepared.temporary_bytes();
    let exact = prepare(input.input(), None, limits)?;
    assert_eq!(exact.as_stored_bytes(), prepared.as_stored_bytes());
    limits.max_work_units -= 1;
    assert!(f::is_limit(
        &prepare(input.input(), None, limits)
            .err()
            .ok_or("work accepted")?
    ));
    limits.max_work_units += 1;
    limits.max_temporary_bytes -= 1;
    assert!(f::is_limit(
        &prepare(input.input(), None, limits)
            .err()
            .ok_or("temporary accepted")?
    ));
    Ok(())
}

#[test]
fn encoder_cannot_reset_the_budget_after_schema_validation() -> TestResult {
    let input = f::empty();
    let ordinary =
        wf_domain::value::prepare_original_flow_value(input.input(), f::encode_limits())?;
    let mut limits = f::encode_limits();
    limits.max_work_units = ordinary.work_units();
    assert!(f::is_limit(
        &prepare(input.input(), None, limits)
            .err()
            .ok_or("separate budgets admitted")?
    ));
    Ok(())
}

#[test]
fn shared_decoder_and_field_scan_share_the_same_inclusive_work_account() -> TestResult {
    let bytes = prepare(f::empty().input(), None, f::encode_limits())?.into_stored_bytes();
    let baseline = decode(bytes.clone(), &f::context(), f::limits())?;
    let mut limits = f::limits();
    limits.max_work_units = baseline.work_units();
    assert_eq!(
        decode(bytes.clone(), &f::context(), limits)?.work_units(),
        baseline.work_units()
    );
    limits.max_work_units -= 1;
    assert!(f::is_limit(
        &decode(bytes, &f::context(), limits)
            .err()
            .ok_or("work accepted")?
    ));
    Ok(())
}

#[test]
fn input_output_spines_and_depth_each_keep_their_own_ceiling() -> TestResult {
    let input = f::empty();
    let prepared = prepare(input.input(), None, f::encode_limits())?;
    let mut limits = f::encode_limits();
    limits.max_input_bytes = input.bytes.len() - 1;
    assert!(f::is_limit(
        &prepare(input.input(), None, limits)
            .err()
            .ok_or("input accepted")?
    ));
    let mut limits = f::encode_limits();
    limits.max_nodes = input.nodes.len() - 1;
    assert!(f::is_limit(
        &prepare(input.input(), None, limits)
            .err()
            .ok_or("nodes accepted")?
    ));
    let mut limits = f::encode_limits();
    limits.max_edges = input.edges.len() - 1;
    assert!(f::is_limit(
        &prepare(input.input(), None, limits)
            .err()
            .ok_or("edges accepted")?
    ));
    let mut limits = f::encode_limits();
    limits.max_depth = 1;
    assert!(f::is_limit(
        &prepare(input.input(), None, limits)
            .err()
            .ok_or("depth accepted")?
    ));
    let mut limits = f::encode_limits();
    limits.max_encoded_bytes = prepared.as_stored_bytes().len() - 1;
    assert!(f::is_limit(
        &prepare(input.input(), None, limits)
            .err()
            .ok_or("output accepted")?
    ));
    Ok(())
}

#[test]
fn moved_capacity_and_decoded_spines_are_admitted_before_returning_fields() -> TestResult {
    let bytes = prepare(f::empty().input(), None, f::encode_limits())?.into_stored_bytes();
    let mut limits = f::limits();
    limits.max_encoded_bytes = bytes.len() - 1;
    assert!(f::is_limit(
        &decode(bytes.clone(), &f::context(), limits)
            .err()
            .ok_or("bytes accepted")?
    ));
    let mut spare = Vec::with_capacity(bytes.len() + 1024);
    spare.extend_from_slice(&bytes);
    let mut limits = f::limits();
    limits.max_encoded_bytes = bytes.len();
    assert!(f::is_limit(
        &decode(spare, &f::context(), limits)
            .err()
            .ok_or("capacity accepted")?
    ));
    let mut limits = f::limits();
    limits.max_nodes = 1;
    assert!(f::is_limit(
        &decode(bytes.clone(), &f::context(), limits)
            .err()
            .ok_or("nodes accepted")?
    ));
    let mut limits = f::limits();
    limits.max_edges = 1;
    assert!(f::is_limit(
        &decode(bytes.clone(), &f::context(), limits)
            .err()
            .ok_or("edges accepted")?
    ));
    let mut limits = f::limits();
    limits.max_depth = 1;
    assert!(f::is_limit(
        &decode(bytes, &f::context(), limits)
            .err()
            .ok_or("depth accepted")?
    ));
    Ok(())
}

#[test]
fn zero_operational_limits_are_not_reported_as_missing_metadata() -> TestResult {
    let input = f::empty();
    let mut limits = f::encode_limits();
    limits.max_work_units = 0;
    assert!(matches!(
        prepare(input.input(), None, limits),
        Err(E::InvalidLimits)
    ));
    let bytes = prepare(input.input(), None, f::encode_limits())?.into_stored_bytes();
    let mut limits = f::limits();
    limits.max_nodes = 0;
    assert!(matches!(
        decode(bytes, &f::context(), limits),
        Err(E::InvalidLimits)
    ));
    Ok(())
}
