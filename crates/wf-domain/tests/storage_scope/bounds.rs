use super::*;

#[test]
fn record_nil_refuses_nonempty_unscoped_metadata_and_scoped_empty_tail_survives()
-> Result<(), String> {
    let sealed = [
        I::Integer(1),
        I::Atom("boolean"),
        I::Atom("system_metadata"),
        I::Boolean(true),
    ];
    let rows = [(I::Integer(1), I::Tuple(&sealed))];
    assert!(
        original_record_physical_partition(Some(I::Nil), Some(I::Map(&rows)), limits()).is_err()
    );
    assert!(
        original_record_logical_partition(Some(I::Nil), Some(I::Map(&rows)), limits()).is_err()
    );
    let sealed = [
        I::Integer(1),
        I::Atom("uint64"),
        I::Atom("isolation_scope"),
        I::Integer(42),
    ];
    let rows = [(I::Integer(1), I::Tuple(&sealed))];
    let encoded =
        original_record_physical_partition(Some(I::Binary(b"")), Some(I::Map(&rows)), limits())
            .map_err(|error| format!("{error:?}"))?
            .value
            .ok_or("expected source value")?;
    assert_eq!(encoded.as_ref(), physical(b"", &42_u64.to_be_bytes()));
    assert!(original_physical_scope(&encoded).is_none());
    let logical =
        original_record_logical_partition(Some(I::Binary(&encoded)), Some(I::Map(&rows)), limits())
            .map_err(|error| format!("{error:?}"))?;
    assert_eq!(logical.value.ok_or("expected source value")?.as_ref(), b"");
    assert_eq!(
        original_physical_partition(&encoded, Some(&42_u64.to_be_bytes()), limits())
            .map_err(|error| format!("{error:?}"))?
            .value,
        encoded
    );
    Ok(())
}

#[test]
fn direct_logical_scope_does_not_inherit_the_physical_scope_size_guard() -> Result<(), String> {
    let large = [b'x'; 8193];
    let encoded = physical(b"p", &large);
    assert_eq!(
        original_logical_partition(&encoded, &large, limits())
            .map_err(|error| format!("{error:?}"))?
            .value,
        b"p"
    );
    assert!(original_physical_scope(&encoded).is_none());
    assert!(original_physical_partition(b"p", Some(&large), limits()).is_err());
    assert!(original_logical_partition(&encoded, b"", limits()).is_err());
    for end in 0..15 {
        assert!(original_physical_scope(&encoded[..end]).is_none());
    }
    Ok(())
}

#[test]
fn automatic_scope_requires_all_exact_ordered_256_partitions() -> Result<(), String> {
    let scope = 42_u64.to_be_bytes();
    let partitions: Vec<_> = (0..256)
        .map(|bucket| physical(format!("__flow_auto__:{bucket}").as_bytes(), &scope))
        .collect();
    let mut refs: Vec<_> = partitions.iter().map(Vec::as_slice).collect();
    assert_eq!(
        original_scoped_auto_partition_scope(&refs, limits())
            .map_err(|error| format!("{error:?}"))?
            .value,
        scope
    );
    refs.swap(1, 2);
    assert!(original_scoped_auto_partition_scope(&refs, limits()).is_err());
    refs.swap(1, 2);
    assert!(original_scoped_auto_partition_scope(&refs[..255], limits()).is_err());
    refs[255] = refs[254];
    assert!(original_scoped_auto_partition_scope(&refs, limits()).is_err());
    Ok(())
}

#[test]
fn independent_work_temporary_and_output_endpoints_are_inclusive() -> Result<(), String> {
    let sealed = [
        I::Integer(1),
        I::Atom("keyword"),
        I::Atom("isolation_scope"),
        I::Binary(b"tenant"),
    ];
    let rows = [(I::Integer(1), I::Tuple(&sealed))];
    let input = I::Map(&rows);
    let measured =
        original_system_metadata_scope(input, limits()).map_err(|error| format!("{error:?}"))?;
    let exact = OriginalScopeLimits {
        maximum_output_bytes: measured.value.ok_or("expected source value")?.len(),
        maximum_work_units: measured.work_units,
        maximum_temporary_bytes: measured.temporary_bytes,
    };
    assert!(original_system_metadata_scope(input, exact).is_ok());
    for selected in [
        OriginalScopeLimits {
            maximum_output_bytes: exact.maximum_output_bytes - 1,
            ..exact
        },
        OriginalScopeLimits {
            maximum_work_units: exact.maximum_work_units - 1,
            ..exact
        },
        OriginalScopeLimits {
            maximum_temporary_bytes: exact.maximum_temporary_bytes - 1,
            ..exact
        },
    ] {
        assert!(matches!(
            original_system_metadata_scope(input, selected),
            Err(OriginalScopeError::Limit)
        ));
    }
    assert!(matches!(
        original_system_metadata_scope(
            input,
            OriginalScopeLimits {
                maximum_work_units: 0,
                ..exact
            }
        ),
        Err(OriginalScopeError::InvalidLimits)
    ));
    assert!(matches!(
        original_physical_partition(
            b"p",
            Some(b"scope"),
            OriginalScopeLimits {
                maximum_work_units: 4096,
                ..limits()
            }
        ),
        Err(OriginalScopeError::Limit)
    ));
    Ok(())
}
