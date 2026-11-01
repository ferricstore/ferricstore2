use super::{fixtures as f, *};

#[test]
fn all_four_physical_locator_sources_preserve_source_flags_and_addresses() -> TestResult {
    let key = f::key()?;
    for (tag, expected) in [
        ("waraft_segment", 1),
        ("waraft_projection", 2),
        ("waraft_apply_projection", 3),
    ] {
        let pair = [I::Atom(tag), I::Integer(99)];
        let rows = f::replace(f::LOCATOR, "file_id", I::Tuple(&pair));
        let rows = f::replace(&rows, "frame_size", I::Integer(64));
        let value = encode(
            I::Binary(&key),
            f::record(),
            Locator {
                fields: I::Map(&rows),
            },
            I::Integer(0),
            f::limits(),
        )
        .map_err(|e| format!("{e:?}"))?;
        assert_eq!(
            value.encoded().get(17..20),
            Some([expected, 22, 99].as_slice())
        );
        let invalid = f::replace(&rows, "frame_size", I::Integer(7));
        assert!(
            encode(
                I::Binary(&key),
                f::record(),
                Locator {
                    fields: I::Map(&invalid)
                },
                I::Integer(0),
                f::limits()
            )
            .is_err()
        );
        let invalid = f::replace(&rows, "segment_generation", I::Nil);
        assert!(
            encode(
                I::Binary(&key),
                f::record(),
                Locator {
                    fields: I::Map(&invalid)
                },
                I::Integer(0),
                f::limits()
            )
            .is_err()
        );
    }
    let plain = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(plain.encoded().get(17..20), Some([0, 7, 3].as_slice()));
    Ok(())
}

#[test]
fn locator_expiry_checksum_and_generation_are_validated_before_emission() -> TestResult {
    let key = f::key()?;
    for (name, value) in [
        ("checksum", I::Nil),
        ("checksum", I::Binary(&[1; 31])),
        ("checksum", I::Binary(&[1; 33])),
        ("value_size", I::Integer(0)),
        ("flow_id", I::Binary(b"foreign")),
        ("kind", I::Atom("value")),
        ("version", I::Integer(8)),
    ] {
        let rows = f::replace(f::LOCATOR, name, value);
        assert!(
            encode(
                I::Binary(&key),
                f::record(),
                Locator {
                    fields: I::Map(&rows)
                },
                I::Integer(0),
                f::limits()
            )
            .is_err()
        );
    }
    let rows = f::replace(f::LOCATOR, "expire_at_ms", I::Integer(9000));
    for expiry in [0, 9001] {
        assert!(
            encode(
                I::Binary(&key),
                f::record(),
                Locator {
                    fields: I::Map(&rows)
                },
                I::Integer(expiry),
                f::limits()
            )
            .is_err()
        );
    }
    for expiry in [1, 9000] {
        assert!(
            encode(
                I::Binary(&key),
                f::record(),
                Locator {
                    fields: I::Map(&rows)
                },
                I::Integer(expiry),
                f::limits()
            )
            .is_ok()
        );
    }
    let rows = f::replace(f::LOCATOR, "expire_at_ms", I::Integer(0));
    assert!(
        encode(
            I::Binary(&key),
            f::record(),
            Locator {
                fields: I::Map(&rows)
            },
            I::Integer(9000),
            f::limits()
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn largest_104_byte_locator_fits_at_the_exact_metadata_limit() -> TestResult {
    let key = f::key()?;
    let large = vec![b'x'; 32424];
    let record = f::replace(f::RECORD, "type", I::Binary(&large));
    let mut locator = f::LOCATOR.to_vec();
    for name in [
        "raft_index",
        "offset",
        "value_size",
        "frame_size",
        "expire_at_ms",
        "segment_generation",
    ] {
        locator = f::replace(&locator, name, I::Integer(i128::from(u64::MAX)));
    }
    locator = f::replace(&locator, "file_id", I::Integer(i128::from(u64::MAX) - 1));
    let value = encode(
        I::Binary(&key),
        I::Map(&record),
        Locator {
            fields: I::Map(&locator),
        },
        I::Integer(1),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        value.encoded().get(13..15),
        Some(104_u16.to_be_bytes().as_slice())
    );
    assert!(value.encoded().len() <= 32768);
    Ok(())
}

#[test]
fn unpartitioned_rows_use_the_original_automatic_key() -> TestResult {
    let key = wf_ports::blob::render_original_state_key(
        b"run-1",
        wf_ports::blob::OriginalValuePartition::Automatic,
        65535,
    )
    .map_err(|e| format!("{e:?}"))?;
    let record = f::replace(f::RECORD, "partition_key", I::Nil);
    let value =
        prepare(I::Binary(&key), I::Map(&record), f::limits()).map_err(|e| format!("{e:?}"))?;
    assert_eq!(value.logical_partition(), None);
    assert!(
        encode(
            I::Binary(&key),
            I::Map(&record),
            f::locator(),
            I::Integer(0),
            f::limits()
        )
        .is_ok()
    );
    Ok(())
}
