use super::{fixtures as f, *};

#[test]
fn snapshot_rejects_truncation_namespace_and_mismatched_events() -> TestResult {
    let snapshot = f::step(Some(b"\0\xff"))?;
    let bytes = f::encoded(&snapshot)?;
    for end in 0..bytes.len() {
        assert!(
            f::decoded(&bytes[..end], f::limits()?).is_err(),
            "prefix {end}"
        );
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(f::decoded(&trailing, f::limits()?).is_err());
    for offset in [
        0, 5, 6, 14, 118, 127, 138, 165, 172, 173, 174, 191, 199, 233, 242, 250, 270, 277, 294,
        299, 302,
    ] {
        let mut bad = bytes.clone();
        bad[offset] ^= 0xff;
        assert!(f::decoded(&bad, f::limits()?).is_err(), "offset {offset}");
    }
    let mut mismatch = bytes.clone();
    mismatch[270] = b'x';
    assert_eq!(
        decode_history_snapshot(
            &mismatch,
            &f::scope()?,
            &snapshot.postimage.partition,
            snapshot.postimage.record.id(),
            f::caps(8192, 1, 1)?
        )
        .err(),
        Some(StateValueError::InvalidValue)
    );
    for scope in [
        RequestScope::new(
            TenantId::new(b"other".to_vec())?,
            GroupId::new(b"g".to_vec())?,
            RecoveryId::new(b"z".to_vec())?,
        ),
        RequestScope::new(
            TenantId::new(b"t".to_vec())?,
            GroupId::new(b"other".to_vec())?,
            RecoveryId::new(b"z".to_vec())?,
        ),
    ] {
        assert!(
            decode_history_snapshot(
                &bytes,
                &scope,
                &snapshot.postimage.partition,
                snapshot.postimage.record.id(),
                f::limits()?
            )
            .is_err()
        );
    }
    assert!(
        decode_history_snapshot(
            &bytes,
            &f::scope()?,
            &PartitionId::new(b"other".to_vec())?,
            snapshot.postimage.record.id(),
            f::limits()?
        )
        .is_err()
    );
    assert!(
        decode_history_snapshot(
            &bytes,
            &f::scope()?,
            &snapshot.postimage.partition,
            &RecordId::new(b"other".to_vec())?,
            f::limits()?
        )
        .is_err()
    );
    let mut wrong = snapshot.clone();
    wrong.event.fence = Fence::new(9);
    assert!(
        encode_history_snapshot(&f::scope()?, &wrong.postimage, &wrong.event, f::limits()?)
            .is_err()
    );
    let raw_history = &bytes[195..];
    assert!(matches!(
        decode_history(
            raw_history,
            &f::scope()?,
            &snapshot.postimage.partition,
            snapshot.postimage.record.id(),
            8192
        ),
        Err(StateValueError::UnsupportedVersion)
    ));
    Ok(())
}
