//! Pinned raw `external_size` controls for converted public patches.

use super::{
    OriginalPolicySetInputRejection, RawFlowPolicyValue, TestResult, assert_view, one, prepare,
    rejected,
};

#[test]
fn original_public_set_measures_converted_patch_before_lower_business() -> TestResult {
    let exact = vec![b'v'; 262_130];
    let above = vec![b'v'; 262_131];
    one("x", RawFlowPolicyValue::Binary(&exact), |request| {
        let (_, leaf) = request.patch().map_entry(0).ok_or("x")?;
        assert_raw(
            leaf.raw().ok_or("binary")?,
            RawFlowPolicyValue::Binary(&exact),
        )?;
        Ok(())
    })?;
    let pair = [
        RawFlowPolicyValue::Atom("x"),
        RawFlowPolicyValue::Binary(&above),
    ];
    rejected(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&[RawFlowPolicyValue::Tuple(&pair)]),
        OriginalPolicySetInputRejection::SnapshotBytes,
    )?;
    assert_byte_lists()?;
    super::assert_large_governance()?;
    Ok(())
}

fn assert_byte_lists() -> TestResult {
    // Pinned default external_size: map-x65535=65547; map-x65536=131087.
    for (count, width) in [(65_535, 65_538), (65_536, 131_078)] {
        let bytes = vec![RawFlowPolicyValue::Integer(255); count];
        one("x", RawFlowPolicyValue::List(&bytes), |request| {
            let (_, list) = request.patch().map_entry(0).ok_or("x")?;
            assert_eq!(list.list_len(), Some(count));
            for index in 0..count {
                assert_raw(
                    list.list_item(index)
                        .ok_or("byte")?
                        .raw()
                        .ok_or("integer")?,
                    RawFlowPolicyValue::Integer(255),
                )?;
            }
            Ok(())
        })?;
        assert_metric_boundary(RawFlowPolicyValue::List(&bytes), width)?;
    }
    let byte = [
        RawFlowPolicyValue::Integer(1),
        RawFlowPolicyValue::Integer(2),
    ];
    let nil = RawFlowPolicyValue::Nil;
    let improper = RawFlowPolicyValue::ImproperList {
        prefix: &byte,
        tail: &nil,
    };
    let tuple = [improper];
    one("x", RawFlowPolicyValue::Tuple(&tuple), |_| Ok(()))?;
    assert_metric_boundary(RawFlowPolicyValue::Tuple(&tuple), 16)?;
    for (value, width) in [
        (RawFlowPolicyValue::Nil, 5),
        (RawFlowPolicyValue::Atom("nil"), 5),
        (RawFlowPolicyValue::List(&[]), 1),
    ] {
        one("x", value, |_| Ok(()))?;
        assert_metric_boundary(value, width)?;
    }
    Ok(())
}

#[test]
fn original_public_set_preserves_arbitrary_integer_and_byte_terms() -> TestResult {
    let wide = [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0];
    let significant255 = vec![1_u8; 255];
    let significant256 = vec![1_u8; 256];
    for (value, width) in [
        (RawFlowPolicyValue::Integer(255), 2),
        (RawFlowPolicyValue::Integer(256), 5),
        (RawFlowPolicyValue::Integer(-1), 5),
        (RawFlowPolicyValue::Integer(-2_147_483_648), 5),
        (RawFlowPolicyValue::Integer(2_147_483_647), 5),
        (RawFlowPolicyValue::Integer(2_147_483_648), 7),
        (RawFlowPolicyValue::Integer(-2_147_483_649), 7),
        (RawFlowPolicyValue::Integer(i128::MAX), 19),
        (
            RawFlowPolicyValue::BigInteger {
                negative: true,
                little_endian: &[0, 0],
            },
            2,
        ),
        (
            RawFlowPolicyValue::BigInteger {
                negative: false,
                little_endian: &wide,
            },
            12,
        ),
        (
            RawFlowPolicyValue::BigInteger {
                negative: false,
                little_endian: &significant255,
            },
            258,
        ),
        (
            RawFlowPolicyValue::BigInteger {
                negative: true,
                little_endian: &significant256,
            },
            262,
        ),
    ] {
        let expected = [(RawFlowPolicyValue::Atom("version"), value)];
        one("version", value, |request| {
            assert_view(request.patch(), &RawFlowPolicyValue::Map(&expected))?;
            Ok(())
        })?;
        assert_metric_boundary(value, width)?;
    }
    assert_header_neighbors()?;
    Ok(())
}

fn assert_metric_boundary(value: RawFlowPolicyValue<'_>, term_length: usize) -> TestResult {
    // patch = %{x: term, z: binary(pad)}; VERSION+Map+keys+binary header=17.
    let padding = 262_144 - 17 - term_length;
    for extra in [0, 1] {
        let bytes = vec![b'z'; padding + extra];
        let x = [RawFlowPolicyValue::Atom("x"), value];
        let z = [
            RawFlowPolicyValue::Atom("z"),
            RawFlowPolicyValue::Binary(&bytes),
        ];
        let rows = [RawFlowPolicyValue::Tuple(&x), RawFlowPolicyValue::Tuple(&z)];
        if extra == 0 {
            let request = prepare(
                &RawFlowPolicyValue::Binary(b"r"),
                &RawFlowPolicyValue::List(&rows),
            )?;
            assert_eq!(request.patch().map_len(), Some(2));
        } else {
            rejected(
                &RawFlowPolicyValue::Binary(b"r"),
                &RawFlowPolicyValue::List(&rows),
                OriginalPolicySetInputRejection::SnapshotBytes,
            )?;
        }
    }
    Ok(())
}

fn assert_header_neighbors() -> TestResult {
    let atom255 = "é".repeat(127) + "x";
    let atom256 = "é".repeat(128);
    for (atom, width) in [(&atom255, 257), (&atom256, 259)] {
        one("x", RawFlowPolicyValue::Atom(atom), |request| {
            let (_, leaf) = request.patch().map_entry(0).ok_or("x")?;
            assert_raw(leaf.raw().ok_or("atom")?, RawFlowPolicyValue::Atom(atom))?;
            Ok(())
        })?;
        assert_metric_boundary(RawFlowPolicyValue::Atom(atom), width)?;
    }
    for (arity, width) in [(255, 512), (256, 517)] {
        let tuple = vec![RawFlowPolicyValue::Integer(0); arity];
        one("x", RawFlowPolicyValue::Tuple(&tuple), |request| {
            let (_, leaf) = request.patch().map_entry(0).ok_or("x")?;
            assert_raw(
                leaf.raw().ok_or("tuple")?,
                RawFlowPolicyValue::Tuple(&tuple),
            )?;
            Ok(())
        })?;
        assert_metric_boundary(RawFlowPolicyValue::Tuple(&tuple), width)?;
    }
    assert_metric_boundary(RawFlowPolicyValue::Binary(b"\xff\0"), 7)
}

pub(super) fn assert_default_work() -> TestResult {
    // Old all-prior filter alone costs134184960; the converted patch is49158B.
    let profile = wf_domain::policy::complete::PolicySchemaLimits::default();
    assert_eq!(profile.max_work_bytes(), 134_217_728);
    let names: Vec<_> = (0..4096).map(|i| format!("k{i:04}")).collect();
    let pairs: Vec<_> = names
        .iter()
        .map(|key| [RawFlowPolicyValue::Atom(key), RawFlowPolicyValue::Nil])
        .collect();
    let rows: Vec<_> = pairs
        .iter()
        .map(|pair| RawFlowPolicyValue::Tuple(pair))
        .collect();
    let mut charge = super::PolicySchemaCharge::new(profile);
    let request = super::prepare_original_policy_set_request(
        &RawFlowPolicyValue::Binary(b"r"),
        &RawFlowPolicyValue::List(&rows),
        profile,
        &mut charge,
    )?
    .map_err(|reason| format!("unexpected default-profile rejection {reason:?}"))?;
    assert_eq!(request.policy_key(), b"f:{f}:policy:r");
    assert_eq!(request.patch().map_len(), Some(4096));
    for (index, expected) in names.iter().enumerate() {
        let (key, value) = request.patch().map_entry(index).ok_or("surviving pair")?;
        assert_raw(
            key.raw().ok_or("atom key")?,
            RawFlowPolicyValue::Atom(expected),
        )?;
        assert_raw(value.raw().ok_or("nil value")?, RawFlowPolicyValue::Nil)?;
    }
    assert_eq!(charge.p(), 1_278_672);
    assert!(charge.total() <= profile.max_work_bytes());
    Ok(())
}

pub(super) fn assert_raw(
    actual: RawFlowPolicyValue<'_>,
    expected: RawFlowPolicyValue<'_>,
) -> TestResult {
    match (actual, expected) {
        (RawFlowPolicyValue::Nil, RawFlowPolicyValue::Nil)
        | (RawFlowPolicyValue::Other, RawFlowPolicyValue::Other) => {}
        (RawFlowPolicyValue::Integer(a), RawFlowPolicyValue::Integer(b)) => assert_eq!(a, b),
        (RawFlowPolicyValue::Atom(a), RawFlowPolicyValue::Atom(b)) => assert_eq!(a, b),
        (RawFlowPolicyValue::Binary(a), RawFlowPolicyValue::Binary(b)) => assert_eq!(a, b),
        (
            RawFlowPolicyValue::BigInteger {
                negative: a,
                little_endian: aa,
            },
            RawFlowPolicyValue::BigInteger {
                negative: b,
                little_endian: bb,
            },
        ) => {
            assert_eq!(a, b);
            assert_eq!(aa, bb);
        }
        (RawFlowPolicyValue::Map(a), RawFlowPolicyValue::Map(b)) => {
            assert_eq!(a.len(), b.len());
            for ((ak, av), (bk, bv)) in a.iter().zip(b) {
                assert_raw(*ak, *bk)?;
                assert_raw(*av, *bv)?;
            }
        }
        (RawFlowPolicyValue::List(a), RawFlowPolicyValue::List(b))
        | (RawFlowPolicyValue::Tuple(a), RawFlowPolicyValue::Tuple(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_raw(*a, *b)?;
            }
        }
        (
            RawFlowPolicyValue::ImproperList {
                prefix: a,
                tail: aa,
            },
            RawFlowPolicyValue::ImproperList {
                prefix: b,
                tail: bb,
            },
        ) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_raw(*a, *b)?;
            }
            assert_raw(*aa, *bb)?;
        }
        other => return Err(format!("different original terms: {other:?}").into()),
    }
    Ok(())
}
