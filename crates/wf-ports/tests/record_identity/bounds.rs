use super::*;
use wf_domain::metadata::scope::OriginalScopeError as E;
use wf_ports::blob::parse_original_state_key as parse;

#[test]
fn canonical_hash_crc_and_global_tags_keep_original_nil_routing() -> Result<(), String> {
    let explicit =
        key(b"plain", P::Explicit(b"tenant-a"), 65_535).map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        explicit,
        b"f:{f:gKcHr33HfuEij5EnGA85ZINeW-tMSrDYEvD-dZNXmzo}:s:plain"
    );
    let automatic = key(b"plain", P::Automatic, 65_535).map_err(|error| format!("{error:?}"))?;
    assert_eq!(automatic, b"f:{fa:207}:s:plain");
    assert_eq!(
        key(b"plain", P::Explicit(b"__flow_auto__:207"), 65_535)
            .map_err(|error| format!("{error:?}"))?,
        automatic
    );
    assert!(
        owns(
            Some(I::Binary(b"plain")),
            Some(I::Nil),
            None,
            I::Binary(&automatic),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    let global = key(b"plain", P::Global, 65_535).map_err(|error| format!("{error:?}"))?;
    assert_eq!(global, b"f:{f}:s:plain");
    assert!(
        !owns(
            Some(I::Binary(b"plain")),
            Some(I::Nil),
            None,
            I::Binary(&global),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    assert!(
        !owns(
            Some(I::Binary(b"plain")),
            None,
            None,
            I::Binary(&automatic),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    Ok(())
}

#[test]
fn parser_accepts_marker_bytes_in_ids_and_refuses_noncanonical_tags() -> Result<(), String> {
    let id = b"x}:s:y\0\xff";
    for partition in [P::Automatic, P::Global, P::Explicit(b"partition")] {
        let encoded = key(id, partition, 65_535).map_err(|error| format!("{error:?}"))?;
        assert_eq!(parse(&encoded), Some(id.as_slice()));
    }
    for invalid in [
        b"f:{fa:00}:s:id".as_slice(),
        b"f:{fa:256}:s:id",
        b"f:{fa:-1}:s:id",
        b"f:{}:s:id",
        b"f:{f}:v:id",
        b"f:{f}:s:",
        b"f:{f:bad}:s:id",
    ] {
        assert!(parse(invalid).is_none());
    }
    let invalid = b"f:{f:gKcHr33HfuEij5EnGA85ZINeW-tMSrDYEvD-dZNXmzp}:s:plain";
    assert!(parse(invalid).is_none());
    Ok(())
}

#[test]
fn ownership_checks_source_key_and_partition_ceiling_without_full_key_copy() -> Result<(), String> {
    let id = vec![b'x'; 65_483];
    let encoded = key(&id, P::Explicit(b"p"), 65_535).map_err(|error| format!("{error:?}"))?;
    assert_eq!(encoded.len(), 65_535);
    assert!(
        owns(
            Some(I::Binary(&id)),
            Some(I::Binary(b"p")),
            None,
            I::Binary(&encoded),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    let mut too_large = encoded.clone();
    too_large.push(b'x');
    assert!(
        !owns(
            Some(I::Binary(&id)),
            Some(I::Binary(b"p")),
            None,
            I::Binary(&too_large),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    let partition = vec![b'p'; 65_536];
    let encoded =
        key(b"id", P::Explicit(&partition), 65_535).map_err(|error| format!("{error:?}"))?;
    assert!(
        !owns(
            Some(I::Binary(b"id")),
            Some(I::Binary(&partition)),
            None,
            I::Binary(&encoded),
            limits()
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    Ok(())
}

#[test]
fn operational_refusal_never_becomes_a_false_ownership_observation() -> Result<(), String> {
    let encoded =
        key(b"plain", P::Explicit(b"tenant-a"), 65_535).map_err(|error| format!("{error:?}"))?;
    let measured = owns(
        Some(I::Binary(b"plain")),
        Some(I::Binary(b"tenant-a")),
        None,
        I::Binary(&encoded),
        limits(),
    )
    .map_err(|error| format!("{error:?}"))?;
    let exact = OriginalScopeLimits {
        maximum_work_units: measured.work_units,
        maximum_temporary_bytes: measured.temporary_bytes,
        ..limits()
    };
    assert!(
        owns(
            Some(I::Binary(b"plain")),
            Some(I::Binary(b"tenant-a")),
            None,
            I::Binary(&encoded),
            exact
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    for selected in [
        OriginalScopeLimits {
            maximum_work_units: exact.maximum_work_units - 1,
            ..exact
        },
        OriginalScopeLimits {
            maximum_temporary_bytes: exact.maximum_temporary_bytes - 1,
            ..exact
        },
    ] {
        assert!(matches!(
            owns(
                Some(I::Binary(b"plain")),
                Some(I::Binary(b"tenant-a")),
                None,
                I::Binary(&encoded),
                selected
            ),
            Err(E::Limit)
        ));
    }
    assert!(matches!(
        owns(
            None,
            None,
            None,
            I::Nil,
            OriginalScopeLimits {
                maximum_work_units: 0,
                ..exact
            }
        ),
        Err(E::InvalidLimits)
    ));
    let rejected = owns(
        Some(I::Binary(b"plain")),
        Some(I::Map(&[])),
        None,
        I::Binary(&encoded),
        limits(),
    )
    .map_err(|error| format!("{error:?}"))?;
    assert!(!rejected.value);
    let selected = OriginalScopeLimits {
        maximum_work_units: rejected.work_units,
        maximum_temporary_bytes: rejected.temporary_bytes,
        ..limits()
    };
    assert!(
        !owns(
            Some(I::Binary(b"plain")),
            Some(I::Map(&[])),
            None,
            I::Binary(&encoded),
            selected
        )
        .map_err(|error| format!("{error:?}"))?
        .value
    );
    assert!(matches!(
        owns(
            Some(I::Binary(b"plain")),
            Some(I::Map(&[])),
            None,
            I::Binary(&encoded),
            OriginalScopeLimits {
                maximum_work_units: selected.maximum_work_units - 1,
                ..selected
            }
        ),
        Err(E::Limit)
    ));
    Ok(())
}
