//! Canonical golden bytes independently produced with Python struct/hashlib.

use wf_ports::log::{CodecError, CodecLimits, decode_prepared, encode_prepared};

use super::{TestResult, bytes, request};

#[test]
fn every_claim_phase_vector_matches_independent_bytes_and_hash() -> TestResult {
    let mut count = 0;
    for line in include_str!("../../../../tests/prepared/claim-phase-v1-vectors.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let name = fields.next().ok_or("missing name")?;
        let encoded = bytes(fields.next().ok_or("missing bytes")?)?;
        let digest = bytes(fields.next().ok_or("missing digest")?)?;
        assert_eq!(fields.next(), None);
        let expected = request(name)?;
        let actual = encode_prepared(&expected, CodecLimits::default())?;
        assert_eq!(actual.as_bytes(), encoded, "{name}");
        assert_eq!(actual.digest().as_bytes().as_slice(), digest, "{name}");
        let decoded = decode_prepared(&encoded, CodecLimits::default())?;
        assert_eq!(decoded.request(), &expected, "{name}");
        assert_eq!(decoded.digest(), actual.digest());
        count += 1;
    }
    assert_eq!(count, 21);
    Ok(())
}

#[test]
fn claim_filter_order_and_optionality_have_distinct_prepared_identity() -> TestResult {
    for (left, right) in [
        ("claim_states", "claim_states_reverse"),
        ("claim_excluded", "claim_excluded_reverse"),
        ("claim_partitions", "claim_partitions_reverse"),
        ("claim_any", "claim_states_empty"),
        ("claim_any", "claim_partitions_empty"),
        ("claim_any", "claim_priority_zero"),
    ] {
        let first = encode_prepared(&request(left)?, CodecLimits::default())?;
        let second = encode_prepared(&request(right)?, CodecLimits::default())?;
        assert_ne!(first.as_bytes(), second.as_bytes(), "{left}/{right}");
        assert_ne!(first.digest(), second.digest(), "{left}/{right}");
    }
    Ok(())
}

#[test]
fn every_claim_phase_truncation_and_trailing_byte_rejects() -> TestResult {
    for line in include_str!("../../../../tests/prepared/claim-phase-v1-vectors.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let name = fields.next().ok_or("missing fixture name")?;
        let encoded = bytes(fields.next().ok_or("missing fixture bytes")?)?;
        for end in 0..encoded.len() {
            assert!(
                decode_prepared(&encoded[..end], CodecLimits::default()).is_err(),
                "{name}/{end}"
            );
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert!(
            matches!(
                decode_prepared(&trailing, CodecLimits::default()),
                Err(CodecError::TrailingBytes)
            ),
            "{name}"
        );
    }
    Ok(())
}
