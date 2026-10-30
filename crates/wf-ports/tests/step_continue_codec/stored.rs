//! Explicit stored Step and independent wire recipe; old binary bytes stay closed.

use super::{TestResult, WORKSPACE, fixtures as f};
use wf_ports::log::{
    CodecError, RecordBatchApplyError, encode_step_continue, encode_stored_step_continue,
    prepare_step_continue, prepare_stored_step_continue,
};

const PAYLOAD: &[u8] = b"FSV2\x02\x83t\0\0\0\x01w\x04stepm\0\0\0\x0bcharge_card";
const FRAME: &str = "4657435000010100000001740000000167000000017a0000000171000ba7502abccdb5252cd02ed46c1ca67d97ba5556ed240b1e96818dc7edf5a3e3b100000000000000020000000000000000000200000001000000017000000001690000000000000000020c000000056c65617365000000000000000700000009617574686f72697a6500000006636861726765000000000000007530010000002146535632028374000000017704737465706d0000000b6368617267655f63617264";

#[test]
fn stored_step_codec_derives_independent_frame_without_reinterpreting_plain_bytes() -> TestResult {
    let mut request = f::request()?;
    f::step_mut(&mut request)?.payload = Some(PAYLOAD.to_vec());
    let encoded = encode_stored_step_continue(request, limits()?, WORKSPACE);
    assert!(
        encoded.is_ok(),
        "healthy stored Step encode: {:?}",
        encoded.as_ref().err()
    );
    let encoded = encoded?;
    assert_eq!(encoded.as_bytes(), f::bytes(FRAME)?);
    assert_eq!(
        *encoded.digest().as_bytes(),
        f::digest("a438db0c25662e4c28e3a0322e4bc49715d8505400e7fdee10cd90f26991aaa9")?
    );
    let decoded = prepare_stored_step_continue(encoded.as_bytes(), limits()?, WORKSPACE)?;
    assert_eq!(decoded.request().intent.version(), 11);
    assert_eq!(
        *decoded.request().intent.digest(),
        f::digest("a7502abccdb5252cd02ed46c1ca67d97ba5556ed240b1e96818dc7edf5a3e3b1")?
    );
    assert_eq!(decoded.value_bytes(0), Some([Some(PAYLOAD), None]));
    assert!(prepare_step_continue(encoded.as_bytes(), limits()?, WORKSPACE).is_err());
    assert_eq!(
        prepare_stored_step_continue(encoded.as_bytes(), f::limits()?, WORKSPACE).err(),
        Some(RecordBatchApplyError::Encoding(CodecError::ValueLimit))
    );
    let mut request = f::request()?;
    f::step_mut(&mut request)?.payload = Some(PAYLOAD.to_vec());
    let plain = encode_step_continue(request, limits()?, WORKSPACE)?;
    let plain_decoded = prepare_step_continue(plain.as_bytes(), limits()?, WORKSPACE)?;
    assert_eq!(plain_decoded.value_bytes(0), Some([Some(PAYLOAD), None]));
    assert_ne!(plain_decoded.request().intent, decoded.request().intent);
    assert!(prepare_stored_step_continue(plain.as_bytes(), limits()?, WORKSPACE).is_err());
    Ok(())
}

#[test]
fn stored_step_codec_prepares_independent_literal_before_corruption_and_limits() -> TestResult {
    let bytes = f::bytes(FRAME)?;
    let decoded = prepare_stored_step_continue(&bytes, limits()?, WORKSPACE);
    assert!(
        decoded.is_ok(),
        "healthy stored Step literal: {:?}",
        decoded.as_ref().err()
    );
    assert_eq!(decoded?.value_bytes(0), Some([Some(PAYLOAD), None]));
    for index in [29, 35, bytes.len() - 1] {
        let mut corrupt = bytes.clone();
        corrupt[index] ^= 1;
        assert!(prepare_stored_step_continue(&corrupt, limits()?, WORKSPACE).is_err());
    }
    let mut tail = bytes.clone();
    tail.push(0);
    assert!(prepare_stored_step_continue(&tail, limits()?, WORKSPACE).is_err());
    assert!(matches!(
        prepare_stored_step_continue(&bytes, limits()?, 1),
        Err(RecordBatchApplyError::Workspace { attempted, maximum: 1 }) if attempted > 1
    ));
    assert!(matches!(
        prepare_step_continue(&bytes, limits()?, WORKSPACE),
        Err(RecordBatchApplyError::Encoding(
            CodecError::UnsupportedVersion
        ))
    ));
    Ok(())
}

fn limits() -> TestResult<wf_ports::log::CodecLimits> {
    Ok(wf_ports::log::CodecLimits::new(
        wf_domain::workflow::admission::AdmissionLimits::new(64, 8192, 4)?,
    ))
}
