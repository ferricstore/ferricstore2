use super::{fixtures as f, *};
#[test]
fn partition_and_scope_preserve_missing_nil_and_empty_binary() -> TestResult {
    for field in [F::Partition, F::ScopePrefix] {
        for shape in 0..3 {
            let mut input = f::CaptureInput::default();
            let key = input.binary(field.as_bytes());
            let nil = input.atom(b"nil");
            let empty = input.binary(b"");
            let pairs = match shape {
                0 => vec![],
                1 => vec![(key, nil)],
                _ => vec![(key, empty)],
            };
            input.envelope(&pairs, 1);
            let prepared = prepare(input.input(), None, f::encode_limits())?;
            let captured = decode(prepared.into_stored_bytes(), &f::context(), f::limits())?;
            match shape {
                0 => assert!(captured.field(field).is_none()),
                1 => assert!(
                    matches!(captured.field(field),Some(Node::Atom{bytes,..}) if captured.document().byte_range(bytes.clone())==Some(b"nil"))
                ),
                _ => assert!(
                    matches!(captured.field(field),Some(Node::Binary{bytes}) if captured.document().byte_range(bytes.clone())==Some(b""))
                ),
            }
        }
    }
    Ok(())
}
#[test]
fn empty_capture_moves_one_allocation_through_decode_and_back() -> TestResult {
    let input = f::empty();
    let prepared = prepare(input.input(), None, f::encode_limits())?;
    let wire = prepared.into_stored_bytes();
    let ptr = wire.as_ptr();
    let expected = wire.clone();
    let captured = decode(wire, &f::context(), f::limits())?;
    assert_eq!(captured.document().encoded().as_ptr(), ptr);
    let returned = captured.into_stored_bytes();
    assert_eq!(returned.as_ptr(), ptr);
    assert_eq!(returned, expected);
    Ok(())
}

#[test]
fn decoder_requires_the_explicit_capture_schema_not_just_a_valid_value() -> TestResult {
    assert!(matches!(
        decode(f::wire(2, &[])?, &f::context(), f::limits()),
        Err(E::UnsupportedVersion)
    ));
    assert!(matches!(
        decode(f::wire(1, &[(b"unknown", 1)])?, &f::context(), f::limits()),
        Err(E::InvalidSchema)
    ));
    let mut input = f::CaptureInput::default();
    let name = input.binary(b"unknown");
    let value = input.push(Node::Integer(1));
    input.envelope(&[(name, value)], 1);
    assert!(matches!(
        prepare(input.input(), None, f::encode_limits()),
        Err(E::InvalidSchema)
    ));
    let mut input = f::CaptureInput::default();
    let key = input.binary(F::Partition.as_bytes());
    input.envelope(&[(key, usize::MAX)], 1);
    assert!(matches!(
        prepare(input.input(), None, f::encode_limits()),
        Err(E::InvalidSchema)
    ));
    Ok(())
}

#[test]
fn logical_cycles_cannot_be_branded_as_complete_metadata() -> TestResult {
    let mut input = f::CaptureInput::default();
    let key = input.binary(F::Attributes.as_bytes());
    let value = input.binary(b"v");
    input.envelope(&[(key, value)], 1);
    prepare(input.input(), None, f::encode_limits())?;
    let at = input.edges.len();
    input.edges.push(value);
    input.nodes[value] = Node::Tuple {
        children: at..at + 1,
    };
    assert!(matches!(
        prepare(input.input(), None, f::encode_limits()),
        Err(E::Encoding(
            wf_domain::value::OriginalValueEncodeError::InvalidInput
        ))
    ));
    Ok(())
}

#[test]
fn repeated_fields_refuse_without_an_ambiguous_capture() -> TestResult {
    let mut input = f::CaptureInput::default();
    let key = input.binary(F::Partition.as_bytes());
    let value = input.binary(b"p");
    input.envelope(&[(key, value), (key, value)], 1);
    assert!(matches!(
        prepare(input.input(), None, f::encode_limits()),
        Err(E::InvalidSchema)
    ));
    assert!(matches!(
        decode(
            f::wire(1, &[(b"partition_key", 1), (b"partition_key", 2)])?,
            &f::context(),
            f::limits()
        ),
        Err(E::Decoding(
            wf_domain::value::OriginalFlowValueError::InvalidFlowValue
        ))
    ));
    Ok(())
}

#[test]
fn wrong_root_tag_or_dangling_graph_has_no_metadata_owner() -> TestResult {
    let mut input = f::empty();
    input.root = usize::MAX;
    assert!(matches!(
        prepare(input.input(), None, f::encode_limits()),
        Err(E::InvalidSchema)
    ));
    let mut input = f::CaptureInput::default();
    input.root = input.binary(b"wf.record.metadata");
    assert!(matches!(
        prepare(input.input(), None, f::encode_limits()),
        Err(E::InvalidSchema)
    ));
    let mut bytes = f::wire(1, &[])?;
    let at = bytes
        .windows(b"wf.record.metadata".len())
        .position(|v| v == b"wf.record.metadata")
        .ok_or("tag")?;
    bytes[at] = b'x';
    assert!(matches!(
        decode(bytes, &f::context(), f::limits()),
        Err(E::InvalidSchema)
    ));
    Ok(())
}

#[test]
fn stored_capture_does_not_infer_bare_or_escaped_value_profiles() -> TestResult {
    let wire = f::wire(1, &[])?;
    assert!(matches!(
        decode(wire[5..].to_vec(), &f::context(), f::limits()),
        Err(E::InvalidSchema)
    ));
    let mut escaped = b"FSV2\x01".to_vec();
    escaped.extend_from_slice(&wire);
    assert!(matches!(
        decode(escaped, &f::context(), f::limits()),
        Err(E::InvalidSchema)
    ));
    let mut trailing = wire.clone();
    trailing.push(0);
    assert!(matches!(
        decode(trailing, &f::context(), f::limits()),
        Err(E::Decoding(_))
    ));
    for length in 0..wire.len() {
        assert!(decode(wire[..length].to_vec(), &f::context(), f::limits()).is_err());
    }
    Ok(())
}

#[test]
fn debug_keeps_record_fields_and_scope_bytes_redacted() -> TestResult {
    let mut input = f::CaptureInput::default();
    let key = input.binary(F::ScopePrefix.as_bytes());
    let secret = input.binary(b"secret-scope");
    input.envelope(&[(key, secret)], 1);
    let prepared = prepare(input.input(), None, f::encode_limits())?;
    assert!(!format!("{prepared:?}").contains("secret-scope"));
    let record = decode(prepared.into_stored_bytes(), &f::context(), f::limits())?;
    assert!(!format!("{record:?}").contains("secret-scope"));
    Ok(())
}
#[test]
fn unsupported_version_is_distinct_from_a_malformed_envelope() {
    let mut input = f::CaptureInput::default();
    input.envelope(&[], 2);
    assert!(matches!(
        prepare(input.input(), None, f::encode_limits()),
        Err(E::UnsupportedVersion)
    ));
}
