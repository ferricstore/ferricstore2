//! Whole-read resource endpoints and prospective shared-decoder/schema ceilings.
use super::*;

#[test]
fn native_query_parent_limits_are_inclusive_and_each_one_below_refuses() -> TestResult {
    let (directory, mut store, _) = setup("prepared-parent-limits", &data(true, b"p")?)?;
    endpoints(&store)?;
    let policy = crate::workflow_values::fixtures::original_policy(1)?;
    crate::workflow_values::fixtures::install_original(
        &mut store,
        &crate::fixtures::scope()?,
        policy.encoded(),
        None,
        1,
        110,
    )?;
    endpoints(&store)?;
    drop(store);
    std::fs::remove_dir_all(directory)?;
    let (directory, store) = setup_indexed("prepared-complete-parent-limits")?;
    endpoints(&store)?;
    drop(store);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn endpoints(store: &wf_storage::state::RedbStateStore) -> TestResult {
    let baseline = prepare(reread(store, b"i")?, &context(), preparation_limits())?;
    for dimension in 0..3 {
        let mut cap = preparation_limits();
        let endpoint = match dimension {
            0 => baseline.owned_bytes(),
            1 => baseline.work_units(),
            _ => baseline.temporary_bytes(),
        };
        for (bound, accepted) in [
            (endpoint, true),
            (endpoint.checked_sub(1).ok_or("endpoint")?, false),
            (0, false),
        ] {
            match dimension {
                0 => cap.query.maximum_output_bytes = bound,
                1 => cap.query.maximum_work_units = bound,
                _ => cap.query.maximum_temporary_bytes = bound,
            }
            let result = prepare(reread(store, b"i")?, &context(), cap);
            assert_eq!(
                result.is_ok(),
                accepted,
                "dimension {dimension}, bound {bound}: {result:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn native_query_bounded_metadata_decoder_preserves_old_work_and_exact_auxiliary_endpoint()
-> TestResult {
    use wf_domain::metadata::record::{
        decode_original_record_metadata as decode,
        decode_original_record_metadata_bounded as bounded,
    };
    let bytes = data(true, b"p")?;
    let limits = preparation_limits().metadata;
    let old = decode(bytes.clone(), &context(), limits)?;
    let (new, peak) = bounded(bytes.clone(), &context(), limits, usize::MAX)?;
    assert_eq!(old.work_units(), new.work_units());
    assert_eq!(old.document().encoded(), new.document().encoded());
    assert!(bounded(bytes.clone(), &context(), limits, peak).is_ok());
    assert!(
        bounded(
            bytes.clone(),
            &context(),
            limits,
            peak.checked_sub(1).ok_or("peak")?
        )
        .is_err()
    );
    assert!(bounded(bytes, &context(), limits, 0).is_err());
    Ok(())
}

#[test]
fn native_query_schema_scratch_refuses_before_callback_and_preserves_default_charges() -> TestResult
{
    use wf_domain::policy::complete::{
        PolicySchemaCharge, with_stored_policy_indexed_fields as inspect,
    };
    let donor = crate::workflow_values::fixtures::original_policy(1)?;
    let limits = preparation_limits().policy;
    let mut old = PolicySchemaCharge::new(limits);
    let expected = inspect(Some(&donor), b"r", limits, &mut old, |v| {
        (v.work_units, v.temporary_bytes)
    })?;
    assert!(expected.1 > 0);
    let mut bounded = PolicySchemaCharge::with_scratch_limit(limits, expected.1);
    assert_eq!(
        inspect(Some(&donor), b"r", limits, &mut bounded, |v| (
            v.work_units,
            v.temporary_bytes
        ))?,
        expected
    );
    let mut invoked = false;
    let mut short =
        PolicySchemaCharge::with_scratch_limit(limits, expected.1.checked_sub(1).ok_or("scratch")?);
    assert!(inspect(Some(&donor), b"r", limits, &mut short, |_| invoked = true).is_err());
    assert!(!invoked);
    let mut zero = PolicySchemaCharge::with_scratch_limit(limits, 0);
    assert_eq!(
        inspect(None, b"r", limits, &mut zero, |v| v.temporary_bytes)?,
        0
    );
    Ok(())
}
