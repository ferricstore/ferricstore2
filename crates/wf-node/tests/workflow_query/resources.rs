//! Whole-parent and independent native-read ceilings remain continuous.
use super::*;

#[test]
fn strong_record_query_whole_output_work_and_peak_limits_are_inclusive() -> TestResult {
    let mut fixture = QueryFixture::new("strong-query-limits")?;
    fixture.create()?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let baseline = query(
        &mut fixture,
        request(&scope, &id, None),
        &context(),
        limits(),
    )?;
    for dimension in 0..3 {
        let endpoint = match dimension {
            0 => baseline.owned_bytes(),
            1 => baseline.work_units(),
            _ => baseline.temporary_bytes(),
        };
        for (cap, success) in [
            (endpoint, true),
            (endpoint.checked_sub(1).ok_or("endpoint")?, false),
            (0, false),
        ] {
            let mut selected = limits();
            match dimension {
                0 => selected.preparation.query.maximum_output_bytes = cap,
                1 => selected.preparation.query.maximum_work_units = cap,
                _ => selected.preparation.query.maximum_temporary_bytes = cap,
            }
            let result = query(
                &mut fixture,
                request(&scope, &id, None),
                &context(),
                selected,
            );
            assert_eq!(
                result.is_ok(),
                success,
                "dimension {dimension}, bound {cap}: {result:?}"
            );
        }
    }
    fixture.finish()
}

#[test]
fn strong_record_query_input_limit_precedes_any_authorized_read() -> TestResult {
    let mut fixture = QueryFixture::new("strong-query-input")?;
    fixture.create()?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    let total = scope.tenant().as_bytes().len()
        + scope.group().as_bytes().len()
        + scope.recovery().as_bytes().len()
        + id.as_bytes().len()
        + 1;
    let mut selected = limits();
    selected.maximum_input_bytes = total;
    assert!(
        query(
            &mut fixture,
            request(&scope, &id, None),
            &context(),
            selected
        )
        .is_ok()
    );
    let before = fixture.reads;
    selected.maximum_input_bytes = total.checked_sub(1).ok_or("total")?;
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &id, None),
            &context(),
            selected
        ),
        Err(E::Limit)
    ));
    selected.maximum_input_bytes = 0;
    assert!(matches!(
        query(
            &mut fixture,
            request(&scope, &id, None),
            &context(),
            selected
        ),
        Err(E::InvalidLimits)
    ));
    assert_eq!(fixture.reads, before);
    fixture.finish()
}

#[test]
fn strong_record_query_keeps_independent_native_read_refusals() -> TestResult {
    use wf_ports::state::{StateTransactionError as S, StrongWorkflowRecordReadError as R};
    let mut fixture = QueryFixture::new("strong-query-native-limits")?;
    fixture.create()?;
    let scope = f::scope()?;
    let id = RecordId::new(b"i".to_vec())?;
    for dimension in 0..5 {
        let mut selected = limits();
        let r = &mut selected.read.record;
        match dimension {
            0 => r.maximum_row_reads = 1,
            1 => r.maximum_inspected_bytes = 1,
            2 => r.maximum_returned_bytes = 1,
            3 => r.maximum_work = 1,
            _ => r.maximum_temporary_bytes = 1,
        }
        assert!(matches!(
            query(
                &mut fixture,
                request(&scope, &id, None),
                &context(),
                selected
            ),
            Err(E::Read(R::State(S::Limit)))
        ));
    }
    assert_eq!(fixture.reads, 5);
    fixture.finish()
}
