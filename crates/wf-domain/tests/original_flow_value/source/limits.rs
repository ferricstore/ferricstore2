use wf_domain::value::decode_original_flow_value_compat as compat;

use super::super::{C, L, decode};
use super::{E, N, TestResult, fixtures as f};

#[test]
fn source_value_compat_keeps_operational_limits_and_owned_fallback() -> TestResult {
    let typed = f::encoded_map(b"kind", b"typed", false)?;
    let (document, size) = f::decoded(typed.clone())?;
    assert_eq!(
        f::map_fields(&document)?,
        (b"kind".as_slice(), b"typed".as_slice())
    );
    assert_eq!(size, typed.len() - 5);
    resource_limits(&typed);
    context_lanes(&typed)?;
    owned_fallback(&typed)?;
    continuous_work()?;
    wrapper_boundaries()?;
    owned_after_context_scope()?;
    Ok(())
}

fn resource_limits(typed: &[u8]) {
    let limits = f::limits();
    for cut in [
        L {
            max_encoded_bytes: 1,
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
        L {
            max_work_units: 1,
            ..limits
        },
    ] {
        assert!(matches!(
            compat(typed.to_vec(), &f::context(), cut),
            Err(E::Limit)
        ));
    }
    for invalid in [
        L {
            max_encoded_bytes: 0,
            ..limits
        },
        L {
            max_nodes: 0,
            ..limits
        },
        L {
            max_edges: 0,
            ..limits
        },
        L {
            max_depth: 0,
            ..limits
        },
        L {
            max_work_units: 0,
            ..limits
        },
    ] {
        assert!(matches!(
            compat(typed.to_vec(), &f::context(), invalid),
            Err(E::InvalidLimits)
        ));
    }
    let mut oversized_owner = Vec::with_capacity(limits.max_encoded_bytes + 1);
    oversized_owner.extend_from_slice(b"FSV2\x02bad");
    assert!(matches!(
        compat(oversized_owner, &f::context(), limits),
        Err(E::Limit)
    ));
}

fn context_lanes(typed: &[u8]) -> TestResult {
    let missing = C {
        safe_atoms: None,
        ..f::context()
    };
    assert!(matches!(
        compat(typed.to_vec(), &missing, f::limits()),
        Err(E::UnsupportedContext { .. })
    ));
    let closed = C {
        safe_atoms: Some(&[]),
        ..f::context()
    };
    let (document, size) = compat(typed.to_vec(), &closed, f::limits())
        .map_err(|error| format!("closed table fallback: {error:?}"))?;
    assert_eq!(
        (f::binary(&document)?, size),
        (&typed[5..], typed.len() - 5)
    );
    assert!(matches!(
        decode(typed.to_vec(), &closed, f::limits()),
        Err(E::InvalidFlowValue)
    ));
    assert!(matches!(
        compat(
            b"FSV2\x02\x83M\0\0\0\0\0".to_vec(),
            &f::context(),
            f::limits()
        ),
        Err(E::UnsupportedProfile { tag: 77, offset: 6 })
    ));
    assert!(matches!(
        compat(b"FSV2\x02\x83I\0\x01".to_vec(), &f::context(), f::limits()),
        Err(E::UnsupportedContext { tag: 73, offset: 6 })
    ));
    Ok(())
}

fn owned_fallback(typed: &[u8]) -> TestResult {
    let mut trailing = typed.to_vec();
    trailing.push(0);
    let expected = trailing.clone();
    let pointer = trailing.as_ptr();
    let (document, size) = f::decoded(trailing)?;
    assert_eq!(document.encoded().as_ptr(), pointer);
    assert_eq!(document.encoded(), expected);
    assert_eq!(
        (f::binary(&document)?, size),
        (&expected[5..], expected.len() - 5)
    );
    assert!(document.node(1).is_none());
    assert!(document.edge_range(0..1).is_none());
    assert!(matches!(
        decode(expected, &f::context(), f::limits()),
        Err(E::InvalidFlowValue)
    ));
    Ok(())
}

fn continuous_work() -> TestResult {
    let duplicate = b"FSV2\x02\x83t\0\0\0\x02a\x07a\0a\x07a\0";
    let (document, size) = f::decoded(duplicate.to_vec())?;
    assert_eq!(
        (f::binary(&document)?, size),
        (&duplicate[5..], duplicate.len() - 5)
    );
    assert!(document.node(1).is_none());
    assert!(document.edge_range(0..1).is_none());
    let raw = vec![b'x'; duplicate.len()];
    let minimum = minimum_raw_work(&raw)?;
    let limited = L {
        max_work_units: minimum,
        ..f::limits()
    };
    assert!(compat(raw, &f::context(), limited).is_ok());
    assert!(matches!(
        compat(duplicate.to_vec(), &f::context(), limited),
        Err(E::Limit)
    ));
    Ok(())
}

fn minimum_raw_work(raw: &[u8]) -> TestResult<usize> {
    let mut low = 1;
    let mut high = f::limits().max_work_units;
    assert!(compat(raw.to_vec(), &f::context(), f::limits()).is_ok());
    while low < high {
        let middle = low + (high - low) / 2;
        let limited = L {
            max_work_units: middle,
            ..f::limits()
        };
        match compat(raw.to_vec(), &f::context(), limited) {
            Ok(_) => high = middle,
            Err(E::Limit) => low = middle + 1,
            Err(error) => return Err(format!("work boundary: {error:?}").into()),
        }
    }
    Ok(low)
}

fn wrapper_boundaries() -> TestResult {
    for (stored, value, size) in [
        (b"".as_slice(), b"".as_slice(), 0),
        (b"FSV2", b"FSV2", 4),
        (b"FSV2\0suffix", b"FSV2\0suffix", 11),
        (b"FSV2\x01", b"", 0),
        (b"FSV2\x02", b"", 0),
        (b"FSV2\x02\x83m\0\0\0\x01x", b"x", 7),
    ] {
        let (document, actual) = f::decoded(stored.to_vec())?;
        assert_eq!((f::binary(&document)?, actual), (value, size));
    }
    Ok(())
}

fn owned_after_context_scope() -> TestResult {
    let (document, _) = {
        let name = String::from("kind");
        let atoms = [name.as_str()];
        let context = C {
            safe_atoms: Some(&atoms),
            ..f::context()
        };
        compat(
            f::encoded_map(b"kind", b"typed", false)?,
            &context,
            f::limits(),
        )
        .map_err(|error| format!("owned context: {error:?}"))?
    };
    assert!(matches!(document.root(), Some(N::Map { .. })));
    assert_eq!(
        f::map_fields(&document)?,
        (b"kind".as_slice(), b"typed".as_slice())
    );
    Ok(())
}
