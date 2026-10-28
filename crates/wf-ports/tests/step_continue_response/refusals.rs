use super::{fixtures as f, *};

#[test]
fn retained_reply_rejects_truncation_profiles_and_noncanonical_fields() -> TestResult {
    let bytes = f::reply()?;
    for end in 0..bytes.len() {
        assert!(f::summary(&bytes[..end]).is_err(), "prefix {end}");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(f::summary(&trailing).is_err());
    for offset in [0, 5, 6, 14, 22, 30, 38, 79, 134, 151] {
        let mut bad = bytes.clone();
        bad[offset] = if matches!(offset, 38 | 151) {
            2
        } else {
            bad[offset] ^ 0xff
        };
        assert!(f::summary(&bad).is_err(), "offset {offset}");
    }
    for (offset, value) in [(86, 1_u64), (94, 1), (102, 2), (110, u64::MAX), (118, 3)] {
        let mut bad = bytes.clone();
        bad[offset..offset + 8].copy_from_slice(&value.to_be_bytes());
        assert!(f::summary(&bad).is_err(), "scalar {offset}");
    }
    let mut terminal = bytes.clone();
    terminal[62..68].copy_from_slice(b"failed");
    assert!(f::summary(&terminal).is_err());
    for version in [1, 4, 5, 6, 7, 8, 10] {
        assert_eq!(
            step_continue_retained_summary(
                ClientIntent::step_continue([0; 32]),
                version,
                &bytes,
                f::position(),
                f::time(),
                f::limits()?
            ),
            Err(CodecError::UnsupportedVersion)
        );
    }
    assert_eq!(
        step_continue_retained_summary(
            ClientIntent::atomic_start([0; 32]),
            9,
            &bytes,
            f::position(),
            f::time(),
            f::limits()?
        ),
        Err(CodecError::UnsupportedVersion)
    );
    assert!(
        atomic_start_retained_summary(
            ClientIntent::atomic_start([0; 32]),
            4,
            &bytes,
            f::position(),
            f::time(),
            f::limits()?
        )
        .is_err()
    );
    assert!(record_batch_retained_summary(ClientIntent::record_batch([0; 32]), 1, &bytes).is_err());
    Ok(())
}
