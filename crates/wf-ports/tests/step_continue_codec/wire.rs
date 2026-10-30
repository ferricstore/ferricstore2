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

#[test]
fn explicit_step_recovery_header_borrows_golden_and_keeps_old_profiles_closed() -> TestResult {
    use wf_ports::log::{
        normal_recovery_header, step_continue_recovery_header, stored_value_recovery_header,
    };
    let wire = f::bytes(f::FRAME)?;
    let (profile, fields, stamp) = step_continue_recovery_header(&wire, f::limits()?, false)?;
    assert_eq!(profile, 10);
    assert_eq!(fields, [b"t".as_slice(), b"g", b"z"]);
    assert_eq!(fields[0].as_ptr(), wire[11..].as_ptr());
    assert_eq!(stamp, f::clock(2)?);
    for old in [normal_recovery_header, stored_value_recovery_header] {
        assert!(matches!(
            old(&wire, f::limits()?),
            Err(CodecError::UnsupportedVersion)
        ));
    }
    let mut changed = wire.clone();
    changed[27..29].copy_from_slice(&9_u16.to_be_bytes());
    assert!(matches!(
        step_continue_recovery_header(&changed, f::limits()?, false),
        Err(CodecError::UnsupportedVersion)
    ));
    assert_eq!(
        step_continue_recovery_header(&changed, f::limits()?, true)?.0,
        9
    );
    let narrow = CodecLimits::new(AdmissionLimits::new(32, wire.len() - 1, 4)?);
    assert!(matches!(
        step_continue_recovery_header(&wire, narrow, false),
        Err(CodecError::ByteLimit)
    ));
    for end in 0..79 {
        assert!(step_continue_recovery_header(&wire[..end], f::limits()?, false).is_err());
    }
    assert_eq!(
        step_continue_recovery_header(&wire[..79], f::limits()?, false)?.0,
        10
    );
    Ok(())
}
