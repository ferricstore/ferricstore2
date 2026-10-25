use super::{E, K, P, R, named, parse, render};

#[test]
fn original_value_refs_parse_all_five_families_and_borrow_original_spans() {
    for (input, expected_kind, expected_id, expected_version, tag_len) in [
        (
            b"f:{f}:v:p:id:0".as_slice(),
            K::Payload,
            b"id".as_slice(),
            0_u64,
            5,
        ),
        (b"f:{f}:v:r:id:1", K::Result, b"id", 1, 5),
        (
            b"f:{f}:v:e:id:9007199254740991",
            K::Error,
            b"id",
            9_007_199_254_740_991,
            5,
        ),
        (b"f:{f}:v:s:a:b\0\xff:0", K::Shared, b"a:b\0\xff", 0, 5),
        (b"f:{f}:v:p:a}:v:p:b:0", K::Payload, b"a}:v:p:b", 0, 5),
        (
            b"f:{f}:v:r:id:9007199254740991",
            K::Result,
            b"id",
            9_007_199_254_740_991,
            5,
        ),
        (b"f:{fa:255}:v:e:a:b\0\xff:1", K::Error, b"a:b\0\xff", 1, 10),
        (
            b"f:{f:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA}:v:s:id:1",
            K::Shared,
            b"id",
            1,
            49,
        ),
    ] {
        let backing = input.to_vec();
        let actual = {
            let descriptor = backing.as_slice();
            parse(descriptor)
        };
        assert!(matches!(actual, Some(R::Regular { kind, id, version, .. })
            if kind == expected_kind && id == expected_id && version == expected_version));
        if let Some(R::Regular { tag_prefix, id, .. }) = actual {
            assert_eq!(tag_prefix, &backing[..tag_len]);
            assert!(std::ptr::eq(tag_prefix, &raw const backing[..tag_len]));
            let id_start = tag_len + 5;
            assert!(std::ptr::eq(
                id,
                &raw const backing[id_start..id_start + expected_id.len()]
            ));
        }
    }
    for (input, owner, name, version) in [
        (
            b"f:{f}:v:n:YTr_:ADo:0".as_slice(),
            b"YTr_".as_slice(),
            b"ADo".as_slice(),
            0_u64,
        ),
        (b"f:{f}:v:n:AA:AAA:0", b"AA", b"AAA", 0),
    ] {
        let backing = input.to_vec();
        let actual = {
            let descriptor = backing.as_slice();
            parse(descriptor)
        };
        assert!(
            matches!(actual, Some(R::Named { owner_component, name_component, version: v, .. })
            if owner_component == owner && name_component == name && v == version)
        );
        if let Some(R::Named {
            tag_prefix,
            owner_component,
            name_component,
            ..
        }) = actual
        {
            assert_eq!(tag_prefix, b"f:{f}");
            assert!(std::ptr::eq(tag_prefix, &raw const backing[..5]));
            assert!(std::ptr::eq(
                owner_component,
                &raw const backing[10..10 + owner.len()]
            ));
            let start = 11 + owner.len();
            assert!(std::ptr::eq(
                name_component,
                &raw const backing[start..start + name.len()]
            ));
        }
    }
}

#[test]
fn original_value_refs_reject_noncanonical_tags_versions_and_named_components() {
    // These are the15 negative predicates in the actual original packet.
    for input in [
        b"f:{f}:v:p:id:-1".as_slice(),
        b"f:{f}:v:p:id:+1",
        b"f:{f}:v:p:id:01",
        b"f:{f}:v:p:id:9007199254740992",
        b"f:{f}:v:p::0",
        b"f:{f}:v:n::YQ:0",
        b"f:{f}:v:n:YQ::0",
        b"f:{f}:v:n:YQ:Yg:0:1",
        b"f:{f}:v:n:A:YQ:0",
        b"f:{f}:v:n:AB:YQ:0",
        b"f:{f}:v:n:YQ==:Yg:0",
        b"f:{fa:00}:v:p:id:0",
        b"f:{fa:256}:v:p:id:0",
        b"f:{f:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAB}:v:p:id:0",
        b"f:{f:///////////////////////////////////////////}:v:p:id:0",
    ] {
        assert!(parse(input).is_none(), "{input:?}");
    }
    for input in [
        b"".as_slice(),
        b"g:{f}:v:p:id:0",
        b"f:{f}:v:x:id:0",
        b"f:{fa:-1}:v:p:id:0",
        b"f:{fa:}:v:p:id:0",
        b"f:{f}:v:p:id:",
        b"f:{f}:v:n:YQ:Yg:00",
        b"f:{f}:v:n:YQ:Yg:9007199254740992",
        b"f:{f}:v:n:YQ+:Yg:0",
        b"f:{f}:v:n:YQ:Yh:0",
    ] {
        assert!(parse(input).is_none(), "{input:?}");
    }
}

#[test]
fn original_value_ref_rendering_matches_global_automatic_and_hashed_partitions() {
    for (kind, id, version, expected, size) in [
        (
            K::Payload,
            b"id".as_slice(),
            0_u64,
            b"f:{f}:v:p:id:0".as_slice(),
            14,
        ),
        (K::Result, b"id", 1, b"f:{f}:v:r:id:1", 14),
        (
            K::Error,
            b"id",
            9_007_199_254_740_991,
            b"f:{f}:v:e:id:9007199254740991",
            29,
        ),
        (K::Shared, b"a:b\0\xff", 0, b"f:{f}:v:s:a:b\0\xff:0", 17),
    ] {
        assert_eq!(expected.len(), size);
        let actual = render(id, kind, version, P::Global, size);
        assert!(actual.is_ok(), "{actual:?}");
        if let Ok(bytes) = actual {
            assert_eq!(bytes, expected);
        }
    }
    // Both omitted and explicit nil are captured by Automatic, never Global.
    for partition in [P::Automatic, P::Automatic] {
        let actual = render(b"123456789", K::Payload, 0, partition, 25);
        assert!(actual.is_ok(), "{actual:?}");
        if let Ok(bytes) = actual {
            assert_eq!(bytes, b"f:{fa:38}:v:p:123456789:0");
        }
        assert!(matches!(
            render(b"123456789", K::Payload, 0, partition, 24),
            Err(E::Limit)
        ));
    }
    for (partition, expected, size) in [
        (
            b"".as_slice(),
            b"f:{f:47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU}:v:p:id:1".as_slice(),
            58,
        ),
        (
            b"tenant-a",
            b"f:{f:gKcHr33HfuEij5EnGA85ZINeW-tMSrDYEvD-dZNXmzo}:v:p:id:1",
            58,
        ),
        (b"__flow_auto__:0", b"f:{fa:0}:v:p:id:1", 17),
        (b"__flow_auto__:255", b"f:{fa:255}:v:p:id:1", 19),
        (
            b"__flow_auto__:00",
            b"f:{f:gsr2hFtEIp_kCPcVOkiEkDb9oEsnlO4Thrk7rF2xduM}:v:p:id:1",
            58,
        ),
        (
            b"__flow_auto__:256",
            b"f:{f:uthyxt9AjShMQ2d8kNIlnuWWxNfR2G33RLrnw9iyB_c}:v:p:id:1",
            58,
        ),
        (
            b"\0\xff:",
            b"f:{f:Nqdq8Bq-H0FEpg9nI1JHsUjcy4R42mQyvvlTlNuHI-g}:v:p:id:1",
            58,
        ),
    ] {
        assert_eq!(expected.len(), size);
        let actual = render(b"id", K::Payload, 1, P::Explicit(partition), size);
        assert!(actual.is_ok(), "{partition:?}: {actual:?}");
        if let Ok(bytes) = actual {
            assert_eq!(bytes, expected);
        }
        assert!(matches!(
            render(b"id", K::Payload, 1, P::Explicit(partition), size - 1),
            Err(E::Limit)
        ));
    }
    for (id, version) in [(b"".as_slice(), 0), (b"id", 9_007_199_254_740_992)] {
        assert!(matches!(
            render(id, K::Payload, version, P::Global, 65_535),
            Err(E::InvalidInput)
        ));
        assert!(matches!(
            render(id, K::Payload, version, P::Global, 0),
            Err(E::InvalidInput)
        ));
    }
}

#[test]
fn original_named_refs_encode_opaque_owner_and_name_once() {
    let owner = b"a:\xff".to_vec();
    let name = b"\0:".to_vec();
    for (partition, version, expected, size) in [
        (P::Global, 0_u64, b"f:{f}:v:n:YTr_:ADo:0".as_slice(), 20),
        (
            P::Automatic,
            9_007_199_254_740_991,
            b"f:{fa:241}:v:n:YTr_:ADo:9007199254740991",
            40,
        ),
    ] {
        let actual = {
            let owner_descriptor = owner.as_slice();
            let name_descriptor = name.as_slice();
            named(owner_descriptor, name_descriptor, version, partition, size)
        };
        assert_eq!(expected.len(), size);
        assert!(actual.is_ok(), "{actual:?}");
        if let Ok(bytes) = actual {
            assert_eq!(bytes, expected);
        }
        assert!(matches!(
            named(&owner, &name, version, partition, size - 1),
            Err(E::Limit)
        ));
    }
    for (owner, name, version) in [
        (b"".as_slice(), b"name".as_slice(), 0_u64),
        (b"owner", b"", 0),
        (b"owner", b"name", 9_007_199_254_740_992),
    ] {
        assert!(matches!(
            named(owner, name, version, P::Global, 65_535),
            Err(E::InvalidInput)
        ));
        assert!(matches!(
            named(owner, name, version, P::Global, 0),
            Err(E::InvalidInput)
        ));
    }
    assert_eq!(owner, b"a:\xff");
    assert_eq!(name, b"\0:");
}

#[test]
fn original_value_ref_key_limit_does_not_change_shape_recognition() {
    for size in [65_535, 65_536] {
        let id = vec![b'x'; size - 12];
        let mut reference = b"f:{f}:v:p:".to_vec();
        reference.extend_from_slice(&id);
        reference.extend_from_slice(b":0");
        assert_eq!(reference.len(), size);
        let parsed = parse(&reference);
        assert!(
            matches!(parsed, Some(R::Regular { kind: K::Payload, id: value, version: 0, .. })
            if value == id.as_slice())
        );
        let actual = render(&id, K::Payload, 0, P::Global, usize::MAX);
        if size == 65_535 {
            assert!(actual.is_ok(), "{actual:?}");
            if let Ok(bytes) = actual {
                assert_eq!(bytes, reference);
            }
            assert!(matches!(
                render(&id, K::Payload, 0, P::Global, 65_534),
                Err(E::Limit)
            ));
        } else {
            assert!(matches!(actual, Err(E::Limit)));
        }
        assert_eq!(id.len(), size - 12);
    }
    assert!(matches!(
        render(b"id", K::Payload, 0, P::Global, 0),
        Err(E::Limit)
    ));
    assert!(matches!(named(b"a", b"b", 0, P::Global, 0), Err(E::Limit)));
}
