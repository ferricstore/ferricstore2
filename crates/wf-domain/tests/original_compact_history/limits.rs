use super::{C, E, L, TestResult, decode, fixtures as f, prepare};

#[test]
fn compact_history_encoding_admits_exact_resources() -> TestResult {
    let snapshot = f::base(&[]);
    let encoded = f::healthy(&snapshot, f::limits())?;
    let exact = L {
        max_encoded_bytes: encoded.as_bytes().len(),
        max_work_units: encoded.work_units(),
        max_temporary_bytes: encoded.temporary_bytes(),
        ..f::limits()
    };
    assert!(prepare(&snapshot, exact).is_ok());
    for limited in [
        L {
            max_encoded_bytes: exact.max_encoded_bytes - 1,
            ..f::limits()
        },
        L {
            max_work_units: exact.max_work_units - 1,
            ..f::limits()
        },
        L {
            max_temporary_bytes: exact.max_temporary_bytes - 1,
            ..f::limits()
        },
        L {
            max_input_bytes: 1,
            ..f::limits()
        },
    ] {
        assert!(matches!(prepare(&snapshot, limited), Err(E::Limit)));
    }
    for invalid in [
        L {
            max_input_bytes: 0,
            ..f::limits()
        },
        L {
            max_metadata_pairs: 0,
            ..f::limits()
        },
        L {
            max_encoded_bytes: 0,
            ..f::limits()
        },
        L {
            max_output_bytes: 0,
            ..f::limits()
        },
        L {
            max_work_units: 0,
            ..f::limits()
        },
        L {
            max_temporary_bytes: 0,
            ..f::limits()
        },
    ] {
        assert!(matches!(prepare(&snapshot, invalid), Err(E::InvalidLimits)));
    }
    Ok(())
}

#[test]
fn compact_history_decoding_admits_exact_resources() -> TestResult {
    let encoded = f::healthy(&f::base(&[]), f::limits())?;
    let decoded = f::decoded(encoded.as_bytes(), f::context(), f::limits())?;
    let length: usize = decoded
        .fields()
        .iter()
        .map(|field| field.key.len() + field.value.as_deref().map_or(0, <[u8]>::len))
        .sum();
    let exact = L {
        max_encoded_bytes: encoded.as_bytes().len(),
        max_output_bytes: length,
        max_work_units: decoded.work_units(),
        max_temporary_bytes: decoded.temporary_bytes(),
        ..f::limits()
    };
    assert!(decode(encoded.as_bytes(), f::context(), exact).is_ok());
    for limited in [
        L {
            max_encoded_bytes: exact.max_encoded_bytes - 1,
            ..f::limits()
        },
        L {
            max_output_bytes: exact.max_output_bytes - 1,
            ..f::limits()
        },
        L {
            max_work_units: exact.max_work_units - 1,
            ..f::limits()
        },
        L {
            max_temporary_bytes: exact.max_temporary_bytes - 1,
            ..f::limits()
        },
        L {
            max_input_bytes: 1,
            ..f::limits()
        },
    ] {
        assert!(matches!(
            decode(encoded.as_bytes(), f::context(), limited),
            Err(E::Limit)
        ));
    }
    assert!(matches!(
        decode(
            encoded.as_bytes(),
            C::default(),
            L {
                max_work_units: 0,
                ..f::limits()
            }
        ),
        Err(E::InvalidLimits)
    ));
    let corrupt = f::decoded(
        b"FSH2\x01\x02e\x02\x01\x02s\0",
        C::default(),
        L {
            max_output_bytes: 1,
            ..f::limits()
        },
    )?;
    assert!(corrupt.fields().is_empty());
    Ok(())
}
