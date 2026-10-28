use super::{C, TestResult, fixtures as f};

#[test]
fn compact_history_decoder_preserves_permissive_wire_and_empty_fallback() -> TestResult {
    let encoded = f::healthy(&f::empty(), f::limits())?;
    let canonical = f::decoded(encoded.as_bytes(), C::default(), f::limits())?;
    for prefix in 0..encoded.as_bytes().len() {
        assert!(
            f::decoded(&encoded.as_bytes()[..prefix], C::default(), f::limits())?
                .fields()
                .is_empty()
        );
    }
    for invalid in [
        b"FSH1\x01\x02e\x02\x01\x02s".as_slice(),
        b"FSH2\x00",
        b"FSH2\x01\x02e\x02\x01\x02s\0",
    ] {
        assert!(
            f::decoded(invalid, C::default(), f::limits())?
                .fields()
                .is_empty()
        );
    }
    for permissive in [
        b"FSH2\x81\x00\x02e\x02\x01\x02s".as_slice(),
        b"FSH2\x81\x40\x02e\x02\x01\x02s",
    ] {
        let decoded = f::decoded(permissive, C::default(), f::limits())?;
        assert_eq!(f::pairs(&decoded), f::pairs(&canonical));
    }
    let nil_event = f::decoded(b"FSH2\x01\x00\x02\x01\x02s", C::default(), f::limits())?;
    assert!(nil_event.fields()[0].value.is_none());
    Ok(())
}
