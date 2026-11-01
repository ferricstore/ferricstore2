use super::{fixtures as f, *};

#[test]
fn relocation_compare_and_swaps_only_the_physical_locator() -> TestResult {
    let key = f::key()?;
    let loc = f::replace(f::LOCATOR, "expire_at_ms", I::Integer(9000));
    let old = location(Some(9000));
    let new = Location {
        source_id: 4,
        offset: 1024,
        value_size: 600,
        segment_generation: Some(5),
        ..old
    };
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
    let moved = relocate(encoded.encoded(), &key, &old, &new, limits());
    assert!(moved.is_ok());
    let moved = moved.map_err(|e| format!("{e:?}"))?;
    let decoded = decode(moved.encoded(), &key, None, limits());
    assert!(decoded.is_ok());
    let decoded = row(decoded.map_err(|e| format!("{e:?}"))?)?;
    assert!(decoded.locator() == &new && decoded.expire_at_ms() == 9000);
    assert!(decoded.record().is_some_and(|r| record_eq(r, f::record())));
    let stale = Location {
        offset: old.offset + 1,
        ..old
    };
    assert!(matches!(
        relocate(encoded.encoded(), &key, &stale, &new, limits()),
        Err(E::LocatorCompareFailed)
    ));
    let newer = Location {
        version: new.version + 1,
        ..new
    };
    assert!(matches!(
        relocate(encoded.encoded(), &key, &old, &newer, limits()),
        Err(E::LogicalGenerationMismatch)
    ));
    Ok(())
}

#[test]
fn supports_every_authoritative_state_locator_source() -> TestResult {
    let key = f::key()?;
    for (tag, id) in [
        ("", 7),
        ("waraft_segment", 8),
        ("waraft_projection", 9),
        ("waraft_apply_projection", 10),
    ] {
        let pair = [I::Atom(tag), I::Integer(id)];
        let file = if tag.is_empty() {
            I::Integer(id)
        } else {
            I::Tuple(&pair)
        };
        let loc = f::replace(f::LOCATOR, "file_id", file);
        let loc = f::replace(
            &loc,
            "raft_index",
            I::Integer(if tag.is_empty() { 99 } else { id }),
        );
        let loc = f::replace(
            &loc,
            "segment_generation",
            if tag.is_empty() {
                I::Nil
            } else {
                I::Integer(4)
            },
        );
        let loc = f::replace(
            &loc,
            "frame_size",
            if tag.is_empty() {
                I::Nil
            } else {
                I::Integer(1024)
            },
        );
        let expected = Location {
            source_tag: match tag {
                "waraft_segment" => 1,
                "waraft_projection" => 2,
                "waraft_apply_projection" => 3,
                _ => 0,
            },
            source_id: u64::try_from(id).map_err(|e| format!("{e:?}"))?,
            raft_index: if tag.is_empty() {
                99
            } else {
                u64::try_from(id).map_err(|e| format!("{e:?}"))?
            },
            segment_generation: (!tag.is_empty()).then_some(4),
            frame_size: (!tag.is_empty()).then_some(1024),
            ..location(None)
        };
        let encoded = encode(
            I::Binary(&key),
            f::record(),
            Locator {
                fields: I::Map(&loc),
            },
            I::Integer(0),
            f::limits(),
        );
        assert!(encoded.is_ok());
        let encoded = encoded.map_err(|e| format!("{e:?}"))?;
        let decoded =
            decode(encoded.encoded(), &key, None, limits()).map_err(|e| format!("{e:?}"))?;
        assert!(matches!(decoded,Read::Row{value,..} if *value.locator()==expected));
    }
    Ok(())
}

#[test]
fn rejects_logical_waraft_locations_and_round_trips_complete_physical_frame_address() -> TestResult
{
    let key = f::key()?;
    let pair = [I::Atom("waraft_apply_projection"), I::Integer(99)];
    let loc = f::replace(f::LOCATOR, "file_id", I::Tuple(&pair));
    let logical = f::replace(&loc, "segment_generation", I::Nil);
    assert!(matches!(
        encode(
            I::Binary(&key),
            f::record(),
            Locator {
                fields: I::Map(&logical)
            },
            I::Integer(0),
            f::limits()
        ),
        Err(E::InvalidRecord)
    ));
    let physical = f::replace(&logical, "segment_generation", I::Integer(7));
    let physical = f::replace(&physical, "offset", I::Integer(4096));
    let physical = f::replace(&physical, "frame_size", I::Integer(2048));
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        Locator {
            fields: I::Map(&physical),
        },
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let expected = Location {
        source_tag: 3,
        source_id: 99,
        segment_generation: Some(7),
        offset: 4096,
        frame_size: Some(2048),
        ..location(None)
    };
    let decoded = decode(encoded.encoded(), &key, None, limits()).map_err(|e| format!("{e:?}"))?;
    assert!(matches!(decoded,Read::Row{value,..} if *value.locator()==expected));
    Ok(())
}

#[test]
fn binds_the_row_to_its_state_key_and_logical_generation() -> TestResult {
    let key = f::key()?;
    let loc = f::replace(f::LOCATOR, "flow_id", I::Binary(b"other"));
    assert!(matches!(
        encode(
            I::Binary(&key),
            f::record(),
            Locator {
                fields: I::Map(&loc)
            },
            I::Integer(0),
            f::limits()
        ),
        Err(E::InvalidRecord)
    ));
    let loc = f::replace(f::LOCATOR, "version", I::Integer(8));
    assert!(matches!(
        encode(
            I::Binary(&key),
            f::record(),
            Locator {
                fields: I::Map(&loc)
            },
            I::Integer(0),
            f::limits()
        ),
        Err(E::InvalidRecord)
    ));
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let foreign = wf_ports::blob::render_original_state_key(
        b"run-1",
        wf_ports::blob::OriginalValuePartition::Explicit(b"tenant-b"),
        65535,
    )
    .map_err(|e| format!("{e:?}"))?;
    let result = decode(encoded.encoded(), &foreign, None, limits());
    assert!(result.is_err());
    assert!(matches!(result, Err(E::InvalidRecord)));
    Ok(())
}

#[test]
fn rejects_inconsistent_row_and_locator_expiry() -> TestResult {
    let key = f::key()?;
    let loc = f::replace(f::LOCATOR, "expire_at_ms", I::Integer(8000));
    assert!(matches!(
        encode(
            I::Binary(&key),
            f::record(),
            Locator {
                fields: I::Map(&loc)
            },
            I::Integer(9000),
            f::limits()
        ),
        Err(E::InvalidRecord)
    ));
    let loc = f::replace(f::LOCATOR, "expire_at_ms", I::Integer(1));
    assert!(matches!(
        encode(
            I::Binary(&key),
            f::record(),
            Locator {
                fields: I::Map(&loc)
            },
            I::Integer(0),
            f::limits()
        ),
        Err(E::InvalidRecord)
    ));
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(9000),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let decoded = decode(encoded.encoded(), &key, None, limits()).map_err(|e| format!("{e:?}"))?;
    assert!(
        matches!(decoded,Read::Row{value,..} if value.locator().expire_at_ms.is_none() && value.expire_at_ms()==9000)
    );
    let loc = f::replace(f::LOCATOR, "expire_at_ms", I::Integer(10_000));
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
    let decoded = decode(encoded.encoded(), &key, None, limits()).map_err(|e| format!("{e:?}"))?;
    assert!(
        matches!(decoded,Read::Row{value,..} if value.locator().expire_at_ms==Some(10_000) && value.expire_at_ms()==9000)
    );
    Ok(())
}

#[test]
fn relocation_canonicalizes_accepted_redundant_root_and_locator_encodings() -> TestResult {
    let key = f::key()?;
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let mut bytes = encoded.encoded().to_vec();
    let locator_len = usize::from(u16::from_be_bytes(
        bytes
            .get(13..15)
            .ok_or("length")?
            .try_into()
            .map_err(|e| format!("{e:?}"))?,
    ));
    *bytes.get_mut(17 + locator_len).ok_or("metadata flags")? = 1;
    let bytes = adversarial::refresh(bytes, &key)?;
    let decoded = row(decode(&bytes, &key, None, limits()).map_err(|e| format!("{e:?}"))?)?;
    assert!(matches!(
        decoded.record().ok_or("record")?.field("root_flow_id"),
        Some(I::Binary(b"run-1"))
    ));
    let old = location(None);
    let new = Location {
        source_id: 4,
        ..old
    };
    let moved = relocate(&bytes, &key, &old, &new, limits()).map_err(|e| format!("{e:?}"))?;
    let canonical_record = f::replace(f::RECORD, "root_flow_id", I::Binary(b"run-1"));
    let canonical_loc = f::replace(f::LOCATOR, "file_id", I::Integer(4));
    let canonical = encode(
        I::Binary(&key),
        I::Map(&canonical_record),
        Locator {
            fields: I::Map(&canonical_loc),
        },
        I::Integer(0),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(moved.encoded(), canonical.encoded());
    assert!(moved.encoded().len() < bytes.len());
    let loc = f::replace(f::LOCATOR, "file_id", I::Integer(99));
    let canonical = encode(
        I::Binary(&key),
        f::record(),
        Locator {
            fields: I::Map(&loc),
        },
        I::Integer(0),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let mut redundant = canonical.encoded().to_vec();
    let old_len = u16::from_be_bytes(
        redundant
            .get(13..15)
            .ok_or("length")?
            .try_into()
            .map_err(|e| format!("{e:?}"))?,
    );
    *redundant.get_mut(18).ok_or("flags")? |= 1;
    redundant.insert(20, 99);
    redundant
        .get_mut(13..15)
        .ok_or("length")?
        .copy_from_slice(&(old_len + 1).to_be_bytes());
    let redundant = adversarial::refresh(redundant, &key)?;
    assert!(decode(&redundant, &key, None, limits()).is_ok());
    let expected = Location {
        source_id: 99,
        ..location(None)
    };
    let moved =
        relocate(&redundant, &key, &expected, &expected, limits()).map_err(|e| format!("{e:?}"))?;
    assert_eq!(moved.encoded(), canonical.encoded());
    Ok(())
}

#[test]
fn relocation_preserves_error_order_and_excludes_source_expiry_from_cas() -> TestResult {
    let key = f::key()?;
    let old = location(None);
    let newer = Location { version: 8, ..old };
    assert!(matches!(
        relocate(&[], &key, &old, &newer, limits()),
        Err(E::LogicalGenerationMismatch)
    ));
    assert!(matches!(
        relocate(&[], &key, &old, &old, limits()),
        Err(E::InvalidRecord)
    ));
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let stale = Location {
        offset: old.offset + 1,
        ..old
    };
    let invalid = Location {
        value_size: 0,
        ..old
    };
    assert!(matches!(
        relocate(encoded.encoded(), &key, &stale, &invalid, limits()),
        Err(E::LocatorCompareFailed)
    ));
    assert!(matches!(
        relocate(encoded.encoded(), &key, &old, &invalid, limits()),
        Err(E::InvalidRecord)
    ));
    let different_expiry = Location {
        expire_at_ms: Some(99),
        ..old
    };
    let new = Location {
        source_id: 4,
        ..old
    };
    assert!(relocate(encoded.encoded(), &key, &different_expiry, &new, limits()).is_ok());
    Ok(())
}
