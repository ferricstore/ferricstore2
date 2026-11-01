//! Supplemental original error order, large selected maxima and zero boundaries.
use super::*;
#[test]
fn payload_return_preserves_flag_and_integer_error_order() -> TestResult {
    let input = [
        ("full", R::Nil),
        ("payload", R::Nil),
        ("payload_max_bytes", R::Integer(-1)),
    ];
    assert_eq!(
        payload(&input, false, R::Integer(8))
            .err()
            .ok_or("full error")?
            .to_string(),
        "ERR flow full must be a boolean"
    );
    assert_eq!(
        payload(&input[1..], false, R::Integer(8))
            .err()
            .ok_or("payload error")?
            .to_string(),
        "ERR flow payload must be a boolean"
    );
    assert_eq!(
        payload(&input[2..], false, R::Integer(8))
            .err()
            .ok_or("max error")?
            .to_string(),
        "ERR flow payload_max_bytes must be a non-negative integer"
    );
    assert_eq!(
        history(
            &[("values", R::Nil), ("payload_max_bytes", R::Integer(-1))],
            R::Integer(8)
        )
        .err()
        .ok_or("values error")?
        .to_string(),
        "ERR flow values must be a boolean"
    );
    Ok(())
}
#[test]
fn payload_return_keeps_huge_configuration_distinct_from_the_requested_exact_integer_ceiling()
-> TestResult {
    let huge = R::BigInteger {
        negative: false,
        little_endian: &[0, 0, 0, 0, 0, 0, 0, 0, 1],
    };
    assert_eq!(
        payload(&[("payload_max_bytes", R::Integer(8))], true, huge)?.maximum_bytes(),
        8
    );
    assert_eq!(
        payload(&[], true, huge)
            .err()
            .ok_or("default ceiling")?
            .to_string(),
        "ERR flow payload_max_bytes exceeds maximum 9007199254740991"
    );
    assert_eq!(
        payload(
            &[("payload_max_bytes", R::Integer(9_007_199_254_740_992))],
            true,
            R::Integer(8)
        )
        .err()
        .ok_or("ceiling")?
        .to_string(),
        "ERR flow payload_max_bytes exceeds maximum 9007199254740991"
    );
    Ok(())
}
#[test]
fn payload_return_zero_and_shadowed_options_keep_source_meaning() -> TestResult {
    let result = payload(
        &[
            ("payload_max_bytes", R::Integer(0)),
            ("payload_max_bytes", R::Nil),
        ],
        true,
        R::Integer(0),
    )?;
    assert!(result.enabled());
    assert_eq!(result.maximum_bytes(), 0);
    assert_eq!(
        result.value_return(),
        wf_domain::history::values::HistoryValueReturn::Enabled { max_bytes: 0 }
    );
    assert_eq!(
        history(&[("full", R::Nil), ("payload", R::Nil)], R::Integer(8))?.value_return(),
        wf_domain::history::values::HistoryValueReturn::Disabled
    );
    Ok(())
}
