use super::{fixtures as f, *};

pub(super) fn refresh(mut bytes: Vec<u8>, key: &[u8]) -> TestResult<Vec<u8>> {
    let mut crc = u32::MAX;
    for &byte in key.iter().chain(bytes.get(9..).ok_or("body")?) {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 0 { 0 } else { 0xedb8_8320 };
        }
    }
    bytes
        .get_mut(5..9)
        .ok_or("checksum")?
        .copy_from_slice(&(!crc).to_be_bytes());
    Ok(bytes)
}
fn corrupt_name(bytes: &[u8], key: &[u8], name: &[u8]) -> TestResult<Vec<u8>> {
    let mut marker = vec![u8::try_from(name.len()).map_err(|e| format!("{e:?}"))?];
    marker.extend_from_slice(name);
    let start = bytes
        .windows(marker.len())
        .position(|v| v == marker)
        .ok_or("name")?;
    let mut bytes = bytes.to_vec();
    *bytes.get_mut(start + 1).ok_or("name byte")? = 255;
    refresh(bytes, key)
}
fn lean() -> Plan<'static> {
    Plan {
        attributes: Selection::None,
        state_meta: StateSelection::None,
    }
}

#[test]
fn query_decoding_validates_skipped_metadata_instead_of_trusting_checksum() -> TestResult {
    let key = f::key()?;
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let invalid = corrupt_name(encoded.encoded(), &key, b"labels")?;
    assert!(matches!(
        decode(&invalid, &key, None, limits()),
        Err(E::InvalidRecord)
    ));
    assert!(matches!(
        query(&invalid, &key, 0, lean(), limits()),
        Err(E::InvalidRecord)
    ));
    let invalid = corrupt_name(encoded.encoded(), &key, b"created")?;
    assert!(matches!(
        decode(&invalid, &key, None, limits()),
        Err(E::InvalidRecord)
    ));
    assert!(matches!(
        query(&invalid, &key, 0, lean(), limits()),
        Err(E::InvalidRecord)
    ));
    Ok(())
}

#[test]
fn query_decoding_rejects_duplicate_list_values_in_selected_and_skipped_attributes() -> TestResult {
    let key = f::key()?;
    let attributes = [
        (I::Binary(b"tier"), I::Binary(b"gold")),
        (
            I::Binary(b"labels"),
            I::List(&[I::Binary(b"alpha"), I::Binary(b"bravo")]),
        ),
    ];
    let record = f::replace(f::RECORD, "attributes", I::Map(&attributes));
    let encoded = encode(
        I::Binary(&key),
        I::Map(&record),
        f::locator(),
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let start = encoded
        .encoded()
        .windows(5)
        .position(|v| v == b"bravo")
        .ok_or("bravo")?;
    let mut bytes = encoded.encoded().to_vec();
    bytes
        .get_mut(start..start + 5)
        .ok_or("list value")?
        .copy_from_slice(b"alpha");
    let bytes = refresh(bytes, &key)?;
    assert!(matches!(
        decode(&bytes, &key, None, limits()),
        Err(E::InvalidRecord)
    ));
    let selected = [b"labels".as_slice()];
    assert!(matches!(
        query(
            &bytes,
            &key,
            0,
            Plan {
                attributes: Selection::Names(&selected),
                state_meta: StateSelection::None
            },
            limits()
        ),
        Err(E::InvalidRecord)
    ));
    assert!(matches!(
        query(&bytes, &key, 0, lean(), limits()),
        Err(E::InvalidRecord)
    ));
    Ok(())
}

#[test]
fn reference_deliberately_skips_metadata_but_full_and_expired_reads_validate_it() -> TestResult {
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
    let bad = corrupt_name(encoded.encoded(), &key, b"created")?;
    let reference = row(reference(&bad, &key, 1999, limits()).map_err(|e| format!("{e:?}"))?)?;
    assert!(reference.record().is_none());
    assert_eq!(
        reference.scope_prefix(),
        wf_ports::projection::query_row::OriginalQueryScope::NotRead
    );
    assert!(matches!(
        decode(&bad, &key, Some(2000), limits()),
        Err(E::InvalidRecord)
    ));
    assert!(matches!(
        query(&bad, &key, 2000, lean(), limits()),
        Err(E::InvalidRecord)
    ));
    Ok(())
}

#[test]
fn rejects_every_truncation_and_single_bit_change_and_sealed_noncanonical_varints() -> TestResult {
    let key = f::key()?;
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    for end in 0..encoded.encoded().len() {
        assert!(matches!(
            decode(&encoded.encoded()[..end], &key, None, limits()),
            Err(E::InvalidRecord)
        ));
    }
    for offset in 0..encoded.encoded().len() {
        let mut bytes = encoded.encoded().to_vec();
        *bytes.get_mut(offset).ok_or("offset")? ^= 1;
        assert!(matches!(
            decode(&bytes, &key, None, limits()),
            Err(E::InvalidRecord)
        ));
    }
    let mut bytes = encoded.encoded().to_vec();
    bytes.splice(9..10, [0x87, 0]);
    let bytes = refresh(bytes, &key)?;
    assert!(matches!(
        decode(&bytes, &key, None, limits()),
        Err(E::InvalidRecord)
    ));
    assert!(matches!(
        reference(&bytes, &key, 0, limits()),
        Err(E::InvalidRecord)
    ));
    Ok(())
}

#[test]
fn reference_retains_u64_version_while_full_record_uses_covering_ceiling() -> TestResult {
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
    bytes.splice(9..10, [255, 255, 255, 255, 255, 255, 255, 255, 255, 1]);
    let bytes = refresh(bytes, &key)?;
    let r = row(reference(&bytes, &key, 0, limits()).map_err(|e| format!("{e:?}"))?)?;
    assert_eq!(r.version(), u64::MAX);
    assert!(matches!(
        decode(&bytes, &key, None, limits()),
        Err(E::InvalidRecord)
    ));
    Ok(())
}
