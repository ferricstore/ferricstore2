use super::{fixtures as f, *};

#[test]
fn independent_metadata_and_row_resource_endpoints_are_inclusive() -> TestResult {
    let key = f::key()?;
    let value = prepare(I::Binary(&key), f::record(), f::limits()).map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        format!("{value:?}"),
        "PreparedOriginalQueryMetadata(<redacted>)"
    );
    let exact = L {
        maximum_output_bytes: value.encoded_metadata().len(),
        maximum_work_units: value.work_units(),
        maximum_temporary_bytes: value.temporary_bytes(),
    };
    assert!(prepare(I::Binary(&key), f::record(), exact).is_ok());
    for limits in [
        L {
            maximum_output_bytes: exact.maximum_output_bytes - 1,
            ..exact
        },
        L {
            maximum_work_units: exact.maximum_work_units - 1,
            ..exact
        },
        L {
            maximum_temporary_bytes: exact.maximum_temporary_bytes - 1,
            ..exact
        },
    ] {
        assert!(matches!(
            prepare(I::Binary(&key), f::record(), limits),
            Err(E::Limit)
        ));
    }
    let value = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let exact = L {
        maximum_output_bytes: value.encoded().len(),
        maximum_work_units: value.work_units(),
        maximum_temporary_bytes: value.temporary_bytes(),
    };
    assert!(
        encode(
            I::Binary(&key),
            f::record(),
            f::locator(),
            I::Integer(0),
            exact
        )
        .is_ok()
    );
    for limits in [
        L {
            maximum_output_bytes: exact.maximum_output_bytes - 1,
            ..exact
        },
        L {
            maximum_work_units: exact.maximum_work_units - 1,
            ..exact
        },
        L {
            maximum_temporary_bytes: exact.maximum_temporary_bytes - 1,
            ..exact
        },
    ] {
        assert!(matches!(
            encode(
                I::Binary(&key),
                f::record(),
                f::locator(),
                I::Integer(0),
                limits
            ),
            Err(E::Limit)
        ));
    }
    assert!(matches!(
        prepare(
            I::Binary(&key),
            f::record(),
            L {
                maximum_work_units: 0,
                ..exact
            }
        ),
        Err(E::InvalidLimits)
    ));
    Ok(())
}

#[test]
fn source_size_refusal_precedes_output_reservation_and_stack_work_is_bounded() -> TestResult {
    let key = f::key()?;
    let large = vec![b'x'; 32629];
    let rows = f::replace(f::RECORD, "type", I::Binary(&large));
    assert!(matches!(
        prepare(
            I::Binary(&key),
            I::Map(&rows),
            L {
                maximum_output_bytes: 1,
                ..f::limits()
            }
        ),
        Err(E::MetadataTooLarge)
    ));
    let nested = [
        I::List(&[I::List(&[I::Binary(b"worker")])]),
        I::List(&[I::Nil, I::Binary(b"worker")]),
    ];
    let rows = f::replace(f::RECORD, "indexed_state_meta", I::List(&nested));
    let value =
        prepare(I::Binary(&key), I::Map(&rows), f::limits()).map_err(|e| format!("{e:?}"))?;
    let exact = L {
        maximum_work_units: value.work_units(),
        maximum_temporary_bytes: value.temporary_bytes(),
        ..f::limits()
    };
    assert!(prepare(I::Binary(&key), I::Map(&rows), exact).is_ok());
    assert!(matches!(
        prepare(
            I::Binary(&key),
            I::Map(&rows),
            L {
                maximum_work_units: exact.maximum_work_units - 1,
                ..exact
            }
        ),
        Err(E::Limit)
    ));
    Ok(())
}
