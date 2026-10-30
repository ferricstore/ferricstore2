use super::{fixtures as f, *};

#[test]
fn snapshot_bounds_admit_exact_wire_owner_and_workspace() -> TestResult {
    let snapshot = f::step(Some(b"\0\xff"))?;
    let bytes = f::encoded(&snapshot)?;
    let record = scan_record_snapshot(
        f::record_bytes(&bytes)?,
        &f::scope()?,
        &snapshot.postimage.partition,
        snapshot.postimage.record.id(),
        f::limits()?.admission(),
        8192,
    )?;
    let owned = record.owned_bytes()
        + 512
        + snapshot.event.event_id.len()
        + snapshot.event.state.len()
        + snapshot.event.worker.as_ref().map_or(0, Vec::len);
    let encode_work = bytes.len() + 2048;
    assert_eq!(
        encode_history_snapshot(
            &f::scope()?,
            &snapshot.postimage,
            &snapshot.event,
            f::caps(bytes.len(), 1, encode_work)?
        )?,
        bytes
    );
    assert_eq!(
        encode_history_snapshot(
            &f::scope()?,
            &snapshot.postimage,
            &snapshot.event,
            f::caps(bytes.len(), 1, encode_work - 1)?
        )
        .err(),
        Some(StateValueError::Limit {
            bytes: encode_work,
            maximum: encode_work - 1
        })
    );
    let decode_work = bytes.len() + owned + 2048;
    assert_eq!(
        f::decoded(&bytes, f::caps(bytes.len(), owned, decode_work)?)?,
        snapshot
    );
    assert_eq!(
        decode_history_snapshot(
            &bytes,
            &f::scope()?,
            &snapshot.postimage.partition,
            snapshot.postimage.record.id(),
            f::caps(bytes.len() - 1, owned, decode_work)?
        )
        .err(),
        Some(StateValueError::Limit {
            bytes: bytes.len(),
            maximum: bytes.len() - 1
        })
    );
    assert_eq!(
        decode_history_snapshot(
            &bytes,
            &f::scope()?,
            &snapshot.postimage.partition,
            snapshot.postimage.record.id(),
            f::caps(bytes.len(), owned - 1, decode_work)?
        )
        .err(),
        Some(StateValueError::Limit {
            bytes: owned,
            maximum: owned - 1
        })
    );
    assert_eq!(
        decode_history_snapshot(
            &bytes,
            &f::scope()?,
            &snapshot.postimage.partition,
            snapshot.postimage.record.id(),
            f::caps(bytes.len(), owned, decode_work - 1)?
        )
        .err(),
        Some(StateValueError::Limit {
            bytes: decode_work,
            maximum: decode_work - 1
        })
    );
    let large = f::step(Some(&[0; 33]))?;
    assert!(
        encode_history_snapshot(&f::scope()?, &large.postimage, &large.event, f::limits()?)
            .is_err()
    );
    for (encoded, owned, work) in [(0, 1, 1), (1, 0, 1), (1, 1, 0)] {
        assert!(
            HistorySnapshotLimits::new(f::limits()?.admission(), encoded, owned, work).is_err()
        );
    }
    Ok(())
}
