//! Source v1 cursor grammar, full cohort validation and fair traversal updates.
use wf_domain::governance::release::ReleaseReconcileCursor as C;
type R<T = ()> = Result<T, Box<dyn std::error::Error>>;

#[test]
fn reconcile_cursor_source_v1_roundtrip_rotates_and_preserves_sparse_positions() -> R {
    let mut cursor = C::decode(Some(b"v1:2|1=17,0=257"), 4, 4096)?;
    assert_eq!(cursor.next_shard(), 2);
    assert_eq!(cursor.position(0), Some(257));
    assert_eq!(cursor.encode(4096)?, b"v1:2|0=257,1=17");
    cursor.advance(2, Some(513))?;
    assert_eq!(cursor.encode(4096)?, b"v1:3|0=257,1=17,2=513");
    cursor.advance(0, None)?;
    assert_eq!(cursor.encode(4096)?, b"v1:1|1=17,2=513");
    Ok(())
}

#[test]
fn reconcile_cursor_accepts_source_integer_spelling_and_refuses_complete_malformed_cohorts() -> R {
    let cursor = C::decode(Some(b"v1:+00|-0=0001,+1=+002"), 4, 4096)?;
    assert_eq!(cursor.encode(4096)?, b"v1:0|0=1,1=2");
    for wire in [
        b"v1:2".as_slice(),
        b"v2:2|0=257",
        b"v1:2|0=1,0=2",
        b"v1:4|",
        b"v1:0|1=0",
        b"v1:0|1=-1",
        b"v1:0|1=9007199254740992",
        b"v1:0|1=1,",
        b"v1:0|1=1|",
        b"v1:0|4=1",
        b"v1:0|1=1,01=2",
    ] {
        assert!(C::decode(Some(wire), 4, 4096).is_err(), "{wire:?}");
    }
    assert!(C::decode(None, 0, 4096).is_err());
    assert!(C::decode(Some(b"v1:0|0=1"), 1, 1).is_err());
    let mut cursor = C::decode(None, 4, 4096)?;
    assert_eq!(cursor.encode(4096)?, b"v1:0|");
    assert!(cursor.advance(4, None).is_err());
    assert!(cursor.advance(0, Some(0)).is_err());
    cursor.advance(3, None)?;
    assert_eq!(cursor.next_shard(), 0);
    assert!(cursor.encode(1).is_err());
    Ok(())
}
