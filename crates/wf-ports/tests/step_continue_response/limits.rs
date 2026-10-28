use super::{fixtures as f, *};

#[test]
fn reply_admits_exact_output_and_inline_boundaries() -> TestResult {
    let bytes = f::reply()?;
    let exact = AdmissionLimits::new(32, 160, 4)?;
    let short = AdmissionLimits::new(32, 159, 4)?;
    f::with_step(b"charge", Some(b"\0\xff"), |decision| {
        assert_eq!(
            encode_step_continue_response(decision, f::position(), f::time(), exact)?.0,
            bytes
        );
        assert_eq!(
            encode_step_continue_response(decision, f::position(), f::time(), short),
            Err(CodecError::ByteLimit)
        );
        Ok(())
    })?;
    assert_eq!(
        step_continue_retained_summary(
            ClientIntent::step_continue([0; 32]),
            9,
            &bytes,
            f::position(),
            f::time(),
            short
        ),
        Err(CodecError::ByteLimit)
    );
    for length in [32, 33] {
        f::with_step(b"charge", Some(&vec![0; length]), |decision| {
            let result =
                encode_step_continue_response(decision, f::position(), f::time(), f::limits()?);
            if length == 32 {
                assert!(result.is_ok());
            } else {
                assert_eq!(result, Err(CodecError::ValueLimit));
            }
            Ok(())
        })?;
    }
    Ok(())
}
