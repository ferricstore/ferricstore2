use super::{M, TestResult, fixtures as f};

#[test]
fn source_history_codec_keeps_terminal_metadata_and_refs() -> TestResult {
    let nodes = f::metadata_nodes(8);
    let metadata = f::normalized(b"reasonok", &nodes)?;
    let mut snapshot = f::base(metadata.fields());
    snapshot.state = Some(b"completed");
    snapshot.references[2] = Some(b"flow/value/a/result/2");
    let encoded = f::healthy(&snapshot, f::limits())?;
    let reference = f::terminal_wire();
    assert_eq!(encoded.as_bytes(), reference);
    let decoded = f::decoded(encoded.as_bytes(), snapshot.context, f::limits())?;
    let mut expected = f::base_fields();
    expected[5] = (b"state", Some(b"completed"));
    expected[18] = (b"result_ref", Some(b"flow/value/a/result/2"));
    expected.push((b"reason", Some(b"ok")));
    assert_eq!(f::pairs(&decoded), expected);
    assert_eq!(f::value(&decoded, b"event"), Some(b"123-1".as_slice()));
    assert_eq!(f::value(&decoded, b"version"), Some(b"1".as_slice()));
    assert_eq!(f::value(&decoded, b"state"), Some(b"completed".as_slice()));
    assert_eq!(
        f::value(&decoded, b"result_ref"),
        Some(b"flow/value/a/result/2".as_slice())
    );
    assert_eq!(f::value(&decoded, b"reason"), Some(b"ok".as_slice()));
    Ok(())
}

#[test]
fn source_history_codec_omits_immutable_metadata_and_restores_context() -> TestResult {
    let mut snapshot = f::base(&[]);
    snapshot.event = b"transition";
    snapshot.version = M::Integer(12);
    snapshot.at = M::Integer(2000);
    snapshot.numbers = [
        M::Integer(0),
        M::Integer(2),
        M::Integer(9),
        M::Integer(1000),
        M::Integer(2000),
        M::Nil,
        M::Integer(0),
    ];
    snapshot.references = [None, Some(b"shared-payload-ref"), None, None, None];
    snapshot.context.id = Some(b"flow-compact-history");
    snapshot.context.workflow_type = Some(b"iot-fanout-worker");
    snapshot.context.root_flow_id = Some(b"flow-compact-history");
    snapshot.context.correlation_id = Some(b"device-batch-2026-05-23");
    let encoded = f::healthy(&snapshot, f::limits())?;
    assert!(encoded.as_bytes().len() <= 95);
    assert!(
        !encoded
            .as_bytes()
            .windows(20)
            .any(|bytes| bytes == b"flow-compact-history")
    );
    assert!(
        !encoded
            .as_bytes()
            .windows(17)
            .any(|bytes| bytes == b"iot-fanout-worker")
    );
    assert!(
        !encoded
            .as_bytes()
            .windows(23)
            .any(|bytes| bytes == b"device-batch-2026-05-23")
    );
    assert_eq!(encoded.as_bytes(), f::compact_wire());
    let decoded = f::decoded(encoded.as_bytes(), snapshot.context, f::limits())?;
    assert_eq!(
        f::value(&decoded, b"id"),
        Some(b"flow-compact-history".as_slice())
    );
    assert_eq!(
        f::value(&decoded, b"type"),
        Some(b"iot-fanout-worker".as_slice())
    );
    assert_eq!(
        f::value(&decoded, b"correlation_id"),
        Some(b"device-batch-2026-05-23".as_slice())
    );
    assert_eq!(
        f::value(&decoded, b"payload_ref"),
        Some(b"shared-payload-ref".as_slice())
    );
    assert_eq!(f::value(&decoded, b"state"), Some(b"running".as_slice()));
    Ok(())
}

#[test]
fn source_history_codec_roundtrips_multibyte_length_headers() -> TestResult {
    let id = vec![b'f'; 160];
    let kind = vec![b't'; 190];
    let correlation = vec![b'c'; 180];
    let mut pool = b"reason".to_vec();
    pool.extend_from_slice(&[b'r'; 170]);
    let nodes = f::metadata_nodes(pool.len());
    let metadata = f::normalized(&pool, &nodes)?;
    let mut snapshot = f::base(metadata.fields());
    snapshot.event = b"123456789-1";
    snapshot.at = M::Integer(123_456_789);
    snapshot.context.id = Some(&id);
    snapshot.context.workflow_type = Some(&kind);
    snapshot.context.correlation_id = Some(&correlation);
    let encoded = f::healthy(&snapshot, f::limits())?;
    assert_eq!(encoded.as_bytes(), f::long_wire());
    let decoded = f::decoded(encoded.as_bytes(), snapshot.context, f::limits())?;
    let mut expected = f::base_fields();
    expected[0] = (b"event", Some(b"123456789-1"));
    expected[2] = (b"at", Some(b"123456789"));
    expected[3] = (b"id", Some(&id));
    expected[4] = (b"type", Some(&kind));
    expected[17] = (b"correlation_id", Some(&correlation));
    expected.push((b"reason", Some(&pool[6..])));
    assert_eq!(f::pairs(&decoded), expected);
    Ok(())
}
