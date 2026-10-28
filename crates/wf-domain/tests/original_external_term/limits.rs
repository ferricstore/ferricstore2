use super::{DError, E, I, L, N, TestResult, decode, fixtures as f, prepare};

#[test]
fn external_term_encoding_enforces_exact_resource_boundaries() -> TestResult {
    let first = f::current()?;
    let length = first.as_external_term().len();
    let work = first.work_units();
    let temporary = first.temporary_bytes();
    let nodes = f::current_nodes();
    let input = I {
        bytes: b"currentvaluepayload",
        nodes: &nodes,
        edges: &[1, 2, 0, 3],
        root: 4,
    };
    let limits = f::limits();
    assert!(
        prepare(
            input,
            L {
                max_encoded_bytes: length,
                max_work_units: work,
                max_temporary_bytes: temporary,
                ..limits
            }
        )
        .is_ok()
    );
    for profile in [
        L {
            max_encoded_bytes: length - 1,
            ..limits
        },
        L {
            max_work_units: work - 1,
            ..limits
        },
        L {
            max_temporary_bytes: temporary - 1,
            ..limits
        },
        L {
            max_nodes: 1,
            ..limits
        },
        L {
            max_edges: 1,
            ..limits
        },
        L {
            max_depth: 1,
            ..limits
        },
    ] {
        assert!(matches!(prepare(input, profile), Err(E::Limit)));
    }
    assert!(matches!(
        prepare(
            input,
            L {
                max_input_bytes: 0,
                ..limits
            }
        ),
        Err(E::InvalidLimits)
    ));
    let mut cyclic = f::current_nodes();
    cyclic[4] = N::Tuple { children: 0..1 };
    assert!(matches!(
        prepare(
            I {
                nodes: &cyclic,
                edges: &[4],
                ..input
            },
            limits
        ),
        Err(E::InvalidInput)
    ));
    Ok(())
}

#[test]
fn external_term_decoding_keeps_input_owner_and_bounds() -> TestResult {
    let prepared = f::current()?;
    let mut bytes = prepared.into_external_term();
    bytes.shrink_to_fit();
    let pointer = bytes.as_ptr();
    let length = bytes.len();
    let document = f::decoded(bytes, &f::context(), f::decode_limits())?;
    assert_eq!(document.encoded().as_ptr(), pointer);
    assert_eq!(document.encoded().len(), length);
    let profile = f::decode_limits();
    for limited in [
        super::DL {
            max_encoded_bytes: length - 1,
            ..profile
        },
        super::DL {
            max_nodes: 1,
            ..profile
        },
        super::DL {
            max_edges: 1,
            ..profile
        },
        super::DL {
            max_depth: 1,
            ..profile
        },
        super::DL {
            max_work_units: 1,
            ..profile
        },
    ] {
        assert!(matches!(
            decode(document.encoded().to_vec(), &f::context(), limited),
            Err(DError::Limit)
        ));
    }
    let mut excess = Vec::with_capacity(profile.max_encoded_bytes + 1);
    excess.extend_from_slice(document.encoded());
    assert!(matches!(
        decode(excess, &f::context(), profile),
        Err(DError::Limit)
    ));
    assert!(matches!(
        decode(
            document.encoded().to_vec(),
            &f::context(),
            super::DL {
                max_work_units: 0,
                ..profile
            }
        ),
        Err(DError::InvalidLimits)
    ));
    let mut low = 1;
    let mut high = profile.max_work_units;
    while low < high {
        let middle = low + (high - low) / 2;
        match decode(
            document.encoded().to_vec(),
            &f::context(),
            super::DL {
                max_work_units: middle,
                ..profile
            },
        ) {
            Ok(_) => high = middle,
            Err(DError::Limit) => low = middle + 1,
            Err(error) => return Err(format!("decode work boundary: {error:?}").into()),
        }
    }
    assert!(
        decode(
            document.encoded().to_vec(),
            &f::context(),
            super::DL {
                max_work_units: low,
                ..profile
            }
        )
        .is_ok()
    );
    assert!(matches!(
        decode(
            document.encoded().to_vec(),
            &f::context(),
            super::DL {
                max_work_units: low - 1,
                ..profile
            }
        ),
        Err(DError::Limit)
    ));
    Ok(())
}
