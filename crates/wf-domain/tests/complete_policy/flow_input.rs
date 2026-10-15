//! Complete original incoming policy classes, source priorities and owned ETF.

use super::{PolicySchemaCharge, PolicySchemaLimits, TestResult, limits};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as Error, FlowPolicyInputRejection as Reason,
    GovernancePolicyInputRejection as Gov, PolicyIndexedInputRejection as Indexed,
    PolicyRetentionInputRejection as Retention, PolicyRetryInputRejection as Retry,
    PolicySchemaError, RawFlowPolicyValue as Raw,
    encode_original_complete_policy_input_rejection_body as encode,
    normalize_original_complete_policy_input as normalize,
};

mod cases;
mod etf;
mod governance;
mod refusals;
mod states;

use cases::{atom, binary, list, map, named_map, number, rejection, snapshot, success};

#[test]
fn original_flow_input_preserves_all_top_level_classes_and_presence() -> TestResult {
    assert_root_classes()?;
    cases::assert_nil_keyword_keys(false)?;
    assert_optional_presence()?;
    assert_top_shadowing()?;
    assert_bounded_big_integers()?;
    // Original4b707: retain actual type/state retry and retention values.
    let rows = [
        (Raw::Atom("max_active_ms"), Raw::Integer(120_000)),
        (Raw::Atom("retry"), original_retry()),
        (
            Raw::Atom("retention"),
            Raw::Map(&[
                (Raw::Atom("ttl_ms"), Raw::Integer(60_000)),
                (Raw::Atom("history_max_events"), Raw::Integer(512)),
            ]),
        ),
        (
            Raw::Atom("states"),
            Raw::Map(&[(
                Raw::Binary(b"charge_card"),
                Raw::Map(&[
                    (
                        Raw::Atom("retry"),
                        Raw::Map(&[
                            (Raw::Atom("max_retries"), Raw::Integer(2)),
                            (Raw::Atom("exhausted_to"), Raw::Binary(b"payment_failed")),
                        ]),
                    ),
                    (
                        Raw::Atom("retention"),
                        Raw::Map(&[
                            (Raw::Atom("ttl_ms"), Raw::Integer(30_000)),
                            (Raw::Atom("history_max_events"), Raw::Integer(256)),
                        ]),
                    ),
                ]),
            )]),
        ),
    ];
    let expected = original_expected()?;
    assert_eq!(success(&Raw::Map(&rows))?, expected);
    Ok(())
}

fn assert_root_classes() -> TestResult {
    let profile = limits(10_000_000)?;
    for input in [
        Raw::Nil,
        Raw::Integer(0),
        Raw::Binary(b""),
        Raw::Tuple(&[]),
        Raw::Other,
    ] {
        rejection(
            &input,
            Reason::RootShape,
            b"ERR flow policy must be a map or keyword list",
        )?;
        let mut charge = PolicySchemaCharge::new(profile);
        let expected_type = if matches!(input, Raw::Binary(b"")) {
            Ok(snapshot(b"", &[])?)
        } else {
            Err(Reason::RootShape)
        };
        assert_eq!(
            normalize(&input, &Raw::List(&[]), 17, profile, &mut charge)?,
            expected_type
        );
    }
    rejection(
        &Raw::List(&[Raw::Atom("not_keyword")]),
        Reason::RootKeyword,
        b"ERR flow opts must be a keyword list",
    )?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(&Raw::Binary(b""), &Raw::Map(&[]), 17, profile, &mut charge)?,
        Ok(snapshot(b"", &[])?)
    );
    assert_eq!(success(&Raw::List(&[]))?, snapshot(b"r", &[])?);
    for key in [Raw::Atom("mode"), Raw::Binary(b"mode")] {
        rejection(
            &Raw::Map(&[(key, Raw::Nil)]),
            Reason::TopStateMode,
            b"ERR flow state mode is state-level only",
        )?;
    }
    Ok(())
}

fn assert_optional_presence() -> TestResult {
    let empty = snapshot(b"r", &[])?;
    for name in [
        "version",
        "max_active_ms",
        "retry",
        "retention",
        "governance",
    ] {
        for value in [Raw::Nil, Raw::Atom("nil")] {
            assert_eq!(success(&Raw::Map(&[(Raw::Atom(name), value)]))?, empty);
        }
    }
    for value in [Raw::Nil, Raw::Atom("nil"), Raw::List(&[])] {
        let rows = [
            (Raw::Atom("indexed_attributes"), value),
            (Raw::Atom("indexed_state_meta"), value),
        ];
        assert_eq!(
            success(&Raw::Map(&rows))?,
            snapshot(b"r", &[("indexed_attributes", list(&[])?)])?
        );
    }
    for name in ["retry", "retention", "governance"] {
        for value in [Raw::Map(&[]), Raw::List(&[])] {
            assert_eq!(
                success(&Raw::Map(&[(Raw::Atom(name), value)]))?,
                snapshot(b"r", &[(name, map(&[])?)])?
            );
        }
    }
    for value in [
        Raw::Nil,
        Raw::Atom("nil"),
        Raw::Atom("infinity"),
        Raw::Binary(b"infinity"),
        Raw::Binary(b"INFINITY"),
    ] {
        assert_eq!(
            success(&Raw::Map(&[(Raw::Atom("max_active_ms"), value)]))?,
            empty
        );
    }
    for value in [
        Raw::Integer(0),
        Raw::Integer(31_536_000_001),
        Raw::Binary(b"Infinity"),
    ] {
        rejection(
            &Raw::Map(&[(Raw::Atom("max_active_ms"), value)]),
            Reason::MaxActive,
            b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
        )?;
    }
    Ok(())
}

fn assert_top_shadowing() -> TestResult {
    for (name, good, encoded) in [
        ("version", Raw::Binary(b"v"), binary(b"v")?),
        ("max_active_ms", Raw::Integer(1), number(1)?),
        ("retry", Raw::Map(&[]), map(&[])?),
        ("retention", Raw::Map(&[]), map(&[])?),
        ("governance", Raw::Map(&[]), map(&[])?),
        ("indexed_attributes", Raw::List(&[]), list(&[])?),
        (
            "indexed_state_meta",
            Raw::Binary(b"version"),
            binary(b"version")?,
        ),
        ("states", Raw::Map(&[]), map(&[])?),
    ] {
        let expected = snapshot(b"r", &[(name, encoded)])?;
        for reversed in [false, true] {
            let mut rows = [
                (Raw::Atom(name), good),
                (Raw::Binary(name.as_bytes()), Raw::Other),
            ];
            if reversed {
                rows.reverse();
            }
            assert_eq!(success(&Raw::Map(&rows))?, expected);
        }
        assert_invalid_atom_shadowing(name, good)?;
    }
    let first = [Raw::Atom("version"), Raw::Other];
    let last = [Raw::Atom("version"), Raw::Binary(b"last")];
    let rows = [Raw::Tuple(&first), Raw::Tuple(&last)];
    assert_eq!(
        success(&Raw::List(&rows))?,
        snapshot(b"r", &[("version", binary(b"last")?)])?
    );
    let bad = [
        (Raw::Atom("mode"), Raw::Nil),
        (Raw::Atom("version"), Raw::Other),
        (Raw::Atom("max_active_ms"), Raw::Integer(0)),
    ];
    rejection(
        &Raw::Map(&bad),
        Reason::TopStateMode,
        b"ERR flow state mode is state-level only",
    )?;
    Ok(())
}

fn assert_invalid_atom_shadowing(name: &str, good: Raw<'_>) -> TestResult {
    let (reason, body) = match name {
        "version" => (
            Reason::Version,
            b"ERR flow policy version must be a non-empty string or non-negative integer"
                .as_slice(),
        ),
        "max_active_ms" => (
            Reason::MaxActive,
            b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity".as_slice(),
        ),
        "retry" => (
            Reason::Retry(Retry::Shape),
            b"ERR flow retry policy must be a map or keyword list".as_slice(),
        ),
        "retention" => (
            Reason::Retention(Retention::Shape),
            b"ERR flow retention policy must be a map or keyword list".as_slice(),
        ),
        "indexed_attributes" => (
            Reason::Indexed(Indexed::AttributesList),
            b"ERR flow indexed_attributes must be a list".as_slice(),
        ),
        "indexed_state_meta" => (
            Reason::Indexed(Indexed::StateMetaShape),
            b"ERR flow indexed_state_meta must be a string or one-item list".as_slice(),
        ),
        "governance" => (
            Reason::Governance(Gov::Shape),
            b"ERR flow governance policy must be a map or keyword list".as_slice(),
        ),
        "states" => (
            Reason::StatesShape,
            b"ERR flow policy states must be a map or keyword list".as_slice(),
        ),
        _ => return Err("unlisted shadow fixture".into()),
    };
    for reversed in [false, true] {
        let mut rows = [
            (Raw::Atom(name), Raw::Other),
            (Raw::Binary(name.as_bytes()), good),
        ];
        if reversed {
            rows.reverse();
        }
        rejection(&Raw::Map(&rows), reason, body)?;
        if matches!(
            name,
            "version" | "max_active_ms" | "retry" | "retention" | "governance"
        ) {
            let mut rows = [
                (Raw::Atom(name), Raw::Nil),
                (Raw::Binary(name.as_bytes()), good),
            ];
            if reversed {
                rows.reverse();
            }
            assert_eq!(success(&Raw::Map(&rows))?, snapshot(b"r", &[])?);
        } else {
            cases::assert_indexed_state_nil_shadowing(name, good, reversed)?;
        }
    }
    Ok(())
}

fn original_retry() -> Raw<'static> {
    Raw::Map(&[
        (Raw::Atom("max_retries"), Raw::Integer(5)),
        (
            Raw::Atom("backoff"),
            Raw::Map(&[
                (Raw::Atom("kind"), Raw::Atom("fixed")),
                (Raw::Atom("base_ms"), Raw::Integer(10_000)),
                (Raw::Atom("max_ms"), Raw::Integer(30_000)),
                (Raw::Atom("jitter_pct"), Raw::Integer(0)),
            ]),
        ),
        (Raw::Atom("exhausted_to"), Raw::Binary(b"failed")),
    ])
}

fn original_expected() -> TestResult<Vec<u8>> {
    snapshot(
        b"r",
        &[
            ("max_active_ms", number(120_000)?),
            (
                "retention",
                map(&[
                    ("history_max_events", number(512)?),
                    ("ttl_ms", number(60_000)?),
                ])?,
            ),
            (
                "retry",
                map(&[
                    (
                        "backoff",
                        map(&[
                            ("base_ms", number(10_000)?),
                            ("jitter_pct", number(0)?),
                            ("kind", atom("fixed")?),
                            ("max_ms", number(30_000)?),
                        ])?,
                    ),
                    ("exhausted_to", binary(b"failed")?),
                    ("max_retries", number(5)?),
                ])?,
            ),
            (
                "states",
                named_map(&[(
                    b"charge_card".to_vec(),
                    map(&[
                        (
                            "retention",
                            map(&[
                                ("history_max_events", number(256)?),
                                ("ttl_ms", number(30_000)?),
                            ])?,
                        ),
                        (
                            "retry",
                            map(&[
                                ("exhausted_to", binary(b"payment_failed")?),
                                ("max_retries", number(2)?),
                            ])?,
                        ),
                    ])?,
                )])?,
            ),
        ],
    )
}

#[test]
fn original_flow_input_stages_once_without_mutating_inputs() -> TestResult {
    let expected = snapshot(
        b"r",
        &[
            (
                "indexed_attributes",
                list(&[binary(b"owner")?, binary(b"version")?])?,
            ),
            ("indexed_state_meta", binary(b"version")?),
            ("retry", map(&[("exhausted_to", binary(&[0, 255, b' '])?)])?),
            ("version", binary(&[255, 0, b'v'])?),
        ],
    )?;
    let owned = {
        let owner = String::from(" owner ");
        let version = String::from("version");
        let target = vec![0, 255, b' '];
        let customer = vec![255, 0, b'v'];
        let names = [
            Raw::Binary(owner.as_bytes()),
            Raw::Atom(&version),
            Raw::Atom(&owner),
        ];
        let meta = [Raw::Nil, Raw::List(&[Raw::Binary(b" version ")])];
        let retry = [(Raw::Atom("exhausted_to"), Raw::Binary(&target))];
        let rows = [
            (Raw::Atom("indexed_attributes"), Raw::List(&names)),
            (Raw::Atom("indexed_state_meta"), Raw::List(&meta)),
            (Raw::Atom("retry"), Raw::Map(&retry)),
            (Raw::Atom("version"), Raw::Binary(&customer)),
            (
                Raw::Atom("ignored"),
                Raw::Tuple(&[Raw::Other, Raw::Integer(-7)]),
            ),
        ];
        let output = success(&Raw::Map(&rows))?;
        assert_eq!(owner, " owner ");
        assert_eq!(version, "version");
        assert_eq!(target, [0, 255, b' ']);
        assert_eq!(customer, [255, 0, b'v']);
        assert_ne!(output.as_ptr(), customer.as_ptr());
        output
    };
    assert_eq!(owned, expected);
    assert_root_error_order()?;
    Ok(())
}

fn assert_root_error_order() -> TestResult {
    let invalid = [
        (
            "version",
            Raw::Other,
            Reason::Version,
            b"ERR flow policy version must be a non-empty string or non-negative integer"
                .as_slice(),
        ),
        (
            "max_active_ms",
            Raw::Integer(0),
            Reason::MaxActive,
            b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity".as_slice(),
        ),
        (
            "retry",
            Raw::Other,
            Reason::Retry(Retry::Shape),
            b"ERR flow retry policy must be a map or keyword list".as_slice(),
        ),
        (
            "retention",
            Raw::Other,
            Reason::Retention(Retention::Shape),
            b"ERR flow retention policy must be a map or keyword list".as_slice(),
        ),
        (
            "indexed_attributes",
            Raw::Other,
            Reason::Indexed(Indexed::AttributesList),
            b"ERR flow indexed_attributes must be a list".as_slice(),
        ),
        (
            "indexed_state_meta",
            Raw::Other,
            Reason::Indexed(Indexed::StateMetaShape),
            b"ERR flow indexed_state_meta must be a string or one-item list".as_slice(),
        ),
        (
            "governance",
            Raw::Other,
            Reason::Governance(Gov::Shape),
            b"ERR flow governance policy must be a map or keyword list".as_slice(),
        ),
        (
            "states",
            Raw::Other,
            Reason::StatesShape,
            b"ERR flow policy states must be a map or keyword list".as_slice(),
        ),
    ];
    for first in 0..invalid.len() {
        let rows = invalid[first..]
            .iter()
            .map(|(name, value, _, _)| (Raw::Atom(name), *value))
            .collect::<Vec<_>>();
        rejection(&Raw::Map(&rows), invalid[first].2, invalid[first].3)?;
    }
    Ok(())
}

fn assert_bounded_big_integers() -> TestResult {
    let one = Raw::BigInteger {
        negative: false,
        little_endian: &[1, 0, 0],
    };
    let zero = Raw::BigInteger {
        negative: true,
        little_endian: &[0, 0],
    };
    let backoff = [
        (Raw::Atom("kind"), Raw::Atom("fixed")),
        (Raw::Atom("base_ms"), one),
        (Raw::Atom("max_ms"), one),
        (Raw::Atom("jitter_pct"), zero),
    ];
    let retry = [
        (Raw::Atom("max_retries"), zero),
        (Raw::Atom("backoff"), Raw::Map(&backoff)),
    ];
    let retention = [
        (Raw::Atom("ttl_ms"), one),
        (Raw::Atom("history_max_events"), one),
    ];
    let input = [
        (Raw::Atom("max_active_ms"), one),
        (Raw::Atom("retry"), Raw::Map(&retry)),
        (Raw::Atom("retention"), Raw::Map(&retention)),
    ];
    let expected = snapshot(
        b"r",
        &[
            ("max_active_ms", number(1)?),
            (
                "retention",
                map(&[("history_max_events", number(1)?), ("ttl_ms", number(1)?)])?,
            ),
            (
                "retry",
                map(&[
                    (
                        "backoff",
                        map(&[
                            ("base_ms", number(1)?),
                            ("jitter_pct", number(0)?),
                            ("kind", atom("fixed")?),
                            ("max_ms", number(1)?),
                        ])?,
                    ),
                    ("max_retries", number(0)?),
                ])?,
            ),
        ],
    )?;
    assert_eq!(success(&Raw::Map(&input))?, expected);
    assert_big_integer_errors()
}

fn assert_big_integer_errors() -> TestResult {
    let oversized = [0, 0, 0, 0, 0, 0, 0, 0, 1];
    for bad in [
        Raw::BigInteger {
            negative: true,
            little_endian: &[1],
        },
        Raw::BigInteger {
            negative: false,
            little_endian: &oversized,
        },
    ] {
        rejection(
            &Raw::Map(&[(Raw::Atom("max_active_ms"), bad)]),
            Reason::MaxActive,
            b"ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
        )?;
        rejection(
            &Raw::Map(&[(
                Raw::Atom("retry"),
                Raw::Map(&[(Raw::Atom("max_retries"), bad)]),
            )]),
            Reason::Retry(Retry::MaxRetries),
            b"ERR flow retry max_retries must be between 0 and 1000",
        )?;
        rejection(
            &Raw::Map(&[(
                Raw::Atom("retention"),
                Raw::Map(&[(Raw::Atom("ttl_ms"), bad)]),
            )]),
            Reason::Retention(Retention::Ttl),
            b"ERR flow retention ttl_ms must be between 1 and 31536000000",
        )?;
        rejection(
            &Raw::Map(&[(
                Raw::Atom("retention"),
                Raw::Map(&[(Raw::Atom("history_max_events"), bad)]),
            )]),
            Reason::Retention(Retention::History { maximum: 17 }),
            b"ERR flow retention history_max_events must be between 1 and 17",
        )?;
        for (field, reason, body) in [
            (
                "base_ms",
                Retry::BaseMillis,
                b"ERR flow retry base_ms must be between 0 and 2592000000".as_slice(),
            ),
            (
                "max_ms",
                Retry::MaxMillis,
                b"ERR flow retry max_ms must be between 0 and 2592000000".as_slice(),
            ),
            (
                "jitter_pct",
                Retry::JitterPercent,
                b"ERR flow retry jitter_pct must be between 0 and 100".as_slice(),
            ),
        ] {
            let backoff = [(Raw::Atom(field), bad)];
            let retry = [(Raw::Atom("backoff"), Raw::Map(&backoff))];
            rejection(
                &Raw::Map(&[(Raw::Atom("retry"), Raw::Map(&retry))]),
                Reason::Retry(reason),
                body,
            )?;
        }
    }
    Ok(())
}
