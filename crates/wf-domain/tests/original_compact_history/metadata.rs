use std::borrow::Cow;

use super::{C, E, L, TestResult, decode, fixtures as f, prepare};
use wf_domain::history::metadata::OriginalHistoryMetadataField;

#[test]
fn compact_history_metadata_preserves_order_and_filters_null_pairs() -> TestResult {
    f::healthy(&f::empty(), f::limits())?;
    let metadata = [
        OriginalHistoryMetadataField {
            key: Cow::Borrowed(b"alias"),
            value: Cow::Borrowed(b"first"),
        },
        OriginalHistoryMetadataField {
            key: Cow::Borrowed(b"alias"),
            value: Cow::Borrowed(b"last"),
        },
        OriginalHistoryMetadataField {
            key: Cow::Borrowed(b"\xff\0"),
            value: Cow::Borrowed(b"\0\xff"),
        },
    ];
    let mut snapshot = f::empty();
    snapshot.metadata = &metadata;
    let encoded = f::healthy(&snapshot, f::limits())?;
    let decoded = f::decoded(encoded.as_bytes(), C::default(), f::limits())?;
    assert_eq!(
        &f::pairs(&decoded)[21..],
        &[
            (b"alias".as_slice(), Some(b"first".as_slice())),
            (b"alias".as_slice(), Some(b"last".as_slice())),
            (b"\xff\0".as_slice(), Some(b"\0\xff".as_slice()))
        ]
    );
    assert_eq!(f::value(&decoded, b"alias"), Some(b"last".as_slice()));
    let borrowed = decoded.fields()[21]
        .value
        .as_deref()
        .ok_or("metadata binary")?;
    assert!(
        encoded
            .as_bytes()
            .windows(borrowed.len())
            .any(|bytes| bytes.as_ptr() == borrowed.as_ptr())
    );
    let limited = L {
        max_metadata_pairs: 2,
        ..f::limits()
    };
    assert!(matches!(prepare(&snapshot, limited), Err(E::Limit)));
    assert!(matches!(
        decode(encoded.as_bytes(), C::default(), limited),
        Err(E::Limit)
    ));
    let nullable = b"FSH2\x81\x20\x02e\x02\x01\x02s\x04\x00\x02x\x02a\x00\x02a\x02b";
    let filtered = f::decoded(nullable, C::default(), f::limits())?;
    assert_eq!(filtered.fields().len(), 22);
    assert_eq!(
        f::pairs(&filtered)[21],
        (b"a".as_slice(), Some(b"b".as_slice()))
    );
    assert!(matches!(
        decode(nullable, C::default(), limited),
        Err(E::Limit)
    ));
    Ok(())
}
