use super::{C, D, E, F, L, N, decode};

#[test]
fn flow_value_safe_context_and_foreign_data_keep_operational_lanes() {
    let functions = [F {
        module: "m",
        index: 0,
        arity: 0,
        uniq: [0; 16],
        old_uniq: 7,
    }];
    let context = C {
        safe_atoms: Some(&["ok", "n", "m", "nonode@nohost", "erlang", "length"]),
        active_exports: Some(&[("erlang", "length", 1)]),
        local_node: Some(("nonode@nohost", 0)),
        local_functions: Some(&functions),
    };
    let limits = L {
        max_encoded_bytes: 4096,
        max_nodes: 1024,
        max_edges: 2048,
        max_depth: 128,
        max_work_units: 100_000,
    };
    let mut local_fun = vec![131, 112, 0, 0, 0, 0, 0];
    local_fun.extend_from_slice(&[0; 24]);
    local_fun.extend_from_slice(b"w\x01ma\0a\x07Xw\x01n\0\0\0\0\0\0\0\0\0\0\0\0");
    for (body, kind) in [
        (
            b"\x83Xw\x0dnonode@nohost\0\0\0\x5e\0\0\0\0\0\0\0\0".to_vec(),
            0,
        ),
        (b"\x83Yw\x01n\0\0\0\x01\0\0\0\0".to_vec(), 1),
        (
            b"\x83Z\0\x03w\x0dnonode@nohost\0\0\0\0\0\x02\xfc\xbe\xc1\x7c\0\x0c\xa0\x1a\xa8\xec"
                .to_vec(),
            2,
        ),
        (b"\x83qw\x06erlangw\x06lengtha\x01".to_vec(), 3),
        (local_fun, 4),
    ] {
        let mut stored = b"FSV2\x02".to_vec();
        stored.extend_from_slice(&body);
        let actual = decode(stored, &context, limits);
        assert!(actual.is_ok(), "{body:?}: {actual:?}");
        if let Ok(document) = actual {
            assert_foreign(&document, kind);
        }
    }
    for is_export in [false, true] {
        let mut missing = context;
        let mut closed = context;
        let body = if is_export {
            missing.active_exports = None;
            closed.active_exports = Some(&[]);
            b"\x83qw\x06erlangw\x06lengtha\x01".as_slice()
        } else {
            missing.safe_atoms = None;
            closed.safe_atoms = Some(&[]);
            b"\x83w\x02ok".as_slice()
        };
        let mut stored = b"FSV2\x02".to_vec();
        stored.extend_from_slice(body);
        assert!(matches!(
            decode(stored.clone(), &missing, limits),
            Err(E::UnsupportedContext { offset: 6, .. })
        ));
        assert!(matches!(
            decode(stored, &closed, limits),
            Err(E::InvalidFlowValue)
        ));
    }
    for body in [
        b"\x83I\0\x01".as_slice(),
        b"\x83K\0\0\x01",
        b"\x83y\0\0\0\0j",
    ] {
        let mut stored = b"FSV2\x02".to_vec();
        stored.extend_from_slice(body);
        assert!(
            matches!(decode(stored, &context, limits), Err(E::UnsupportedContext { tag, offset: 6 } | E::UnsupportedProfile { tag, offset: 6 }) if tag == body[1])
        );
    }
}

#[test]
fn flow_value_limits_are_distinct_from_invalid_data() {
    let context = C {
        safe_atoms: Some(&[]),
        active_exports: Some(&[]),
        local_node: None,
        local_functions: Some(&[]),
    };
    let limits = L {
        max_encoded_bytes: 4096,
        max_nodes: 1024,
        max_edges: 2048,
        max_depth: 128,
        max_work_units: 100_000,
    };
    let stored = b"FSV2\x02\x83h\x02h\x01a\x07a\x08".to_vec();
    assert!(decode(stored.clone(), &context, limits).is_ok());
    for axis in 0..5 {
        let mut limited = limits;
        *match axis {
            0 => &mut limited.max_encoded_bytes,
            1 => &mut limited.max_nodes,
            2 => &mut limited.max_edges,
            3 => &mut limited.max_depth,
            _ => &mut limited.max_work_units,
        } = 1;
        assert!(matches!(
            decode(stored.clone(), &context, limited),
            Err(E::Limit)
        ));
        *match axis {
            0 => &mut limited.max_encoded_bytes,
            1 => &mut limited.max_nodes,
            2 => &mut limited.max_edges,
            3 => &mut limited.max_depth,
            _ => &mut limited.max_work_units,
        } = 0;
        assert!(matches!(
            decode(stored.clone(), &context, limited),
            Err(E::InvalidLimits)
        ));
    }
    let mut spare = Vec::with_capacity(4096);
    spare.extend_from_slice(&stored);
    let limited = L {
        max_encoded_bytes: stored.len() + 1,
        ..limits
    };
    assert!(spare.len() < limited.max_encoded_bytes);
    assert!(spare.capacity() > limited.max_encoded_bytes);
    assert!(matches!(decode(spare, &context, limited), Err(E::Limit)));
    // Encoded zero-head wrappers consume depth even when the output is scalar.
    let aliases = b"FSV2\x02\x83l\0\0\0\0l\0\0\0\0a\x07".to_vec();
    assert!(matches!(
        decode(
            aliases.clone(),
            &context,
            L {
                max_depth: 2,
                ..limits
            }
        ),
        Err(E::Limit)
    ));
    let actual = decode(
        aliases,
        &context,
        L {
            max_depth: 3,
            ..limits
        },
    );
    assert!(actual.is_ok());
    if let Ok(document) = actual {
        assert!(matches!(document.root(), Some(N::Integer(7))));
    }
    assert!(matches!(
        decode(b"FSV2\x02\x83\xff".to_vec(), &context, limits),
        Err(E::InvalidFlowValue)
    ));
}

#[test]
fn flow_value_document_owns_its_read_buffer() {
    let bytes = b"FSV2\x02\x83h\x02w\x02okm\0\0\0\x02\xff\0";
    let mut incoming = Vec::with_capacity(128);
    incoming.extend_from_slice(bytes);
    let original_pointer = incoming.as_ptr();
    let actual = {
        let atom_name = String::from("ok");
        let atoms = [atom_name.as_str()];
        let exports = [];
        let functions = [];
        let context = C {
            safe_atoms: Some(&atoms),
            active_exports: Some(&exports),
            local_node: None,
            local_functions: Some(&functions),
        };
        let limits = L {
            max_encoded_bytes: 4096,
            max_nodes: 1024,
            max_edges: 2048,
            max_depth: 128,
            max_work_units: 100_000,
        };
        decode(incoming, &context, limits)
    };
    assert!(actual.is_ok(), "{actual:?}");
    if let Ok(document) = actual {
        assert_eq!(document.encoded(), bytes);
        assert!(std::ptr::eq(document.encoded().as_ptr(), original_pointer));
        assert!(matches!(document.root(), Some(N::Tuple { children })
            if document.edge_range(children.clone()).is_some_and(|ids| ids.len() == 2
                && matches!(document.node(ids[0]), Some(N::Atom { bytes, .. }) if document.byte_range(bytes.clone()) == Some(b"ok".as_slice()))
                && matches!(document.node(ids[1]), Some(N::Binary { bytes }) if document.byte_range(bytes.clone()) == Some(b"\xff\0".as_slice())))));
        assert_eq!(document.byte_range(17..19), Some(b"\xff\0".as_slice()));
        assert!(
            document
                .byte_range(17..19)
                .is_some_and(|leaf| std::ptr::eq(leaf.as_ptr(), original_pointer.wrapping_add(17)))
        );
        assert!(document.node(usize::MAX).is_none());
        assert!(document.byte_range(0..usize::MAX).is_none());
        assert!(document.edge_range(0..usize::MAX).is_none());
        let start = document.encoded().len();
        let end = 1;
        assert!(document.byte_range(start..end).is_none());
        assert!(document.edge_range(start..end).is_none());
        assert_eq!(document.byte_range(0..0), Some(b"".as_slice()));
        assert_eq!(document.edge_range(0..0), Some([].as_slice()));
    }
}

fn assert_foreign(document: &D, kind: u8) {
    let valid = match (kind, document.root()) {
        (
            0,
            Some(N::ForeignPid {
                node,
                id: 94,
                serial: 0,
                creation: 0,
                local_to_context: true,
            }),
        ) => matches!(document.node(*node), Some(N::Atom { bytes, .. })
            if document.byte_range(bytes.clone()) == Some(b"nonode@nohost".as_slice())),
        (
            1,
            Some(N::ForeignPort {
                node,
                id: 1,
                creation: 0,
                local_to_context: false,
            }),
        ) => matches!(document.node(*node), Some(N::Atom { bytes, .. })
            if document.byte_range(bytes.clone()) == Some(b"n".as_slice())),
        (
            2,
            Some(N::ForeignReference {
                node,
                creation: 0,
                words,
                local_to_context: true,
            }),
        ) => {
            matches!(document.node(*node), Some(N::Atom { bytes, .. })
            if document.byte_range(bytes.clone()) == Some(b"nonode@nohost".as_slice()))
                && document.byte_range(words.clone())
                    == Some(b"\0\x02\xfc\xbe\xc1\x7c\0\x0c\xa0\x1a\xa8\xec".as_slice())
        }
        (
            3,
            Some(N::ForeignExport {
                module,
                function,
                arity: 1,
            }),
        ) => {
            matches!(document.node(*module), Some(N::Atom { bytes, .. })
                if document.byte_range(bytes.clone()) == Some(b"erlang".as_slice()))
                && matches!(document.node(*function), Some(N::Atom { bytes, .. })
                    if document.byte_range(bytes.clone()) == Some(b"length".as_slice()))
        }
        (
            4,
            Some(N::ForeignLocalFun {
                module,
                arity: 0,
                index: 0,
                uniq,
                old_index: 0,
                old_uniq: 7,
                creator,
                free,
            }),
        ) => {
            *uniq == [0; 16]
                && document.edge_range(free.clone()) == Some([].as_slice())
                && matches!(document.node(*module), Some(N::Atom { bytes, .. })
                    if document.byte_range(bytes.clone()) == Some(b"m".as_slice()))
                && matches!(
                    document.node(*creator),
                    Some(N::ForeignPid {
                        node,
                        id: 0,
                        serial: 0,
                        creation: 0,
                        local_to_context: false,
                    }) if matches!(document.node(*node), Some(N::Atom { bytes, .. })
                        if document.byte_range(bytes.clone()) == Some(b"n".as_slice()))
                )
        }
        _ => false,
    };
    assert!(valid, "kind {kind}: {document:?}");
}
