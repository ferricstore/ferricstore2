use super::{fixtures as f, *};

#[test]
fn full_selected_reference_and_relocation_caps_have_inclusive_endpoints() -> TestResult {
    let key = f::key()?;
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    for mode in 0..3 {
        let run = |l| match mode {
            0 => decode(encoded.encoded(), &key, None, l),
            1 => query(
                encoded.encoded(),
                &key,
                0,
                Plan {
                    attributes: Selection::None,
                    state_meta: StateSelection::None,
                },
                l,
            ),
            _ => reference(encoded.encoded(), &key, 0, l),
        };
        let measured = run(limits()).map_err(|e| format!("{e:?}"))?;
        let exact = RL {
            maximum_input_bytes: encoded.encoded().len() + key.len(),
            maximum_work_units: measured.work_units(),
            maximum_temporary_bytes: measured.temporary_bytes(),
            ..limits()
        };
        assert!(run(exact).is_ok());
        for l in [
            RL {
                maximum_input_bytes: exact.maximum_input_bytes - 1,
                ..exact
            },
            RL {
                maximum_work_units: exact.maximum_work_units - 1,
                ..exact
            },
            RL {
                maximum_temporary_bytes: exact.maximum_temporary_bytes - 1,
                ..exact
            },
        ] {
            assert!(matches!(run(l), Err(E::Limit)));
        }
    }
    let old = location(None);
    let new = Location {
        source_id: 4,
        ..old
    };
    let measured =
        relocate(encoded.encoded(), &key, &old, &new, limits()).map_err(|e| format!("{e:?}"))?;
    let exact = RL {
        maximum_input_bytes: encoded.encoded().len() + key.len(),
        maximum_output_bytes: measured.encoded().len(),
        maximum_work_units: measured.work_units(),
        maximum_temporary_bytes: measured.temporary_bytes(),
    };
    assert!(relocate(encoded.encoded(), &key, &old, &new, exact).is_ok());
    for l in [
        RL {
            maximum_input_bytes: exact.maximum_input_bytes - 1,
            ..exact
        },
        RL {
            maximum_output_bytes: exact.maximum_output_bytes - 1,
            ..exact
        },
        RL {
            maximum_work_units: exact.maximum_work_units - 1,
            ..exact
        },
        RL {
            maximum_temporary_bytes: exact.maximum_temporary_bytes - 1,
            ..exact
        },
    ] {
        assert!(matches!(
            relocate(encoded.encoded(), &key, &old, &new, l),
            Err(E::Limit)
        ));
    }
    assert!(matches!(
        decode(
            encoded.encoded(),
            &key,
            None,
            RL {
                maximum_input_bytes: 0,
                ..limits()
            }
        ),
        Err(E::InvalidLimits)
    ));
    Ok(())
}

#[test]
fn skipped_dynamic_fields_do_not_allocate_output_descriptors() -> TestResult {
    let key = f::key()?;
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let full = decode(encoded.encoded(), &key, None, limits()).map_err(|e| format!("{e:?}"))?;
    let lean = query(
        encoded.encoded(),
        &key,
        0,
        Plan {
            attributes: Selection::None,
            state_meta: StateSelection::None,
        },
        limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert!(lean.temporary_bytes() < full.temporary_bytes());
    let r = row(lean)?;
    assert_eq!(r.state_key().as_ptr(), key.as_ptr());
    let r = r.record().ok_or("record")?;
    assert!(r.attributes().is_none());
    assert!(r.state_meta().is_none());
    Ok(())
}

#[test]
fn expired_rows_validate_without_allocating_discarded_dynamic_results() -> TestResult {
    let key = f::key()?;
    let loc = f::replace(f::LOCATOR, "expire_at_ms", I::Integer(2000));
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        Locator {
            fields: I::Map(&loc),
        },
        I::Integer(2000),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let active =
        decode(encoded.encoded(), &key, Some(1999), limits()).map_err(|e| format!("{e:?}"))?;
    let expired =
        decode(encoded.encoded(), &key, Some(2000), limits()).map_err(|e| format!("{e:?}"))?;
    assert!(expired.is_expired());
    assert!(expired.temporary_bytes() < active.temporary_bytes());
    let exact = RL {
        maximum_work_units: expired.work_units(),
        maximum_temporary_bytes: expired.temporary_bytes(),
        ..limits()
    };
    assert!(decode(encoded.encoded(), &key, Some(2000), exact).is_ok_and(|r| r.is_expired()));
    assert!(matches!(
        decode(
            encoded.encoded(),
            &key,
            Some(2000),
            RL {
                maximum_work_units: exact.maximum_work_units - 1,
                ..exact
            }
        ),
        Err(E::Limit)
    ));
    Ok(())
}
