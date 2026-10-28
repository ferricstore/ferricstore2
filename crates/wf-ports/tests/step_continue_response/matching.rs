use super::{fixtures as f, *};

fn check_alternative(to: &[u8], payload: Option<&[u8]>, original: &[u8]) -> TestResult {
    f::with_step(to, payload, |decision| {
        let (bytes, status) =
            encode_step_continue_response(decision, f::position(), f::time(), f::limits()?)?;
        assert_eq!(status, RetainedStatus::Accepted);
        assert_eq!(f::summary(&bytes)?, (status, Some(1)));
        assert!(step_continue_response_matches(
            decision,
            &bytes,
            f::position(),
            f::time(),
            f::limits()?
        )?);
        assert!(!step_continue_response_matches(
            decision,
            original,
            f::position(),
            f::time(),
            f::limits()?
        )?);
        if to == b"authorize" {
            let IndexedDecision::Accepted(plan) = decision else {
                return Err("actual Step".into());
            };
            assert_eq!(plan.consumed_ordinals(), 0);
            assert_eq!(plan.metadata().next().ok_or("ordering")?.state_enter_seq, 5);
        }
        Ok(())
    })
}

fn check_legacy_fields() -> TestResult {
    f::with_legacy_step(|decision| {
        let (bytes, status) =
            encode_step_continue_response(decision, f::position(), f::time(), f::limits()?)?;
        assert_eq!(f::summary(&bytes)?, (status, Some(1)));
        assert!(step_continue_response_matches(
            decision,
            &bytes,
            f::position(),
            f::time(),
            f::limits()?
        )?);
        let IndexedDecision::Accepted(plan) = decision else {
            return Err("legacy Step".into());
        };
        let record = &plan.batch().items()[0]
            .plan()
            .write()
            .ok_or("legacy write")?
            .record;
        assert_eq!(
            (record.version(), record.fence().get(), record.attempts()),
            (2, 1, 0)
        );
        assert_eq!(
            (record.created_at().get(), record.updated_at().get()),
            (7, 2)
        );
        assert_eq!(record.payload(), Some(b"old".as_slice()));
        assert_eq!(record.result(), Some(b"res\0\xff".as_slice()));
        assert_eq!(record.error(), Some(b"".as_slice()));
        assert_eq!(
            record.lease().ok_or("legacy lease")?.token.as_bytes(),
            b"w\0\xff:2:1"
        );
        Ok(())
    })
}

#[test]
fn reply_matching_uses_actual_postimages_and_equal_state_ordering() -> TestResult {
    let original = f::reply()?;
    for (to, payload) in [
        (b"invoice".as_slice(), Some(b"\0\xff".as_slice())),
        (b"charge".as_slice(), None),
        (b"charge".as_slice(), Some(b"".as_slice())),
        (b"authorize".as_slice(), Some(b"\0\xff".as_slice())),
    ] {
        check_alternative(to, payload, &original)?;
    }
    check_legacy_fields()?;
    let mut alternative = original;
    alternative[156] ^= 1;
    assert!(f::summary(&alternative).is_ok());
    f::with_step(b"charge", Some(b"\0\xff"), |decision| {
        assert!(!step_continue_response_matches(
            decision,
            &alternative,
            f::position(),
            f::time(),
            f::limits()?
        )?);
        Ok(())
    })
}
