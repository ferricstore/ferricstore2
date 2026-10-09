//! Counterparts for all three pinned partition-key tests, plus deterministic properties.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_types::{error::ValueError, identity::PartitionKey};

// Source declaration f05f226483bfbc13; PartitionKeyTest line 6.
#[test]
fn encodes_components_with_byte_lengths() -> Result<(), ValueError> {
    let key = PartitionKey::encode_components(&[b"tenant-a".as_slice(), b"invoice-1"])?;
    assert_eq!(key.as_bytes(), b"fpk:8:tenant-a9:invoice-1");
    Ok(())
}

// Source declaration 3221412a3842b3c5; PartitionKeyTest line 11.
#[test]
fn delimiter_bytes_cannot_make_different_component_lists_collide() -> Result<(), ValueError> {
    assert_ne!(
        PartitionKey::encode_components(&[b"a:b".as_slice(), b"c"])?,
        PartitionKey::encode_components(&[b"a".as_slice(), b"b:c"])?
    );
    Ok(())
}

// Source declaration 4ba84b7b957de543; PartitionKeyTest line 16.
#[test]
fn uses_byte_sizes_for_arbitrary_binary_components() -> Result<(), ValueError> {
    assert_eq!(
        PartitionKey::encode_components(&[&[0, 255]])?.as_bytes(),
        b"fpk:2:\0\xff"
    );
    Ok(())
}

#[test]
fn empty_components_are_valid_but_an_empty_component_list_is_rejected() -> Result<(), ValueError> {
    assert_eq!(
        PartitionKey::encode_components(&[]),
        Err(ValueError::EmptyPartitionComponents)
    );
    for (components, expected) in [
        (vec![b"".as_slice()], b"fpk:0:".as_slice()),
        (vec![b"", b""], b"fpk:0:0:"),
        (vec![b"", b"a"], b"fpk:0:1:a"),
        (vec![b"a", b""], b"fpk:1:a0:"),
    ] {
        assert_eq!(
            PartitionKey::encode_components(&components)?.as_bytes(),
            expected
        );
    }
    Ok(())
}

#[test]
fn multibyte_text_uses_byte_length_and_binary_order_is_preserved() -> Result<(), ValueError> {
    assert_eq!(
        PartitionKey::encode_components(&["é🦀".as_bytes()])?.as_bytes(),
        "fpk:6:é🦀".as_bytes()
    );
    assert_eq!(
        PartitionKey::encode_components(&[b":\0\xff".as_slice(), b"12:3"])?.as_bytes(),
        b"fpk:3::\0\xff4:12:3"
    );
    assert_ne!(
        PartitionKey::encode_components(&[b"a", b"b"])?,
        PartitionKey::encode_components(&[b"b", b"a"])?
    );
    Ok(())
}

#[test]
fn decimal_length_boundaries_have_exact_vectors_without_a_storage_size_cap()
-> Result<(), ValueError> {
    for (length, digits) in [
        (0, "0"),
        (1, "1"),
        (9, "9"),
        (10, "10"),
        (99, "99"),
        (100, "100"),
        (999, "999"),
        (1_000, "1000"),
        (65_535, "65535"),
        (65_536, "65536"),
    ] {
        let component = vec![b':'; length];
        let mut expected = format!("fpk:{digits}:").into_bytes();
        expected.extend_from_slice(&component);
        assert_eq!(
            PartitionKey::encode_components(&[&component])?.as_bytes(),
            expected
        );
    }
    Ok(())
}

#[test]
fn every_single_byte_is_encoded_verbatim() -> Result<(), ValueError> {
    for byte in u8::MIN..=u8::MAX {
        let mut expected = b"fpk:1:".to_vec();
        expected.push(byte);
        assert_eq!(
            PartitionKey::encode_components(&[&[byte]])?.as_bytes(),
            expected
        );
    }
    Ok(())
}

#[test]
fn exhaustive_small_binary_component_lists_are_collision_free() -> Result<(), ValueError> {
    let alphabet = [0, 255, b':', b'0', b'1'];
    let mut components = vec![Vec::new()];
    for first in alphabet {
        components.push(vec![first]);
        for second in alphabet {
            components.push(vec![first, second]);
        }
    }
    assert_eq!(components.len(), 31);
    let mut encodings = BTreeMap::new();
    for first in &components {
        assert_unique(&[first], &mut encodings)?;
        for second in &components {
            assert_unique(&[first, second], &mut encodings)?;
            for third in &components {
                assert_unique(&[first, second, third], &mut encodings)?;
            }
        }
    }
    assert_eq!(encodings.len(), 30_783);
    Ok(())
}

fn assert_unique(
    components: &[&[u8]],
    encodings: &mut BTreeMap<Vec<u8>, Vec<Vec<u8>>>,
) -> Result<(), ValueError> {
    let key = PartitionKey::encode_components(components)?;
    let input: Vec<Vec<u8>> = components
        .iter()
        .map(|component| component.to_vec())
        .collect();
    let previous = encodings.insert(key.as_bytes().to_vec(), input);
    assert!(
        previous.is_none(),
        "collision for {components:?}; previous input: {previous:?}"
    );
    Ok(())
}
