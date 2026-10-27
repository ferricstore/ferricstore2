//! State pair conversion, source reduction order and nested workspace lifetime.

use super::{Error, PolicySchemaCharge, PolicySchemaError, Raw, Reason, TestResult};
use super::{atom, binary, encode, limits, list, map, named_map, normalize};
use super::{number, rejection, snapshot, success};

#[test]
fn original_flow_input_preserves_state_map_keyword_and_pair_semantics() -> TestResult {
    assert_state_classes()?;
    super::cases::assert_nil_keyword_keys(true)?;
    assert_modes_and_duplicates()?;
    assert_improper_source_paths()?;
    assert_concatenated_lists()?;
    // Original6ad2cd82: queued keyword max_active_ms1000 is forbidden.
    let option = [Raw::Atom("max_active_ms"), Raw::Integer(1_000)];
    let state = [(Raw::Binary(b"queued"), Raw::List(&[Raw::Tuple(&option)]))];
    let fields = [(Raw::Atom("states"), Raw::Map(&state))];
    let profile = limits(10_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let actual = normalize(
        &Raw::Binary(b"r"),
        &Raw::Map(&fields),
        17,
        profile,
        &mut charge,
    )?;
    assert_eq!(actual, Err(Reason::StateMaxActive));
    assert_eq!(
        encode(Reason::StateMaxActive, profile, &mut charge)?,
        b"ERR flow max_active_ms is type-level only"
    );
    Ok(())
}

fn assert_state_classes() -> TestResult {
    for value in [Raw::Nil, Raw::Other, Raw::Integer(0), Raw::Binary(b"")] {
        rejection(
            &Raw::Map(&[(Raw::Atom("states"), value)]),
            Reason::StatesShape,
            b"ERR flow policy states must be a map or keyword list",
        )?;
    }
    for value in [
        Raw::Other,
        Raw::Tuple(&[]),
        Raw::Tuple(&[Raw::Binary(b"s")]),
        Raw::Tuple(&[Raw::Binary(b"s"), Raw::Nil]),
        Raw::Tuple(&[Raw::Binary(b"s"), Raw::Map(&[]), Raw::Nil]),
    ] {
        rejection(
            &Raw::Map(&[(Raw::Atom("states"), Raw::List(&[value]))]),
            Reason::StatesList,
            b"ERR flow policy states must be a map, keyword list, or state-policy pair list",
        )?;
    }
    for (name, reason, body) in [
        (
            Raw::Binary(b""),
            Reason::StateName,
            b"ERR flow policy state must be a non-empty string".as_slice(),
        ),
        (
            Raw::Atom("queued"),
            Reason::StateName,
            b"ERR flow policy state must be a non-empty string".as_slice(),
        ),
        (
            Raw::Binary(b"running"),
            Reason::RunningState,
            b"ERR flow policy state cannot be running".as_slice(),
        ),
    ] {
        let states = [(name, Raw::Other)];
        rejection(
            &Raw::Map(&[(Raw::Atom("states"), Raw::Map(&states))]),
            reason,
            body,
        )?;
    }
    let keyword = [Raw::Atom("queued"), Raw::Map(&[])];
    rejection(
        &Raw::Map(&[(Raw::Atom("states"), Raw::List(&[Raw::Tuple(&keyword)]))]),
        Reason::StateName,
        b"ERR flow policy state must be a non-empty string",
    )?;
    for value in [
        Raw::Nil,
        Raw::Other,
        Raw::Integer(0),
        Raw::List(&[Raw::Atom("bad")]),
    ] {
        state_reason(
            value,
            Reason::StatePolicyShape,
            b"ERR flow state policy must be a map or keyword list",
        )?;
    }
    Ok(())
}

fn assert_modes_and_duplicates() -> TestResult {
    for (value, expected) in [
        (Raw::Atom("fifo"), "fifo"),
        (Raw::Atom("parallel"), "parallel"),
        (Raw::Binary(b"FIFO"), "fifo"),
        (Raw::Binary(b"PaRaLLeL"), "parallel"),
    ] {
        let policy = [(Raw::Atom("mode"), value)];
        let states = [(Raw::Binary(b"queued"), Raw::Map(&policy))];
        let expected = snapshot(
            b"r",
            &[(
                "states",
                named_map(&[(b"queued".to_vec(), map(&[("mode", atom(expected)?)])?)])?,
            )],
        )?;
        assert_eq!(
            success(&Raw::Map(&[(Raw::Atom("states"), Raw::Map(&states))]))?,
            expected
        );
    }
    for value in [
        Raw::Nil,
        Raw::Atom("nil"),
        Raw::Atom("FIFO"),
        Raw::Binary(b"\xffFIFO"),
        Raw::Binary(b" fifo"),
        Raw::Other,
    ] {
        state_reason(
            Raw::Map(&[(Raw::Atom("mode"), value)]),
            Reason::StateMode,
            b"ERR flow state mode must be parallel or fifo",
        )?;
    }
    let bad = [
        Raw::Binary(b"queued"),
        Raw::Map(&[(Raw::Atom("mode"), Raw::Other)]),
    ];
    let good = [
        Raw::Binary(b"queued"),
        Raw::Map(&[(Raw::Atom("mode"), Raw::Atom("fifo"))]),
    ];
    let expected = snapshot(
        b"r",
        &[(
            "states",
            named_map(&[(b"queued".to_vec(), map(&[("mode", atom("fifo")?)])?)])?,
        )],
    )?;
    assert_eq!(
        success(&Raw::Map(&[(
            Raw::Atom("states"),
            Raw::List(&[Raw::Tuple(&bad), Raw::Tuple(&good)])
        )]))?,
        expected
    );
    rejection(
        &Raw::Map(&[(
            Raw::Atom("states"),
            Raw::List(&[Raw::Tuple(&good), Raw::Tuple(&bad)]),
        )]),
        Reason::StateMode,
        b"ERR flow state mode must be parallel or fifo",
    )?;
    Ok(())
}

fn state_reason(policy: Raw<'_>, reason: Reason, body: &[u8]) -> TestResult {
    let states = [(Raw::Binary(b"queued"), policy)];
    rejection(
        &Raw::Map(&[(Raw::Atom("states"), Raw::Map(&states))]),
        reason,
        body,
    )
}

fn assert_improper_source_paths() -> TestResult {
    let valid_pair = [Raw::Binary(b"queued"), Raw::Map(&[])];
    let prefix = [Raw::Tuple(&valid_pair)];
    for tail in [Raw::Nil, Raw::Atom("nil"), Raw::Atom("tail")] {
        let states = Raw::ImproperList {
            prefix: &prefix,
            tail: &tail,
        };
        let input = [(Raw::Atom("states"), states)];
        let profile = limits(10_000_000)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            normalize(
                &Raw::Binary(b"r"),
                &Raw::Map(&input),
                17,
                profile,
                &mut charge
            ),
            Err(Error::Schema(PolicySchemaError::InvalidAuthority))
        );
    }
    let tail = Raw::List(&[]);
    let states = Raw::ImproperList {
        prefix: &prefix,
        tail: &tail,
    };
    assert_eq!(
        success(&Raw::Map(&[(Raw::Atom("states"), states)]))?,
        snapshot(
            b"r",
            &[("states", named_map(&[(b"queued".to_vec(), map(&[])?)])?)]
        )?
    );
    let tail = Raw::Atom("tail");
    let bad = [Raw::Other];
    rejection(
        &Raw::Map(&[(
            Raw::Atom("states"),
            Raw::ImproperList {
                prefix: &bad,
                tail: &tail,
            },
        )]),
        Reason::StatesList,
        b"ERR flow policy states must be a map, keyword list, or state-policy pair list",
    )?;
    let option = [Raw::Atom("version"), Raw::Binary(b"v")];
    let prefix = [Raw::Tuple(&option)];
    rejection(
        &Raw::ImproperList {
            prefix: &prefix,
            tail: &Raw::Nil,
        },
        Reason::RootKeyword,
        b"ERR flow opts must be a keyword list",
    )?;
    assert_indexed_improper()?;
    Ok(())
}

fn assert_indexed_improper() -> TestResult {
    let tail = Raw::Nil;
    let profile = limits(10_000_000)?;
    for name in ["indexed_attributes", "indexed_state_meta"] {
        for leaf in [Raw::Binary(b"x"), Raw::Nil] {
            let prefix = [leaf];
            let fields = [(
                Raw::Atom(name),
                Raw::ImproperList {
                    prefix: &prefix,
                    tail: &tail,
                },
            )];
            let mut charge = PolicySchemaCharge::new(profile);
            assert_eq!(
                normalize(
                    &Raw::Binary(b"r"),
                    &Raw::Map(&fields),
                    17,
                    profile,
                    &mut charge
                ),
                Err(Error::Schema(PolicySchemaError::InvalidAuthority))
            );
        }
    }
    let prefix = [Raw::Binary(b"")];
    rejection(
        &Raw::Map(&[(
            Raw::Atom("indexed_attributes"),
            Raw::ImproperList {
                prefix: &prefix,
                tail: &tail,
            },
        )]),
        Reason::Indexed(super::Indexed::AttributeEmpty),
        b"ERR flow attribute key must not be empty",
    )?;
    let prefix = [Raw::Integer(17)];
    rejection(
        &Raw::Map(&[(
            Raw::Atom("indexed_state_meta"),
            Raw::ImproperList {
                prefix: &prefix,
                tail: &tail,
            },
        )]),
        Reason::Indexed(super::Indexed::StateMetaShape),
        b"ERR flow indexed_state_meta must be a string or one-item list",
    )?;
    Ok(())
}

#[test]
fn original_flow_input_preserves_source_state_iteration_and_field_order() -> TestResult {
    for count in [32, 33] {
        assert_order(count)?;
        assert_nested_workspace(count)?;
    }
    assert_demotion()?;
    assert_long_hash_order()?;
    let invalid = [
        (
            "max_active_ms",
            Raw::Nil,
            Reason::StateMaxActive,
            b"ERR flow max_active_ms is type-level only".as_slice(),
        ),
        (
            "mode",
            Raw::Nil,
            Reason::StateMode,
            b"ERR flow state mode must be parallel or fifo".as_slice(),
        ),
        (
            "retry",
            Raw::Other,
            Reason::Retry(super::Retry::Shape),
            b"ERR flow retry policy must be a map or keyword list".as_slice(),
        ),
        (
            "retention",
            Raw::Other,
            Reason::Retention(super::Retention::Shape),
            b"ERR flow retention policy must be a map or keyword list".as_slice(),
        ),
        (
            "governance",
            Raw::Other,
            Reason::Governance(super::Gov::Shape),
            b"ERR flow governance policy must be a map or keyword list".as_slice(),
        ),
    ];
    for first in 0..invalid.len() {
        let rows = invalid[first..]
            .iter()
            .map(|(name, value, _, _)| (Raw::Atom(name), *value))
            .collect::<Vec<_>>();
        state_reason(Raw::Map(&rows), invalid[first].2, invalid[first].3)?;
    }
    Ok(())
}

fn assert_order(count: usize) -> TestResult {
    let names = (1..=count).map(|i| format!("s{i:02}")).collect::<Vec<_>>();
    // Pinned release64LE Enum.reduce_while/maps.to_list; NOT Enum.map.
    let order = [
        1, 3, 22, 19, 6, 23, 16, 27, 5, 21, 7, 33, 32, 28, 24, 11, 10, 20, 31, 15, 18, 2, 14, 30,
        9, 25, 4, 26, 12, 8, 17, 13, 29,
    ];
    let first = 1;
    let last = if count == 32 { 32 } else { 29 };
    let mode = [(Raw::Atom("mode"), Raw::Other)];
    let active = [(Raw::Atom("max_active_ms"), Raw::Integer(1))];
    let policies = (1..=count)
        .map(|i| {
            if i == first {
                Raw::Map(&mode)
            } else if i == last {
                Raw::Map(&active)
            } else {
                Raw::Map(&[])
            }
        })
        .collect::<Vec<_>>();
    let pairs = names
        .iter()
        .zip(&policies)
        .map(|(name, policy)| [Raw::Binary(name.as_bytes()), *policy])
        .collect::<Vec<_>>();
    let leaves = pairs
        .iter()
        .map(|pair| Raw::Tuple(pair))
        .collect::<Vec<_>>();
    rejection(
        &Raw::Map(&[(Raw::Atom("states"), Raw::List(&leaves))]),
        Reason::StateMode,
        b"ERR flow state mode must be parallel or fifo",
    )?;
    let direct = if count == 32 {
        (0..32)
            .map(|i| (Raw::Binary(names[i].as_bytes()), policies[i]))
            .collect::<Vec<_>>()
    } else {
        order
            .iter()
            .map(|i| (Raw::Binary(names[i - 1].as_bytes()), policies[i - 1]))
            .collect::<Vec<_>>()
    };
    rejection(
        &Raw::Map(&[(Raw::Atom("states"), Raw::Map(&direct))]),
        Reason::StateMode,
        b"ERR flow state mode must be parallel or fifo",
    )?;
    Ok(())
}

fn assert_demotion() -> TestResult {
    let names = (1..=32).map(|i| format!("s{i:02}")).collect::<Vec<_>>();
    let bad_mode = [(Raw::Atom("mode"), Raw::Other)];
    let bad_active = [(Raw::Atom("max_active_ms"), Raw::Integer(1))];
    let good = [(Raw::Atom("mode"), Raw::Atom("fifo"))];
    let mut pairs = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let policy = if i == 0 || i == 1 {
                Raw::Map(&bad_mode)
            } else if i == 31 {
                Raw::Map(&bad_active)
            } else {
                Raw::Map(&[])
            };
            [Raw::Binary(name.as_bytes()), policy]
        })
        .collect::<Vec<_>>();
    pairs.push([Raw::Binary(names[0].as_bytes()), Raw::Map(&good)]);
    let leaves = pairs
        .iter()
        .map(|pair| Raw::Tuple(pair))
        .collect::<Vec<_>>();
    rejection(
        &Raw::Map(&[(Raw::Atom("states"), Raw::List(&leaves))]),
        Reason::StateMode,
        b"ERR flow state mode must be parallel or fifo",
    )?;
    Ok(())
}

fn assert_nested_workspace(count: usize) -> TestResult {
    let names = (1..=count).map(|i| format!("s{i:02}")).collect::<Vec<_>>();
    let names_raw = [Raw::Binary(b"b"), Raw::Binary(b"a"), Raw::Binary(b"b")];
    let effects = [(Raw::Atom("allowed"), Raw::List(&names_raw))];
    let limit = [(Raw::Atom("limit"), Raw::Integer(1))];
    let rules = [(Raw::Binary(b"running"), Raw::Map(&limit))];
    let governance = [
        (Raw::Atom("effects"), Raw::Map(&effects)),
        (Raw::Atom("limits"), Raw::Map(&rules)),
    ];
    let policy = [(Raw::Atom("governance"), Raw::Map(&governance))];
    let pairs = names
        .iter()
        .map(|name| [Raw::Binary(name.as_bytes()), Raw::Map(&policy)])
        .collect::<Vec<_>>();
    let raw_pairs = pairs
        .iter()
        .map(|pair| Raw::Tuple(pair))
        .collect::<Vec<_>>();
    let expected_state = map(&[(
        "governance",
        map(&[
            (
                "effects",
                map(&[("allowed", list(&[binary(b"b")?, binary(b"a")?])?)])?,
            ),
            (
                "limits",
                named_map(&[(b"running".to_vec(), map(&[("limit", number(1)?)])?)])?,
            ),
        ])?,
    )])?;
    let expected_rows = names
        .iter()
        .map(|name| (name.as_bytes().to_vec(), expected_state.clone()))
        .collect::<Vec<_>>();
    let expected = snapshot(b"r", &[("states", named_map(&expected_rows)?)])?;
    assert_eq!(
        success(&Raw::Map(&[(Raw::Atom("states"), Raw::List(&raw_pairs))]))?,
        expected
    );
    // Source normalization does not import the supplied typed-state budget.
    let names = ["state-0", "state-1"];
    let rows = names
        .iter()
        .map(|name| (Raw::Binary(name.as_bytes()), Raw::Map(&[])))
        .collect::<Vec<_>>();
    let expected_rows = names
        .iter()
        .map(|name| Ok((name.as_bytes().to_vec(), map(&[])?)))
        .collect::<TestResult<Vec<_>>>()?;
    let profile = super::PolicySchemaLimits::new(
        wf_domain::policy::installation::PolicyLimits::new(1)?,
        10_000_000,
    )?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(
            &Raw::Binary(b"r"),
            &Raw::Map(&[(Raw::Atom("states"), Raw::Map(&rows))]),
            17,
            profile,
            &mut charge
        )?,
        Ok(snapshot(b"r", &[("states", named_map(&expected_rows)?)])?)
    );
    Ok(())
}

fn assert_concatenated_lists() -> TestResult {
    let first = [
        Raw::Binary(b"queued"),
        Raw::Map(&[(Raw::Atom("mode"), Raw::Other)]),
    ];
    let last = [
        Raw::Binary(b"queued"),
        Raw::Map(&[(Raw::Atom("mode"), Raw::Atom("fifo"))]),
    ];
    let opaque = [Raw::Binary(&[255, 0]), Raw::Map(&[])];
    let prefix = [Raw::Tuple(&first)];
    let later = [Raw::Tuple(&last), Raw::Tuple(&opaque)];
    let tail = Raw::List(&later);
    let states = Raw::ImproperList {
        prefix: &prefix,
        tail: &tail,
    };
    let expected = snapshot(
        b"r",
        &[(
            "states",
            named_map(&[
                (b"queued".to_vec(), map(&[("mode", atom("fifo")?)])?),
                (vec![255, 0], map(&[])?),
            ])?,
        )],
    )?;
    assert_eq!(
        success(&Raw::Map(&[(Raw::Atom("states"), states)]))?,
        expected
    );
    let first = [Raw::Atom("version"), Raw::Other];
    let last = [Raw::Atom("version"), Raw::Binary(b"last")];
    let prefix = [Raw::Tuple(&first)];
    let later = [Raw::Tuple(&last)];
    let tail = Raw::List(&later);
    assert_eq!(
        success(&Raw::ImproperList {
            prefix: &prefix,
            tail: &tail
        })?,
        snapshot(b"r", &[("version", binary(b"last")?)])?
    );
    Ok(())
}

fn assert_long_hash_order() -> TestResult {
    let mut names = (1..=21)
        .map(|i| format!("s{i:02}").into_bytes())
        .collect::<Vec<_>>();
    names.extend([1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 64, 65].map(|n| vec![b'x'; n]));
    // Pinned native0282a3, before production: long hash blocks and all tail sizes.
    let order = [
        0, 2, 31, 27, 22, 18, 25, 5, 15, 4, 20, 6, 24, 10, 9, 21, 19, 23, 28, 14, 17, 1, 13, 30,
        29, 8, 3, 26, 11, 7, 16, 12, 32,
    ];
    let bad_mode = [(Raw::Atom("mode"), Raw::Other)];
    let bad_active = [(Raw::Atom("max_active_ms"), Raw::Integer(1))];
    let cases = order
        .windows(2)
        .flat_map(|adjacent| [false, true].map(move |mode_first| (adjacent, mode_first)));
    for (adjacent, mode_first) in cases {
        let mode_at = adjacent[usize::from(!mode_first)];
        let active_at = adjacent[usize::from(mode_first)];
        let pairs = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let value = if index == mode_at {
                    Raw::Map(&bad_mode)
                } else if index == active_at {
                    Raw::Map(&bad_active)
                } else {
                    Raw::Map(&[])
                };
                [Raw::Binary(name), value]
            })
            .collect::<Vec<_>>();
        let leaves = pairs
            .iter()
            .map(|pair| Raw::Tuple(pair))
            .collect::<Vec<_>>();
        let (reason, body) = if mode_first {
            (
                Reason::StateMode,
                b"ERR flow state mode must be parallel or fifo".as_slice(),
            )
        } else {
            (
                Reason::StateMaxActive,
                b"ERR flow max_active_ms is type-level only".as_slice(),
            )
        };
        rejection(
            &Raw::Map(&[(Raw::Atom("states"), Raw::List(&leaves))]),
            reason,
            body,
        )?;
    }
    Ok(())
}
