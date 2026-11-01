use super::{fixtures as f, *};

#[test]
fn required_identity_and_covering_numeric_boundaries_remain_exact() -> TestResult {
    let key = f::key()?;
    for (name, bad) in [
        ("id", I::Binary(b"foreign")),
        ("partition_key", I::Binary(b"")),
        ("version", I::Integer(9_007_199_254_740_992)),
        ("priority", I::Integer(i128::from(i64::MAX) + 1)),
        ("attempts", I::Boolean(false)),
        ("state", I::Boolean(false)),
        ("lease_owner", I::Integer(1)),
    ] {
        let rows = f::replace(f::RECORD, name, bad);
        assert!(matches!(
            prepare(I::Binary(&key), I::Map(&rows), f::limits()),
            Err(E::InvalidRecord)
        ));
    }
    let rows = f::replace(f::RECORD, "version", I::Integer(9_007_199_254_740_991));
    assert!(prepare(I::Binary(&key), I::Map(&rows), f::limits()).is_ok());
    let padded = [0, 0, 0, 0, 0, 0, 0, 128, 0, 0];
    let rows = f::replace(
        f::RECORD,
        "priority",
        I::BigInteger {
            negative: true,
            little_endian: &padded,
        },
    );
    assert!(prepare(I::Binary(&key), I::Map(&rows), f::limits()).is_ok());
    for field in ["id", "version", "partition_key"] {
        let rows: Vec<_> = f::RECORD
            .iter()
            .copied()
            .filter(|(key, _)| !matches!(key,I::Atom(s) if *s == field))
            .collect();
        assert!(prepare(I::Binary(&key), I::Map(&rows), f::limits()).is_err());
    }
    Ok(())
}

#[test]
fn source_size_and_invalid_errors_have_the_original_precedence() -> TestResult {
    let key = f::key()?;
    let large = vec![b'x'; 32769];
    for length in [32424, 32425, 32768, 32769] {
        let rows = f::replace(f::RECORD, "type", I::Binary(&large[..length]));
        let result = prepare(I::Binary(&key), I::Map(&rows), f::limits());
        if length == 32424 {
            assert_eq!(
                result
                    .map_err(|e| format!("{e:?}"))?
                    .encoded_metadata()
                    .len(),
                32629
            );
        } else if length == 32769 {
            assert!(matches!(result, Err(E::InvalidRecord)));
        } else {
            assert!(matches!(result, Err(E::MetadataTooLarge)));
        }
    }
    let rows = f::replace(f::RECORD, "type", I::Binary(&large[..32768]));
    let rows = f::replace(
        &rows,
        "attributes",
        I::Map(&[(I::Binary(b"bad"), I::List(&[]))]),
    );
    assert!(matches!(
        prepare(I::Binary(&key), I::Map(&rows), f::limits()),
        Err(E::InvalidRecord)
    ));
    assert_eq!(
        E::MetadataTooLarge.source_error(),
        Some("ERR flow query metadata exceeds 32629 bytes")
    );
    assert_eq!(E::Limit.source_error(), None);
    Ok(())
}

#[test]
fn omitted_null_and_invalid_dynamic_shapes_remain_distinct() -> TestResult {
    let key = f::key()?;
    for name in ["attributes", "state_meta"] {
        let rows: Vec<_> = f::RECORD
            .iter()
            .copied()
            .filter(|(key, _)| !matches!(key,I::Atom(s) if *s == name))
            .collect();
        assert!(prepare(I::Binary(&key), I::Map(&rows), f::limits()).is_ok());
        let rows = f::replace(f::RECORD, name, I::Nil);
        assert!(matches!(
            prepare(I::Binary(&key), I::Map(&rows), f::limits()),
            Err(E::InvalidRecord)
        ));
    }
    for value in [
        I::List(&[]),
        I::List(&[I::Binary(b"same"), I::Binary(b"same")]),
        I::List(&[I::Binary(b"ok"), I::Integer(1)]),
        I::Binary(&[b'x'; 257]),
    ] {
        let attributes = [(I::Binary(b"name"), value)];
        let rows = f::replace(f::RECORD, "attributes", I::Map(&attributes));
        assert!(prepare(I::Binary(&key), I::Map(&rows), f::limits()).is_err());
    }
    let rows = f::replace(
        f::RECORD,
        "state_meta",
        I::Map(&[(
            I::Binary(b"queued"),
            I::Map(&[(I::Binary(b"__reserved"), I::Integer(1))]),
        )]),
    );
    assert!(prepare(I::Binary(&key), I::Map(&rows), f::limits()).is_err());
    Ok(())
}

#[test]
fn indexed_configuration_normalizes_atoms_duplicates_and_nested_state_lists() -> TestResult {
    let key = f::key()?;
    let names = [
        I::Binary(b" tier "),
        I::Atom("tier"),
        I::Atom("label"),
        I::Binary(b"label"),
    ];
    let nested = [
        I::Nil,
        I::List(&[I::Binary(b"worker"), I::List(&[])]),
        I::Binary(b" worker "),
    ];
    let rows = f::replace(f::RECORD, "indexed_attributes", I::List(&names));
    let rows = f::replace(&rows, "indexed_state_meta", I::List(&nested));
    assert!(prepare(I::Binary(&key), I::Map(&rows), f::limits()).is_ok());
    let many = vec![I::Binary(b"tier"); 1000];
    let rows = f::replace(f::RECORD, "indexed_attributes", I::List(&many));
    assert!(prepare(I::Binary(&key), I::Map(&rows), f::limits()).is_ok());
    let four = [I::Atom("a"), I::Atom("b"), I::Atom("c"), I::Atom("d")];
    let rows = f::replace(f::RECORD, "indexed_attributes", I::List(&four));
    assert!(prepare(I::Binary(&key), I::Map(&rows), f::limits()).is_err());
    let distinct = [I::Binary(b"one"), I::List(&[I::Binary(b"two")])];
    let rows = f::replace(f::RECORD, "indexed_state_meta", I::List(&distinct));
    assert!(prepare(I::Binary(&key), I::Map(&rows), f::limits()).is_err());
    Ok(())
}

#[test]
fn original_scope_is_removed_from_public_partition_but_binds_the_state_key() -> TestResult {
    let scoped = wf_domain::metadata::scope::original_physical_partition(
        b"tenant-a",
        Some(&42_u64.to_be_bytes()),
        wf_domain::metadata::scope::OriginalScopeLimits {
            maximum_output_bytes: 100_000,
            maximum_work_units: 20_000_000,
            maximum_temporary_bytes: 2_000_000,
        },
    )
    .map_err(|e| format!("{e:?}"))?
    .value;
    let key = wf_ports::blob::render_original_state_key(
        b"run-1",
        wf_ports::blob::OriginalValuePartition::Explicit(&scoped),
        65_535,
    )
    .map_err(|e| format!("{e:?}"))?;
    let sealed = [
        I::Integer(1),
        I::Atom("uint64"),
        I::Atom("isolation_scope"),
        I::Integer(42),
    ];
    let metadata = [(I::Integer(0x8001), I::Tuple(&sealed))];
    let mut rows = f::replace(f::RECORD, "partition_key", I::Binary(&scoped));
    rows.push((I::Atom("system_metadata"), I::Map(&metadata)));
    let result =
        prepare(I::Binary(&key), I::Map(&rows), f::limits()).map_err(|e| format!("{e:?}"))?;
    assert_eq!(result.logical_partition(), Some(b"tenant-a".as_slice()));
    assert_eq!(result.scope_prefix(), Some(42_u64.to_be_bytes().as_slice()));
    let plain = f::key()?;
    assert!(prepare(I::Binary(&plain), I::Map(&rows), f::limits()).is_err());
    let rows = f::replace(&rows, "partition_key", I::Binary(b"tenant-a"));
    assert!(prepare(I::Binary(&key), I::Map(&rows), f::limits()).is_err());
    Ok(())
}

#[test]
fn large_root_identity_is_compacted_and_unrelated_payloads_are_not_copied() -> TestResult {
    let id = vec![b'x'; 65483];
    let key = wf_ports::blob::render_original_state_key(
        &id,
        wf_ports::blob::OriginalValuePartition::Explicit(b"tenant-a"),
        65_535,
    )
    .map_err(|e| format!("{e:?}"))?;
    let rows = f::replace(f::RECORD, "id", I::Binary(&id));
    let mut rows = f::replace(&rows, "root_flow_id", I::Binary(&id));
    let large = vec![b'x'; 1_000_000];
    rows.push((I::Atom("payload_ref"), I::Binary(&large)));
    rows.push((I::Atom("incarnation"), I::Tuple(&[I::Other])));
    let result =
        prepare(I::Binary(&key), I::Map(&rows), f::limits()).map_err(|e| format!("{e:?}"))?;
    assert_eq!(result.id().as_ptr(), id.as_ptr());
    assert!(result.encoded_metadata().len() < 210);
    assert_eq!(result.encoded_metadata().first(), Some(&1));
    Ok(())
}
