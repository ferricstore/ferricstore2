use super::{fixtures as f, *};
#[test]
fn rich_values_and_large_original_root_ids_survive_without_field_payload_copies() -> TestResult {
    let mut input = f::CaptureInput::default();
    let name = input.binary(b"tags");
    let a = input.binary(b"alpha");
    let b = input.binary(b"beta");
    let list = input.list(&[a, b]);
    let attrs = input.map(&[(name, list)]);
    let state = input.binary(b"queued");
    let state_meta = input.map(&[(state, attrs)]);
    let root = input.binary(&vec![b'r'; 65483]);
    let yes = input.atom(b"true");
    let no = input.atom(b"false");
    let zero = input.push(Node::FloatBits((-0.0_f64).to_bits()));
    let misc = input.tuple(&[yes, no, zero]);
    let mut pairs = Vec::new();
    for (field, value) in [
        (F::Attributes, attrs),
        (F::StateMeta, state_meta),
        (F::Root, root),
        (F::IndexedAttributes, list),
        (F::Correlation, misc),
    ] {
        let key = input.binary(field.as_bytes());
        pairs.push((key, value));
    }
    input.envelope(&pairs, 1);
    let prepared = prepare(input.input(), None, f::encode_limits())?;
    let captured = decode(prepared.into_stored_bytes(), &f::context(), f::limits())?;
    assert!(
        matches!(captured.field(F::Root),Some(Node::Binary{bytes}) if captured.document().byte_range(bytes.clone()).is_some_and(|v|v.len()==65483))
    );
    assert!(matches!(
        captured.field(F::StateMeta),
        Some(Node::Map { .. })
    ));
    assert!(matches!(
        captured.field(F::IndexedAttributes),
        Some(Node::List { .. })
    ));
    assert!(
        matches!(captured.field(F::Correlation),Some(Node::Tuple{children}) if captured.document().edge_range(children.clone()).is_some_and(|v|v.len()==3))
    );
    Ok(())
}

#[test]
fn every_declared_field_retains_its_actual_data_without_business_normalization() -> TestResult {
    let mut input = f::CaptureInput::default();
    let nil = input.atom(b"nil");
    let empty = input.binary(b"");
    let bytes = input.binary(b"original");
    let map = input.map(&[]);
    let list = input.list(&[]);
    let integer = input.push(Node::Integer(100));
    let start = input.bytes.len();
    input.bytes.extend_from_slice(&[255; 8]);
    let wide = input.push(Node::BigInteger {
        negative: false,
        digits: start..input.bytes.len(),
    });
    let mut pairs = Vec::new();
    let fields = [
        (F::Partition, empty),
        (F::ScopePrefix, nil),
        (F::Parent, bytes),
        (F::Root, bytes),
        (F::Correlation, empty),
        (F::Attributes, map),
        (F::StateMeta, map),
        (F::IndexedAttributes, list),
        (F::IndexedStateMeta, empty),
        (F::MaxActive, wide),
        (F::HistoryMax, integer),
        (F::HistoryHotMax, nil),
    ];
    for (field, value) in fields {
        let key = input.binary(field.as_bytes());
        pairs.push((key, value));
    }
    input.envelope(&pairs, 1);
    let prepared = prepare(input.input(), None, f::encode_limits())?;
    drop(input);
    let captured = decode(prepared.into_stored_bytes(), &f::context(), f::limits())?;
    for (field, _) in fields {
        assert!(captured.field(field).is_some());
    }
    assert!(matches!(captured.field(F::Attributes),Some(Node::Map{entries}) if entries.is_empty()));
    assert!(matches!(
        captured.field(F::IndexedAttributes),
        Some(Node::EmptyList)
    ));
    assert!(matches!(
        captured.field(F::HistoryMax),
        Some(Node::Integer(100))
    ));
    assert!(
        matches!(captured.field(F::MaxActive),Some(Node::BigInteger{negative:false,digits}) if captured.document().byte_range(digits.clone())==Some(&[255;8]))
    );
    Ok(())
}

#[test]
fn foreign_identity_remains_data_and_requires_the_actual_explicit_context() -> TestResult {
    let mut input = f::CaptureInput::default();
    let module = input.atom(b"m");
    let function = input.atom(b"f");
    let foreign = input.push(Node::ForeignExport {
        module,
        function,
        arity: 1,
    });
    let key = input.binary(F::Attributes.as_bytes());
    input.envelope(&[(key, foreign)], 1);
    let bytes = prepare(input.input(), None, f::encode_limits())?.into_stored_bytes();
    let missing = Context {
        safe_atoms: Some(&["m", "f"]),
        active_exports: None,
        local_node: None,
        local_functions: None,
    };
    assert!(matches!(
        decode(bytes.clone(), &missing, f::limits()),
        Err(E::Decoding(
            wf_domain::value::OriginalFlowValueError::UnsupportedContext { .. }
        ))
    ));
    let absent = Context {
        active_exports: Some(&[]),
        ..missing
    };
    assert!(matches!(
        decode(bytes.clone(), &absent, f::limits()),
        Err(E::Decoding(
            wf_domain::value::OriginalFlowValueError::InvalidFlowValue
        ))
    ));
    let present = Context {
        active_exports: Some(&[("m", "f", 1)]),
        ..missing
    };
    let captured = decode(bytes, &present, f::limits())?;
    assert!(matches!(
        captured.field(F::Attributes),
        Some(Node::ForeignExport { arity: 1, .. })
    ));
    Ok(())
}

#[test]
fn local_function_data_uses_the_explicit_initialization_pid_without_execution() -> TestResult {
    let mut input = f::CaptureInput::default();
    let node = input.atom(b"n");
    let module = input.atom(b"m");
    let creator = input.push(Node::ForeignPid {
        node,
        id: 1,
        serial: 0,
        creation: 0,
        local_to_context: true,
    });
    let fun = input.push(Node::ForeignLocalFun {
        module,
        arity: 0,
        index: 0,
        uniq: [0; 16],
        old_index: 0,
        old_uniq: 0,
        creator,
        free: 0..0,
    });
    let key = input.binary(F::Attributes.as_bytes());
    input.envelope(&[(key, fun)], 1);
    assert!(matches!(
        prepare(input.input(), None, f::encode_limits()),
        Err(E::Encoding(
            wf_domain::value::OriginalValueEncodeError::UnsupportedContext { .. }
        ))
    ));
    let prepared = prepare(input.input(), Some(creator), f::encode_limits())?;
    let functions = [wf_domain::value::OriginalValueFunctionIdentity {
        module: "m",
        index: 0,
        arity: 0,
        uniq: [0; 16],
        old_uniq: 0,
    }];
    let context = Context {
        safe_atoms: Some(&["m", "n"]),
        active_exports: Some(&[]),
        local_node: Some(("n", 0)),
        local_functions: Some(&functions),
    };
    let captured = decode(prepared.into_stored_bytes(), &context, f::limits())?;
    assert!(matches!(
        captured.field(F::Attributes),
        Some(Node::ForeignLocalFun {
            arity: 0,
            index: 0,
            ..
        })
    ));
    Ok(())
}
