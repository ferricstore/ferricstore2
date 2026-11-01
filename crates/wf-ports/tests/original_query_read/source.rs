use super::{fixtures as f, *};

#[test]
fn round_trips_all_query_metadata_and_a_physical_locator() -> TestResult {
    let key = f::key()?;
    let loc = f::replace(f::LOCATOR, "expire_at_ms", I::Integer(9000));
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        Locator {
            fields: I::Map(&loc),
        },
        I::Integer(9000),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    assert!(encoded.encoded().len() <= 32768);
    let decoded = decode(encoded.encoded(), &key, None, limits());
    assert!(decoded.is_ok());
    let decoded = row(decoded.map_err(|e| format!("{e:?}"))?)?;
    assert!(
        decoded.state_key() == key
            && decoded.expire_at_ms() == 9000
            && *decoded.locator() == location(Some(9000))
    );
    assert!(decoded.record().is_some_and(|r| record_eq(r, f::record())));
    Ok(())
}

#[test]
fn applies_expiry_after_required_metadata_validation() -> TestResult {
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
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    assert!(decode(encoded.encoded(), &key, Some(1999), limits()).is_ok_and(|r| !r.is_expired()));
    assert!(decode(encoded.encoded(), &key, Some(2000), limits()).is_ok_and(|r| r.is_expired()));
    Ok(())
}

#[test]
fn preserves_missing_null_scalar_and_multivalue_metadata_distinctly() -> TestResult {
    let key = f::key()?;
    let attributes = [
        (I::Binary(b"nullable"), I::Nil),
        (I::Binary(b"enabled"), I::Boolean(true)),
        (I::Binary(b"score"), I::Float(1.5)),
        (
            I::Binary(b"labels"),
            I::List(&[I::Binary(b"urgent"), I::Binary(b"finance")]),
        ),
    ];
    let states = [(
        I::Binary(b"queued"),
        I::Map(&[
            (I::Binary(b"nullable"), I::Nil),
            (I::Binary(b"attempt"), I::Integer(2)),
        ]),
    )];
    let record = f::replace(f::RECORD, "attributes", I::Map(&attributes));
    let record = f::replace(&record, "state_meta", I::Map(&states));
    let encoded = encode(
        I::Binary(&key),
        I::Map(&record),
        f::locator(),
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let decoded = decode(encoded.encoded(), &key, None, limits());
    assert!(decoded.is_ok());
    let decoded = row(decoded.map_err(|e| format!("{e:?}"))?)?;
    let record = decoded.record().ok_or("record")?;
    assert!(
        record
            .attribute(b"nullable")
            .is_some_and(|v| value_eq(I::Nil, v))
    );
    assert!(record.attribute(b"missing").is_none());
    assert!(
        record
            .attribute(b"labels")
            .is_some_and(|v| value_eq(I::List(&[I::Binary(b"urgent"), I::Binary(b"finance")]), v))
    );
    assert!(
        record
            .state_field(b"queued", b"nullable")
            .is_some_and(|v| value_eq(I::Nil, v))
    );
    Ok(())
}

#[test]
fn round_trips_an_unpartitioned_run_through_its_automatic_state_key() -> TestResult {
    let key = wf_ports::blob::render_original_state_key(
        b"run-1",
        wf_ports::blob::OriginalValuePartition::Automatic,
        65535,
    )
    .map_err(|e| format!("{e:?}"))?;
    let record = f::replace(f::RECORD, "partition_key", I::Nil);
    let encoded = encode(
        I::Binary(&key),
        I::Map(&record),
        f::locator(),
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let decoded = decode(encoded.encoded(), &key, None, limits()).map_err(|e| format!("{e:?}"))?;
    assert!(
        matches!(decoded,Read::Row{value,..} if value.state_key()==key && value.record().is_some_and(|r|record_eq(r,I::Map(&record))))
    );
    Ok(())
}

#[test]
fn does_not_persist_payload_references_or_internal_system_metadata() -> TestResult {
    let key = f::key()?;
    let record = f::replace(f::RECORD, "payload_ref", I::Binary(b"payload-secret"));
    let record = f::replace(&record, "result_ref", I::Binary(b"result-secret"));
    let record = f::replace(
        &record,
        "value_refs",
        I::Map(&[(I::Binary(b"payload"), I::Binary(b"value-secret"))]),
    );
    let record = f::replace(&record, "system_metadata", I::Map(&[]));
    let encoded = encode(
        I::Binary(&key),
        I::Map(&record),
        f::locator(),
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    assert!(
        !encoded
            .encoded()
            .windows(14)
            .any(|v| v == b"payload-secret")
    );
    assert!(!encoded.encoded().windows(13).any(|v| v == b"result-secret"));
    assert!(!encoded.encoded().windows(12).any(|v| v == b"value-secret"));
    let decoded = decode(encoded.encoded(), &key, None, limits());
    assert!(decoded.is_ok());
    let decoded = row(decoded.map_err(|e| format!("{e:?}"))?)?;
    let record = decoded.record().ok_or("record")?;
    assert!(record.field("payload_ref").is_none());
    assert!(record.field("result_ref").is_none());
    assert!(record.field("value_refs").is_none());
    assert!(record.field("system_metadata").is_none());
    Ok(())
}

#[test]
fn restores_terminal_retention_from_an_expired_covering_row() -> TestResult {
    let key = f::key()?;
    let record = f::replace(f::RECORD, "state", I::Binary(b"cancelled"));
    let record = f::replace(&record, "next_run_at_ms", I::Nil);
    let record = f::replace(&record, "terminal_retention_until_ms", I::Integer(2000));
    let loc = f::replace(f::LOCATOR, "expire_at_ms", I::Integer(2000));
    let encoded = encode(
        I::Binary(&key),
        I::Map(&record),
        Locator {
            fields: I::Map(&loc),
        },
        I::Integer(2000),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let decoded = decode(encoded.encoded(), &key, Some(0), limits());
    assert!(decoded.is_ok());
    let decoded = row(decoded.map_err(|e| format!("{e:?}"))?)?;
    assert!(
        decoded
            .record()
            .ok_or("record")?
            .field("terminal_retention_until_ms")
            .is_none()
    );
    let value = internal(&decoded, limits())
        .map_err(|e| format!("{e:?}"))?
        .into_row()
        .ok_or("internal")?;
    assert!(matches!(
        value.field("terminal_retention_until_ms"),
        Some(I::Integer(2000))
    ));
    Ok(())
}

#[test]
fn decodes_a_strict_hydration_reference_without_materializing_record_metadata() -> TestResult {
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
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let decoded =
        reference(encoded.encoded(), &key, 1999, limits()).map_err(|e| format!("{e:?}"))?;
    assert!(
        matches!(&decoded,Read::Row{value,..} if value.state_key()==key && value.flow_id()==b"run-1" && value.version()==7 && *value.locator()==location(Some(2000)) && value.expire_at_ms()==2000)
    );
    let decoded = row(decoded)?;
    assert!(decoded.record().is_none());
    assert!(reference(encoded.encoded(), &key, 2000, limits()).is_ok_and(|r| r.is_expired()));
    let mut trailing = encoded.encoded().to_vec();
    trailing.push(0);
    assert!(matches!(
        reference(&trailing, &key, 0, limits()),
        Err(E::InvalidRecord)
    ));
    for offset in 0..encoded.encoded().len() {
        let mut damaged = encoded.encoded().to_vec();
        *damaged.get_mut(offset).ok_or("offset")? ^= 1;
        assert!(matches!(
            reference(&damaged, &key, 0, limits()),
            Err(E::InvalidRecord)
        ));
    }
    Ok(())
}

#[test]
fn binds_opaque_shared_scope_while_exposing_only_the_logical_partition() -> TestResult {
    let scope = 42_u64.to_be_bytes();
    let sl = wf_domain::metadata::scope::OriginalScopeLimits {
        maximum_output_bytes: 100_000,
        maximum_work_units: 20_000_000,
        maximum_temporary_bytes: 2_000_000,
    };
    let physical =
        wf_domain::metadata::scope::original_physical_partition(b"tenant-a", Some(&scope), sl);
    assert!(physical.is_ok());
    let physical = physical.map_err(|e| format!("{e:?}"))?.value;
    let key = wf_ports::blob::render_original_state_key(
        b"run-1",
        wf_ports::blob::OriginalValuePartition::Explicit(&physical),
        65535,
    )
    .map_err(|e| format!("{e:?}"))?;
    let sealed = [
        I::Integer(1),
        I::Atom("uint64"),
        I::Atom("isolation_scope"),
        I::Integer(42),
    ];
    let metadata = [(I::Integer(0x8001), I::Tuple(&sealed))];
    let record = f::replace(f::RECORD, "partition_key", I::Binary(&physical));
    let record = f::replace(&record, "system_metadata", I::Map(&metadata));
    let loc = f::replace(f::LOCATOR, "expire_at_ms", I::Integer(9000));
    let encoded = encode(
        I::Binary(&key),
        I::Map(&record),
        Locator {
            fields: I::Map(&loc),
        },
        I::Integer(9000),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let decoded = decode(encoded.encoded(), &key, None, limits()).map_err(|e| format!("{e:?}"))?;
    assert!(
        matches!(&decoded,Read::Row{value,..} if value.state_key()==key && value.scope_prefix()==wf_ports::projection::query_row::OriginalQueryScope::Scoped(&scope) && value.record().is_some_and(|r|matches!(r.field("partition_key"),Some(I::Binary(b"tenant-a")))))
    );
    let decoded = row(decoded)?;
    assert!(
        decoded
            .record()
            .ok_or("record")?
            .field("system_metadata")
            .is_none()
    );
    let foreign = wf_domain::metadata::scope::original_physical_partition(
        b"tenant-a",
        Some(&99_u64.to_be_bytes()),
        sl,
    );
    assert!(foreign.is_ok());
    let foreign = foreign.map_err(|e| format!("{e:?}"))?.value;
    let foreign_key = wf_ports::blob::render_original_state_key(
        b"run-1",
        wf_ports::blob::OriginalValuePartition::Explicit(&foreign),
        65535,
    )
    .map_err(|e| format!("{e:?}"))?;
    assert!(matches!(
        encode(
            I::Binary(&foreign_key),
            I::Map(&record),
            Locator {
                fields: I::Map(&loc)
            },
            I::Integer(9000),
            f::limits()
        ),
        Err(E::InvalidRecord)
    ));
    let physical_view = internal(&decoded, limits())
        .map_err(|e| format!("{e:?}"))?
        .into_row()
        .ok_or("internal")?;
    assert!(
        matches!(physical_view.field("partition_key"),Some(I::Binary(bytes)) if bytes==physical.as_ref())
    );
    Ok(())
}

#[test]
fn derives_large_run_identity_from_the_state_key_instead_of_duplicating_it() -> TestResult {
    let id = vec![b'r'; 60_000];
    let key = wf_ports::blob::render_original_state_key(
        &id,
        wf_ports::blob::OriginalValuePartition::Explicit(b"tenant-a"),
        65535,
    )
    .map_err(|e| format!("{e:?}"))?;
    let record = f::replace(f::RECORD, "id", I::Binary(&id));
    let record = f::replace(&record, "root_flow_id", I::Binary(&id));
    let loc = f::replace(f::LOCATOR, "flow_id", I::Binary(&id));
    let encoded = encode(
        I::Binary(&key),
        I::Map(&record),
        Locator {
            fields: I::Map(&loc),
        },
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    assert!(encoded.encoded().len() < 1024);
    let decoded = decode(encoded.encoded(), &key, None, limits()).map_err(|e| format!("{e:?}"))?;
    assert!(
        matches!(decoded,Read::Row{value,..} if value.locator().flow_id==id && value.record().is_some_and(|r|matches!(r.field("id"),Some(I::Binary(bytes)) if bytes==id)))
    );
    Ok(())
}
