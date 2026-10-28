use super::{fixtures as f, *};
use wf_ports::log::{
    decode_prepared, encode_prepared, encode_record_batch, normal_recovery_header,
    prepare_atomic_start, prepare_record_batch,
};

#[test]
fn step_codec_rejects_truncated_tampered_and_wrong_profile_frames() -> TestResult {
    let encoded = f::encoded(f::request()?)?;
    let bytes = encoded.as_bytes();
    for end in 0..bytes.len() {
        assert!(prepare_step_continue(&bytes[..end], f::limits()?, WORKSPACE).is_err());
    }
    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert!(matches!(
        prepare_step_continue(&trailing, f::limits()?, WORKSPACE),
        Err(RecordBatchApplyError::Encoding(CodecError::TrailingBytes))
    ));
    for (offset, value, expected) in [
        (28, 9, CodecError::UnsupportedVersion),
        (28, 11, CodecError::UnsupportedVersion),
        (102, 11, CodecError::UnsupportedTag),
        (143, 2, CodecError::InvalidOptionality),
        (152, 2, CodecError::InvalidOptionality),
        (82, 0, CodecError::ItemLimit),
        (82, 2, CodecError::ItemLimit),
    ] {
        let mut changed = bytes.to_vec();
        changed[offset] = value;
        assert!(
            matches!(prepare_step_continue(&changed, f::limits()?, WORKSPACE), Err(RecordBatchApplyError::Encoding(error)) if error == expected)
        );
    }
    for offset in [29, 107] {
        let mut changed = bytes.to_vec();
        changed[offset] ^= 1;
        assert!(matches!(
            prepare_step_continue(&changed, f::limits()?, WORKSPACE),
            Err(RecordBatchApplyError::IntentMismatch)
        ));
    }
    Ok(())
}

#[test]
fn step_codec_keeps_legacy_dispatch_and_profiles_closed() -> TestResult {
    let request = f::request()?;
    let encoded = f::encoded(request.clone())?;
    assert!(ClientIntent::new(10, [0; 32]).is_err());
    assert!(encode_prepared(&request, f::limits()?).is_err());
    assert!(encode_record_batch(request, f::limits()?, WORKSPACE).is_err());
    assert!(decode_prepared(encoded.as_bytes(), f::limits()?).is_err());
    assert!(prepare_record_batch(encoded.as_bytes(), f::limits()?, WORKSPACE).is_err());
    assert!(prepare_atomic_start(encoded.as_bytes(), f::limits()?, WORKSPACE).is_err());
    assert!(matches!(
        normal_recovery_header(encoded.as_bytes(), f::limits()?),
        Err(CodecError::UnsupportedVersion)
    ));
    let mut nonstep = f::request()?;
    f::item_mut(&mut nonstep)?.command.operation = Operation::Cancel(wf_domain::workflow::Cancel {
        fence: Fence::new(0),
        lease_token: None,
        expected_state: None,
        expected_version: None,
        error: None,
    });
    assert!(matches!(
        encode_step_continue(nonstep, f::limits()?, WORKSPACE),
        Err(RecordBatchApplyError::Encoding(CodecError::UnsupportedTag))
    ));
    let mut multiple = f::request()?;
    let duplicate = f::item_mut(&mut multiple)?.clone();
    let PreparedOperation::Batch(items) = &mut multiple.operation else {
        return Err("fixture batch".into());
    };
    items.push(duplicate);
    assert!(matches!(
        encode_step_continue(multiple, f::limits()?, WORKSPACE),
        Err(RecordBatchApplyError::Encoding(CodecError::ItemLimit))
    ));
    Ok(())
}
