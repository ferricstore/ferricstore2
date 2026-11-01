//! Whole-call endpoints and nonrefundable reached-stage admission.
use super::*;
use wf_application::queries::StrongWorkflowRecordValueError as E;
#[test]
fn record_values_whole_output_work_and_peak_boundaries_are_inclusive() -> TestResult {
    let mut fixture = HydrationFixture::new("record-values-limits", Some(b"data"))?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let baseline = hydrated(
        &mut fixture,
        request_values(&scope, &id, true, 8)?,
        &context(),
        value_limits(),
    )?;
    for dimension in 0..3 {
        let endpoint = match dimension {
            0 => baseline.owned_bytes(),
            1 => baseline.work_units(),
            _ => baseline.temporary_bytes(),
        };
        for (cap, accepted) in [
            (endpoint, true),
            (endpoint.checked_sub(1).ok_or("endpoint")?, false),
            (0, false),
        ] {
            let mut selected = value_limits();
            match dimension {
                0 => selected.maximum_output_bytes = cap,
                1 => selected.maximum_work_units = cap,
                _ => selected.maximum_temporary_bytes = cap,
            }
            let result = hydrated(
                &mut fixture,
                request_values(&scope, &id, true, 8)?,
                &context(),
                selected,
            );
            assert_eq!(
                result.is_ok(),
                accepted,
                "dimension {dimension} cap {cap}: {result:?}"
            );
        }
    }
    fixture.finish()
}
#[test]
fn record_values_stage_limits_refuse_before_value_acquisition_and_skipped_stages_are_free()
-> TestResult {
    let mut fixture = HydrationFixture::new("record-values-stages", Some(b"data"))?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut selected = value_limits();
    selected.values.maximum_work = 0;
    assert!(matches!(
        hydrated(
            &mut fixture,
            request_values(&scope, &id, true, 8)?,
            &context(),
            selected
        ),
        Err(E::InvalidLimits)
    ));
    assert_eq!(fixture.value_reads, 0);
    selected.projection.maximum_work_units = 0;
    selected.conversion.maximum_work = 0;
    assert!(
        hydrated(
            &mut fixture,
            request_values(&scope, &id, false, 8)?,
            &context(),
            selected
        )?
        .value_writes()
        .is_empty()
    );
    assert_eq!(fixture.value_reads, 0);
    fixture.finish()
}
#[test]
fn record_values_stored_byte_limit_is_an_operational_error_not_semantic_omission() -> TestResult {
    use wf_ports::state::{StateTransactionError as S, StrongWorkflowRecordReadError as R};
    let mut fixture = HydrationFixture::new("record-values-read-limit", Some(b"data"))?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let mut selected = value_limits();
    selected.values.maximum_stored_bytes = 1;
    assert!(matches!(
        hydrated(
            &mut fixture,
            request_values(&scope, &id, true, 8)?,
            &context(),
            selected
        ),
        Err(E::Read(R::State(S::Limit)))
    ));
    fixture.finish()
}
