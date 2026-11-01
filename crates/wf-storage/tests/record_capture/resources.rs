use super::{TestResult, fixtures::*};
use wf_storage::state::value::{
    RecordCaptureError as E, StateValueError as S, encode_record_capture, scan_record_capture,
};

#[test]
fn exact_independent_encode_resource_endpoints_pass() -> TestResult {
    let f = Fixture::new()?;
    let expected = independent_wire()?;
    let mut cap = limits();
    cap.maximum_encoded_bytes = expected.len();
    cap.maximum_metadata_bytes = f.metadata.as_stored_bytes().len();
    cap.maximum_work_units = 4096 + 128 * expected.len();
    cap.maximum_temporary_bytes = 4096 + expected.len();
    assert_eq!(encode_record_capture(f.input(), cap)?, expected);
    Ok(())
}
#[test]
fn borrowed_scan_needs_only_fixed_control_and_one_complete_work_account() -> TestResult {
    let f = Fixture::new()?;
    let bytes = independent_wire()?;
    let mut cap = limits();
    cap.maximum_work_units = 4096 + 128 * bytes.len();
    cap.maximum_temporary_bytes = 4096;
    let scanned = scan_record_capture(&bytes, f.point(), cap)?;
    assert_eq!(scanned.work_units(), cap.maximum_work_units);
    Ok(())
}

#[test]
fn encoded_and_metadata_caps_refuse_independently_one_below_inclusive_limits() -> TestResult {
    let f = Fixture::new()?;
    let bytes = independent_wire()?;
    let mut cap = limits();
    cap.maximum_encoded_bytes = bytes.len() - 1;
    assert!(matches!(
        encode_record_capture(f.input(), cap),
        Err(E::State(S::Limit { .. }))
    ));
    assert!(matches!(
        scan_record_capture(&bytes, f.point(), cap),
        Err(E::State(S::Limit { .. }))
    ));
    let mut cap = limits();
    cap.maximum_metadata_bytes = f.metadata.as_stored_bytes().len() - 1;
    assert!(matches!(
        encode_record_capture(f.input(), cap),
        Err(E::State(S::Limit { .. }))
    ));
    assert!(matches!(
        scan_record_capture(&bytes, f.point(), cap),
        Err(E::State(S::Limit { .. }))
    ));
    Ok(())
}

#[test]
fn work_and_temporary_caps_keep_distinct_units_and_exact_endpoints() -> TestResult {
    let f = Fixture::new()?;
    let bytes = independent_wire()?;
    let mut cap = limits();
    cap.maximum_work_units = 4096 + 128 * bytes.len() - 1;
    assert!(matches!(
        encode_record_capture(f.input(), cap),
        Err(E::WorkLimit)
    ));
    assert!(matches!(
        scan_record_capture(&bytes, f.point(), cap),
        Err(E::WorkLimit)
    ));
    let mut cap = limits();
    cap.maximum_temporary_bytes = 4096 + bytes.len() - 1;
    assert!(matches!(
        encode_record_capture(f.input(), cap),
        Err(E::TemporaryLimit)
    ));
    assert!(scan_record_capture(&bytes, f.point(), cap).is_ok());
    cap.maximum_temporary_bytes = 4095;
    assert!(matches!(
        scan_record_capture(&bytes, f.point(), cap),
        Err(E::TemporaryLimit)
    ));
    Ok(())
}

#[test]
fn zero_limits_are_invalid_before_any_processing() -> TestResult {
    let f = Fixture::new()?;
    let bytes = independent_wire()?;
    for field in 0..4 {
        let mut cap = limits();
        match field {
            0 => cap.maximum_encoded_bytes = 0,
            1 => cap.maximum_metadata_bytes = 0,
            2 => cap.maximum_work_units = 0,
            _ => cap.maximum_temporary_bytes = 0,
        }
        assert!(matches!(
            encode_record_capture(f.input(), cap),
            Err(E::InvalidLimits)
        ));
        assert!(matches!(
            scan_record_capture(&bytes, f.point(), cap),
            Err(E::InvalidLimits)
        ));
    }
    Ok(())
}

#[test]
fn core_admission_is_still_enforced_inside_complete_capture() -> TestResult {
    let f = Fixture::new()?;
    let bytes = independent_wire()?;
    let mut cap = limits();
    cap.admission = wf_domain::workflow::admission::AdmissionLimits::new(1, 1, 1024)?;
    assert!(encode_record_capture(f.input(), cap).is_err());
    assert!(scan_record_capture(&bytes, f.point(), cap).is_err());
    Ok(())
}

#[test]
fn large_metadata_has_its_own_cap_without_weakening_core_admission() -> TestResult {
    let f = Fixture::new()?;
    // The raw donor inside the preparation helper is gone before this call.
    let metadata = large_metadata(131_072)?;
    let mut input = f.input();
    input.metadata = &metadata;
    let mut cap = limits();
    cap.admission = wf_domain::workflow::admission::AdmissionLimits::new(3, 1024, 1)?;
    let bytes = encode_record_capture(input, cap)?;
    let scanned = scan_record_capture(&bytes, f.point(), cap)?;
    assert_eq!(scanned.metadata_bytes(), metadata.as_stored_bytes());
    assert!(scanned.metadata_bytes().len() > 131_072);
    cap.maximum_metadata_bytes = 131_072;
    assert!(matches!(
        encode_record_capture(input, cap),
        Err(E::State(S::Limit { .. }))
    ));
    assert!(matches!(
        scan_record_capture(&bytes, f.point(), cap),
        Err(E::State(S::Limit { .. }))
    ));
    Ok(())
}
