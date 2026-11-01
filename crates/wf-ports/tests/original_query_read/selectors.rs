use super::{fixtures as f, *};

#[test]
fn query_decoding_materializes_only_required_public_metadata_sections() -> TestResult {
    let key = f::key()?;
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let lean = query(
        encoded.encoded(),
        &key,
        0,
        Plan {
            attributes: Selection::None,
            state_meta: StateSelection::None,
        },
        limits(),
    );
    assert!(lean.is_ok());
    let lean = row(lean.map_err(|e| format!("{e:?}"))?)?;
    let lean = lean.record().ok_or("record")?;
    assert!(lean.attributes().is_none());
    assert!(lean.state_meta().is_none());
    assert!(lean.indexed_attributes().is_none());
    assert!(lean.indexed_state_meta().is_none());
    assert!(lean.field("state_enter_seq").is_none());
    let attrs = query(
        encoded.encoded(),
        &key,
        0,
        Plan {
            attributes: Selection::All,
            state_meta: StateSelection::None,
        },
        limits(),
    );
    assert!(attrs.is_ok());
    let attrs = row(attrs.map_err(|e| format!("{e:?}"))?)?;
    let attrs = attrs.record().ok_or("record")?;
    assert!(attributes_eq(attrs, expected_field("attributes")));
    assert!(attrs.state_meta().is_none());
    let complete = query(
        encoded.encoded(),
        &key,
        0,
        Plan {
            attributes: Selection::All,
            state_meta: StateSelection::All,
        },
        limits(),
    );
    assert!(complete.is_ok());
    let complete = row(complete.map_err(|e| format!("{e:?}"))?)?;
    let complete = complete.record().ok_or("record")?;
    assert!(attributes_eq(complete, expected_field("attributes")));
    assert!(states_eq(complete, expected_field("state_meta")));
    Ok(())
}

#[test]
fn query_decoding_materializes_only_the_selected_dynamic_fields() -> TestResult {
    let key = f::key()?;
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    );
    assert!(encoded.is_ok());
    let encoded = encoded.map_err(|e| format!("{e:?}"))?;
    let attrs = [b"tier".as_slice(), b"missing".as_slice()];
    let worker = [b"worker".as_slice()];
    let field = [b"field".as_slice()];
    let states = [
        (b"queued".as_slice(), worker.as_slice()),
        (b"missing".as_slice(), field.as_slice()),
    ];
    let value = query(
        encoded.encoded(),
        &key,
        0,
        Plan {
            attributes: Selection::Names(&attrs),
            state_meta: StateSelection::Names(&states),
        },
        limits(),
    );
    assert!(value.is_ok());
    let value = row(value.map_err(|e| format!("{e:?}"))?)?;
    let selected = value.record().ok_or("record")?;
    assert!(selected.attributes().is_none());
    assert!(selected.state_meta().is_none());
    assert!(
        selected
            .selected_attribute(b"tier")
            .is_some_and(|v| value_eq(I::Binary(b"gold"), v))
    );
    assert!(
        selected
            .selected_state_field(b"queued", b"worker")
            .is_some_and(|v| value_eq(I::Binary(b"worker-1"), v))
    );
    assert!(
        selected
            .attribute(b"tier")
            .is_some_and(|v| value_eq(I::Binary(b"gold"), v))
    );
    assert!(selected.attribute(b"missing").is_none());
    assert!(
        selected
            .state_field(b"queued", b"worker")
            .is_some_and(|v| value_eq(I::Binary(b"worker-1"), v))
    );
    assert!(selected.state_field(b"missing", b"field").is_none());
    Ok(())
}

#[test]
fn selection_source_bounds_allow_512_byte_names_and_78_unique_fields() -> TestResult {
    let key = f::key()?;
    let encoded = encode(
        I::Binary(&key),
        f::record(),
        f::locator(),
        I::Integer(0),
        f::limits(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let long = vec![b'x'; 512];
    let names = [long.as_slice()];
    assert!(
        query(
            encoded.encoded(),
            &key,
            0,
            Plan {
                attributes: Selection::Names(&names),
                state_meta: StateSelection::None
            },
            limits()
        )
        .is_ok()
    );
    let oversized = vec![b'x'; 513];
    let names = [oversized.as_slice()];
    assert!(matches!(
        query(
            encoded.encoded(),
            &key,
            0,
            Plan {
                attributes: Selection::Names(&names),
                state_meta: StateSelection::None
            },
            limits()
        ),
        Err(E::InvalidRecord)
    ));
    let names: Vec<_> = (0..79).map(|n| format!("field-{n}").into_bytes()).collect();
    let refs: Vec<_> = names.iter().map(Vec::as_slice).collect();
    assert!(
        query(
            encoded.encoded(),
            &key,
            0,
            Plan {
                attributes: Selection::Names(&refs[..78]),
                state_meta: StateSelection::None
            },
            limits()
        )
        .is_ok()
    );
    assert!(matches!(
        query(
            encoded.encoded(),
            &key,
            0,
            Plan {
                attributes: Selection::Names(&refs),
                state_meta: StateSelection::None
            },
            limits()
        ),
        Err(E::InvalidRecord)
    ));
    assert!(matches!(
        query(
            encoded.encoded(),
            &key,
            0,
            Plan {
                attributes: Selection::Names(&[]),
                state_meta: StateSelection::None
            },
            limits()
        ),
        Err(E::InvalidRecord)
    ));
    let malformed = [b" padded".as_slice()];
    assert!(matches!(
        query(
            encoded.encoded(),
            &key,
            0,
            Plan {
                attributes: Selection::Names(&malformed),
                state_meta: StateSelection::None
            },
            limits()
        ),
        Err(E::InvalidRecord)
    ));
    Ok(())
}
