use std::mem::size_of;

use super::{fixtures as f, *};

#[test]
fn step_codec_admits_exact_workspace_and_field_caps() -> TestResult {
    let encoded = f::encoded(f::request()?)?;
    assert_eq!(encoded.as_bytes().len(), 159);
    let write_budget = 28 + 2048 + 159;
    assert!(encode_step_continue(f::request()?, f::limits()?, write_budget).is_ok());
    assert!(
        matches!(encode_step_continue(f::request()?, f::limits()?, write_budget - 1), Err(RecordBatchApplyError::Workspace { attempted, maximum }) if attempted == write_budget && maximum == write_budget - 1)
    );
    let read_budget = 4 * 159 + 8192 + 3072 + size_of::<DecodedRequest<'_>>();
    assert!(prepare_step_continue(encoded.as_bytes(), f::limits()?, read_budget).is_ok());
    assert!(
        matches!(prepare_step_continue(encoded.as_bytes(), f::limits()?, read_budget - 1), Err(RecordBatchApplyError::Workspace { attempted, maximum }) if attempted == read_budget && maximum == read_budget - 1)
    );
    let mut exact = f::request()?;
    f::step_mut(&mut exact)?.payload = Some(vec![0; 32]);
    assert!(encode_step_continue(exact, f::limits()?, WORKSPACE).is_ok());
    let mut excessive = f::request()?;
    f::step_mut(&mut excessive)?.payload = Some(vec![0; 33]);
    assert!(matches!(
        encode_step_continue(excessive, f::limits()?, 0),
        Err(RecordBatchApplyError::Encoding(CodecError::ValueLimit))
    ));
    let short = CodecLimits::new(AdmissionLimits::new(32, 158, 4)?);
    assert!(matches!(
        encode_step_continue(f::request()?, short, WORKSPACE),
        Err(RecordBatchApplyError::Encoding(CodecError::ByteLimit))
    ));
    assert!(matches!(
        prepare_step_continue(encoded.as_bytes(), short, WORKSPACE),
        Err(RecordBatchApplyError::Encoding(CodecError::ByteLimit))
    ));
    let mut long_key = f::request()?;
    f::item_mut(&mut long_key)?.command.id = RecordId::new(vec![b'i'; 65_535])?;
    assert!(matches!(
        encode_step_continue(long_key, f::limits()?, WORKSPACE),
        Err(RecordBatchApplyError::Encoding(CodecError::KeyLimit))
    ));
    Ok(())
}

#[test]
fn step_codec_field_caps_precede_workspace_on_both_entries() -> TestResult {
    f::encoded(f::request()?)?;
    let mut request = f::request()?;
    f::step_mut(&mut request)?.payload = Some(vec![0; 33]);
    assert!(matches!(
        encode_step_continue(request.clone(), f::limits()?, 0),
        Err(RecordBatchApplyError::Encoding(CodecError::ValueLimit))
    ));
    let larger = CodecLimits::new(AdmissionLimits::new(33, 8192, 4)?);
    let encoded = encode_step_continue(request, larger, WORKSPACE)?;
    assert!(matches!(
        prepare_step_continue(encoded.as_bytes(), f::limits()?, 0),
        Err(RecordBatchApplyError::Encoding(CodecError::ValueLimit))
    ));
    Ok(())
}
