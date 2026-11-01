use super::*;

#[test]
fn unsigned_scope_validates_system_fields_and_preserves_all_u64_values() -> Result<(), String> {
    for value in [0, 42, u64::MAX] {
        let sealed = [
            I::Integer(1),
            I::Atom("uint64"),
            I::Atom("isolation_scope"),
            I::Integer(i128::from(value)),
        ];
        let entries = [(I::Integer(0x8001), I::Tuple(&sealed))];
        let result = original_system_metadata_scope(I::Map(&entries), limits())
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(
            result.value.ok_or("expected source value")?,
            value.to_be_bytes()
        );
    }
    assert!(
        original_system_metadata_scope(I::Map(&[]), limits())
            .map_err(|error| format!("{error:?}"))?
            .value
            .is_none()
    );
    Ok(())
}

#[test]
fn compound_scope_sorts_ids_and_encodes_only_isolation_roles() -> Result<(), String> {
    let first = [
        I::Integer(1),
        I::Atom("int64"),
        I::Atom("isolation_scope"),
        I::Integer(-1),
    ];
    let second = [
        I::Integer(1),
        I::Atom("keyword"),
        I::Atom("isolation_scope"),
        I::Binary(b"tenant"),
    ];
    let excluded = [
        I::Integer(1),
        I::Atom("boolean"),
        I::Atom("system_metadata"),
        I::Boolean(true),
    ];
    let rows = [
        (I::Integer(9), I::Tuple(&second)),
        (I::Integer(2), I::Tuple(&excluded)),
        (I::Integer(1), I::Tuple(&first)),
    ];
    // Independent source `TermCodec` tuple/list ETF, including version and tail.
    let expected = b"\x83h\x02w\x14flow_isolation_scopel\0\0\0\x02h\x02a\x01h\x04a\x01w\x05int64w\x0fisolation_scopeb\xff\xff\xff\xffh\x02a\x09h\x04a\x01w\x07keywordw\x0fisolation_scopem\0\0\0\x06tenantj";
    assert_eq!(
        original_system_metadata_scope(I::Map(&rows), limits())
            .map_err(|error| format!("{error:?}"))?
            .value
            .ok_or("expected source value")?,
        expected
    );
    let reversed = [rows[2], rows[1], rows[0]];
    assert_eq!(
        original_system_metadata_scope(I::Map(&reversed), limits())
            .map_err(|error| format!("{error:?}"))?
            .value
            .ok_or("expected source value")?,
        expected
    );
    Ok(())
}

#[test]
fn versions_are_positive_arbitrary_integers_and_padding_is_mathematical() -> Result<(), String> {
    let huge_version = [1; 64];
    let padded_id = [1, 0, 0, 0];
    let negative_zero = [0; 4];
    let sealed = [
        I::BigInteger {
            negative: false,
            little_endian: &huge_version,
        },
        I::Atom("uint64"),
        I::Atom("isolation_scope"),
        I::BigInteger {
            negative: true,
            little_endian: &negative_zero,
        },
    ];
    let rows = [(
        I::BigInteger {
            negative: false,
            little_endian: &padded_id,
        },
        I::Tuple(&sealed),
    )];
    assert_eq!(
        original_system_metadata_scope(I::Map(&rows), limits())
            .map_err(|error| format!("{error:?}"))?
            .value
            .ok_or("expected source value")?,
        [0; 8]
    );
    Ok(())
}

#[test]
fn malformed_sealed_fields_are_rejected_even_when_not_isolation_roles() {
    for (kind, value) in [
        ("uint64", I::Integer(-1)),
        ("uint64", I::Integer(i128::from(u64::MAX) + 1)),
        ("int64", I::Integer(i128::from(i64::MAX) + 1)),
        ("datetime", I::Integer(i128::from(i64::MIN) - 1)),
        ("keyword", I::Binary(b"")),
        ("keyword", I::Binary(b"\xff")),
        ("boolean", I::Nil),
        ("unknown", I::Integer(1)),
    ] {
        let sealed = [
            I::Integer(1),
            I::Atom(kind),
            I::Atom("system_metadata"),
            value,
        ];
        let rows = [(I::Integer(1), I::Tuple(&sealed))];
        assert!(matches!(
            original_system_metadata_scope(I::Map(&rows), limits()),
            Err(OriginalScopeError::InvalidSystemMetadata)
        ));
    }
    for id in [0, -1, 65536] {
        let sealed = [
            I::Integer(1),
            I::Atom("boolean"),
            I::Atom("system_metadata"),
            I::Boolean(true),
        ];
        let rows = [(I::Integer(id), I::Tuple(&sealed))];
        assert!(matches!(
            original_system_metadata_scope(I::Map(&rows), limits()),
            Err(OriginalScopeError::InvalidSystemMetadata)
        ));
    }
    for version in [0, -1] {
        let sealed = [
            I::Integer(version),
            I::Atom("boolean"),
            I::Atom("system_metadata"),
            I::Boolean(true),
        ];
        let rows = [(I::Integer(1), I::Tuple(&sealed))];
        assert!(original_system_metadata_scope(I::Map(&rows), limits()).is_err());
    }
    let sealed = [
        I::Integer(1),
        I::Atom("boolean"),
        I::Atom("foreign_role"),
        I::Boolean(true),
    ];
    assert!(
        original_system_metadata_scope(I::Map(&[(I::Integer(1), I::Tuple(&sealed))]), limits())
            .is_err()
    );
    assert!(
        original_system_metadata_scope(I::Map(&[(I::Integer(1), I::List(&sealed))]), limits())
            .is_err()
    );
    assert!(
        original_system_metadata_scope(
            I::Map(&[(I::Integer(1), I::Tuple(&sealed[..3]))]),
            limits()
        )
        .is_err()
    );
    assert!(original_system_metadata_scope(I::Nil, limits()).is_err());
}

#[test]
fn encoded_metadata_ceiling_is_source_invalidity_at_exact_8192_boundary() -> Result<(), String> {
    // Two source rows cost109 bytes beyond their keyword binaries.
    let left = [b'x'; 4096];
    let right = [b'y'; 3988];
    let first = [
        I::Integer(1),
        I::Atom("keyword"),
        I::Atom("system_metadata"),
        I::Binary(&left),
    ];
    let second = [
        I::Integer(1),
        I::Atom("keyword"),
        I::Atom("system_metadata"),
        I::Binary(&right[..3987]),
    ];
    let rows = [
        (I::Integer(1), I::Tuple(&first)),
        (I::Integer(2), I::Tuple(&second)),
    ];
    assert!(
        original_system_metadata_scope(I::Map(&rows), limits())
            .map_err(|error| format!("{error:?}"))?
            .value
            .is_none()
    );
    let second = [second[0], second[1], second[2], I::Binary(&right)];
    let rows = [
        (I::Integer(1), I::Tuple(&first)),
        (I::Integer(2), I::Tuple(&second)),
    ];
    assert!(matches!(
        original_system_metadata_scope(I::Map(&rows), limits()),
        Err(OriginalScopeError::InvalidSystemMetadata)
    ));
    Ok(())
}

#[test]
fn all_sixteen_fields_and_non_u64_scope_types_use_the_existing_etf_path() -> Result<(), String> {
    let sealed = [
        I::Integer(1),
        I::Atom("boolean"),
        I::Atom("isolation_scope"),
        I::Atom("true"),
    ];
    let rows: Vec<_> = (1..=16)
        .map(|id| (I::Integer(id), I::Tuple(&sealed)))
        .collect();
    let value = original_system_metadata_scope(I::Map(&rows), limits())
        .map_err(|error| format!("{error:?}"))?
        .value
        .ok_or("expected source value")?;
    assert!(value.starts_with(b"\x83h\x02w\x14flow_isolation_scopel\0\0\0\x10"));
    let rows: Vec<_> = (1..=17)
        .map(|id| (I::Integer(id), I::Tuple(&sealed)))
        .collect();
    assert!(original_system_metadata_scope(I::Map(&rows), limits()).is_err());
    Ok(())
}
