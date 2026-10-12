//! Structural count/field/scalar boundaries precede decoded variable ownership.

use wf_domain::workflow::admission::AdmissionLimits;
use wf_ports::consensus::PreparedOperation;
use wf_ports::log::{CodecError, CodecLimits, decode_prepared, encode_prepared};

use super::{TestResult, request};

#[test]
fn unknown_claim_optional_tags_and_count_words_reject_before_ownership() -> TestResult {
    let encoded = encode_prepared(&request("claim_any")?, CodecLimits::default())?;
    assert_eq!(encoded.as_bytes().len(), 125);
    for offset in [86, 91, 92] {
        let mut changed = encoded.as_bytes().to_vec();
        changed[offset] = 2;
        assert!(matches!(
            decode_prepared(&changed, CodecLimits::default()),
            Err(CodecError::InvalidOptionality)
        ));
    }
    let mut changed = encoded.as_bytes().to_vec();
    changed[87..91].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::ItemLimit)
    ));
    changed[87..91].copy_from_slice(&100_000_u32.to_be_bytes());
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::Malformed)
    ));
    let mut expected = request("claim_states_empty")?;
    let PreparedOperation::ClaimPhase(value) = &mut expected.operation else {
        return Err("missing claim".into());
    };
    value.query.states = Some(vec![Vec::new(); 65]);
    let encoded = encode_prepared(&expected, CodecLimits::default())?;
    assert_eq!(
        decode_prepared(encoded.as_bytes(), CodecLimits::default())?.request(),
        &expected
    );
    if let Ok(overflow) = usize::try_from(u64::from(u32::MAX) + 1) {
        for (limit, scan, head) in [(overflow, 3, 4), (1, overflow, 4), (1, 3, overflow)] {
            let mut expected = request("claim_any")?;
            let PreparedOperation::ClaimPhase(value) = &mut expected.operation else {
                return Err("missing claim".into());
            };
            value.query.limit = limit;
            value.scan_budget = scan;
            value.head_budget = head;
            assert!(matches!(
                encode_prepared(&expected, CodecLimits::default()),
                Err(CodecError::LengthOverflow)
            ));
        }
    }
    Ok(())
}

#[test]
fn exact_entry_field_and_filter_limits_apply_on_both_paths() -> TestResult {
    let expected = request("claim_any")?;
    let encoded = encode_prepared(&expected, CodecLimits::default())?;
    for (inline, entry, error) in [
        (2, 125, None),
        (2, 124, Some(CodecError::ByteLimit)),
        (1, 125, Some(CodecError::ValueLimit)),
    ] {
        let limits = CodecLimits::new(AdmissionLimits::new(inline, entry, 1)?);
        match error {
            None => {
                assert!(encode_prepared(&expected, limits).is_ok());
                assert!(decode_prepared(encoded.as_bytes(), limits).is_ok());
            }
            Some(error) => {
                assert!(
                    matches!(encode_prepared(&expected, limits), Err(actual) if actual == error)
                );
                assert!(
                    matches!(decode_prepared(encoded.as_bytes(), limits), Err(actual) if actual == error)
                );
            }
        }
    }
    for list in 0..3 {
        let mut expected = request("claim_any")?;
        let PreparedOperation::ClaimPhase(value) = &mut expected.operation else {
            return Err("missing claim".into());
        };
        match list {
            0 => value.query.states = Some(vec![Vec::new(); 100_000]),
            1 => value.query.exclude_states = vec![Vec::new(); 100_000],
            _ => value.query.partitions = Some(vec![Vec::new(); 100_000]),
        }
        let actual = encode_prepared(&expected, CodecLimits::default())?;
        assert_eq!(
            decode_prepared(actual.as_bytes(), CodecLimits::default())?.request(),
            &expected
        );
        let PreparedOperation::ClaimPhase(value) = &mut expected.operation else {
            return Err("missing claim".into());
        };
        match list {
            0 => value
                .query
                .states
                .as_mut()
                .ok_or("missing states")?
                .push(Vec::new()),
            1 => value.query.exclude_states.push(Vec::new()),
            _ => value
                .query
                .partitions
                .as_mut()
                .ok_or("missing partitions")?
                .push(Vec::new()),
        }
        assert!(matches!(
            encode_prepared(&expected, CodecLimits::default()),
            Err(CodecError::ItemLimit)
        ));
        let offset = [87, 87, 92][list];
        let mut changed = actual.as_bytes().to_vec();
        changed[offset..offset + 4].copy_from_slice(&100_001_u32.to_be_bytes());
        assert!(matches!(
            decode_prepared(&changed, CodecLimits::default()),
            Err(CodecError::ItemLimit)
        ));
    }
    let mut expected = request("claim_any")?;
    let PreparedOperation::ClaimPhase(value) = &mut expected.operation else {
        return Err("missing claim".into());
    };
    value.query.states = Some(vec![Vec::new(); 100_000]);
    value.query.exclude_states = vec![Vec::new(); 100_000];
    value.query.partitions = Some(vec![Vec::new(); 100_000]);
    let actual = encode_prepared(&expected, CodecLimits::default())?;
    assert_eq!(
        decode_prepared(actual.as_bytes(), CodecLimits::default())?.request(),
        &expected
    );
    let mut changed = actual.as_bytes().to_vec();
    changed.drain(800_100..1_200_100);
    assert!(matches!(
        decode_prepared(&changed, CodecLimits::default()),
        Err(CodecError::Malformed)
    ));
    Ok(())
}
