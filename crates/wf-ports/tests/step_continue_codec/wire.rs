use super::{fixtures as f, *};

#[test]
fn step_codec_preserves_complete_fields_and_golden_bytes() -> TestResult {
    let request = f::request()?;
    let mut expected = request.clone();
    let encoded = f::encoded(request)?;
    assert_eq!(encoded.as_bytes(), f::bytes(f::FRAME)?);
    assert_eq!(encoded.digest(), PreparedDigest::new(f::digest(f::DIGEST)?));
    let decoded = f::decoded(encoded.as_bytes())?;
    expected.intent = ClientIntent::step_continue(f::digest(f::INTENT)?);
    assert_eq!(decoded.request(), &expected);
    assert_eq!(decoded.digest(), encoded.digest());
    assert_eq!(decoded.encoded().as_ptr(), encoded.as_bytes().as_ptr());
    assert_eq!(decoded.isolation_scope(), f::bytes(f::ISOLATION)?);
    assert_eq!(decoded.state_key(0), Some(f::bytes(f::KEY)?.as_slice()));
    assert_eq!(decoded.state_key(1), None);
    assert_eq!(
        decoded.value_bytes(0),
        Some([Some(b"\0\xff".as_slice()), None])
    );
    Ok(())
}
