use super::{DError, I, N, TestResult, decode, fixtures as f};

#[test]
fn source_term_codec_roundtrips_current_uncompressed_deterministic_format() -> TestResult {
    let encoded = f::current()?;
    assert!(matches!(encoded.as_external_term(), [131, _, ..]));
    assert_ne!(encoded.as_external_term().get(1), Some(&80));
    let value = f::decoded(
        encoded.into_external_term(),
        &f::context(),
        f::decode_limits(),
    )?;
    assert_eq!(
        f::source_shape(&value)?,
        (
            b"current".as_slice(),
            b"value".as_slice(),
            b"payload".as_slice()
        )
    );
    Ok(())
}

#[test]
fn source_term_codec_rejects_compressed_and_trailing_external_terms() -> TestResult {
    f::current()?;
    let compressed = super::fixtures::compressed();
    assert!(matches!(compressed.as_slice(), [131, 80, ..]));
    assert!(matches!(
        decode(compressed, &f::context(), f::decode_limits()),
        Err(DError::InvalidFlowValue)
    ));
    let nodes = [
        N::Atom {
            bytes: 0..7,
            utf8: true,
        },
        N::Atom {
            bytes: 7..12,
            utf8: true,
        },
        N::Tuple { children: 0..2 },
    ];
    let current = f::healthy(
        I {
            bytes: b"currentvalue",
            nodes: &nodes,
            edges: &[0, 1],
            root: 2,
        },
        f::limits(),
    )?;
    let mut trailing = current.into_external_term();
    trailing.push(0);
    assert!(matches!(
        decode(trailing, &f::context(), f::decode_limits()),
        Err(DError::InvalidFlowValue)
    ));
    exact_compressible_recipe()?;
    Ok(())
}

fn exact_compressible_recipe() -> TestResult {
    let mut bytes = b"current".to_vec();
    bytes.extend_from_slice(&b"compressible".repeat(1024));
    let nodes = [
        N::Atom {
            bytes: 0..7,
            utf8: true,
        },
        N::Binary {
            bytes: 7..bytes.len(),
        },
        N::Tuple { children: 0..2 },
    ];
    let encoded = f::healthy(
        I {
            bytes: &bytes,
            nodes: &nodes,
            edges: &[0, 1],
            root: 2,
        },
        f::limits(),
    )?;
    let mut expected = b"\x83h\x02w\x07currentm\0\0\x30\0".to_vec();
    expected.extend_from_slice(&b"compressible".repeat(1024));
    assert_eq!(encoded.as_external_term(), expected);
    Ok(())
}
