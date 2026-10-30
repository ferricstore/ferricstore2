use super::{fixtures as f, *};

#[test]
fn record_snapshot_version_is_explicit_and_all_event_tags_roundtrip() -> TestResult {
    let base = f::step(Some(b"\0\xff"))?;
    let first = encode_record_snapshot(
        &f::scope()?,
        &base.postimage.partition,
        &base.postimage.record,
        base.postimage.ordering,
        f::limits()?.admission(),
        8192,
    );
    assert!(
        first.is_ok(),
        "healthy record snapshot: {:?}",
        first.as_ref().err()
    );
    let first = first?;
    assert!(matches!(
        scan_record_point(
            &first,
            &f::scope()?,
            &base.postimage.partition,
            base.postimage.record.id(),
            f::limits()?.admission(),
            8192
        ),
        Err(StateValueError::UnsupportedVersion)
    ));
    assert_eq!(
        encode_record_fields(
            &f::scope()?,
            &base.postimage.partition,
            &base.postimage.record,
            base.postimage.ordering,
            f::limits()?.admission(),
            8192
        ),
        Err(StateValueError::InvalidValue)
    );
    for kind in [
        EventKind::Created,
        EventKind::Claimed,
        EventKind::Transitioned,
        EventKind::Completed,
        EventKind::Failed,
        EventKind::Retry,
        EventKind::LeaseExtended,
        EventKind::Cancelled,
        EventKind::ChildCompleted,
        EventKind::ChildFailed,
        EventKind::ChildCancelled,
        EventKind::ChildrenCancelled,
        EventKind::ChildrenSpawned,
        EventKind::StepContinued,
    ] {
        let snapshot = f::kind(&base, kind)?;
        let bytes = encode_record_snapshot(
            &f::scope()?,
            &snapshot.postimage.partition,
            &snapshot.postimage.record,
            snapshot.postimage.ordering,
            f::limits()?.admission(),
            8192,
        )?;
        let scanned = scan_record_snapshot(
            &bytes,
            &f::scope()?,
            &snapshot.postimage.partition,
            snapshot.postimage.record.id(),
            f::limits()?.admission(),
            8192,
        )?;
        assert_eq!(scanned.into_owned(16384)?, snapshot.postimage);
        let all = f::encoded(&snapshot)?;
        assert_eq!(f::decoded(&all, f::limits()?)?, snapshot);
        if kind != EventKind::StepContinued {
            let mut legacy = encode_record_fields(
                &f::scope()?,
                &snapshot.postimage.partition,
                &snapshot.postimage.record,
                snapshot.postimage.ordering,
                f::limits()?.admission(),
                8192,
            )?;
            assert!(matches!(
                scan_record_snapshot(
                    &legacy,
                    &f::scope()?,
                    &snapshot.postimage.partition,
                    snapshot.postimage.record.id(),
                    f::limits()?.admission(),
                    8192
                ),
                Err(StateValueError::UnsupportedVersion)
            ));
            legacy[4..6].copy_from_slice(&2_u16.to_be_bytes());
            assert_eq!(legacy, bytes);
        }
    }
    Ok(())
}
