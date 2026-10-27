//! Original optional retention classes, source ordering and captured diagnostics.

use super::{PolicySchemaCharge, PolicySchemaLimits, TestResult, limits};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as Error, OriginalPolicyMapKey as Key,
    PolicyRetentionInputRejection as Reason, PolicySchemaError, RawPolicyRetryValue as Raw,
    encode_complete_policy_retention_input_rejection_body as encode,
    normalize_complete_policy_retention_override_input as normalize,
};
use wf_domain::retention::RetentionOverride;

mod refusals;

fn normalized(
    input: &Raw<'_>,
    maximum: u32,
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<Option<RetentionOverride>> {
    normalize(input, maximum, profile, charge)?
        .map_err(|reason| format!("unexpected retention {reason:?}").into())
}

#[test]
fn retention_input_preserves_optional_presence_and_original_type_state_values() -> TestResult {
    let profile = limits(10_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    for input in [Raw::Nil, Raw::Atom("nil")] {
        assert_eq!(normalized(&input, 1_000_000, profile, &mut charge)?, None);
    }
    for input in [Raw::Map(&[]), Raw::Keyword(&[])] {
        assert_eq!(
            normalized(&input, 1_000_000, profile, &mut charge)?,
            Some(RetentionOverride::default())
        );
    }
    // Source4b707: type60000/512 and charge_card30000/256; source3e46:5000/9.
    for (ttl, history) in [(60_000, 512), (30_000, 256), (5_000, 9)] {
        let fields = [
            ("ttl_ms", Raw::Integer(ttl)),
            ("history_max_events", Raw::Integer(history)),
        ];
        let actual = normalized(&Raw::Keyword(&fields), 1_000_000, profile, &mut charge)?
            .ok_or("original present retention")?;
        assert_eq!(actual.ttl_ms, Some(u64::try_from(ttl)?));
        assert_eq!(actual.history_max_events, Some(u32::try_from(history)?));
    }
    for (name, value, expected) in [
        (
            "ttl_ms",
            1,
            RetentionOverride {
                ttl_ms: Some(1),
                history_max_events: None,
            },
        ),
        (
            "history_max_events",
            1,
            RetentionOverride {
                ttl_ms: None,
                history_max_events: Some(1),
            },
        ),
    ] {
        let rows = [(Key::Binary(name.as_bytes()), Raw::Integer(value))];
        assert_eq!(
            normalized(&Raw::Map(&rows), 17, profile, &mut charge)?,
            Some(expected)
        );
    }
    let copied = {
        let name = String::from("ttl_ms");
        let ignored = vec![0, 255, b' '];
        let rows = [
            (Key::Atom(&name), Raw::Integer(60_000)),
            (Key::Other, Raw::Binary(&ignored)),
        ];
        let result = normalized(&Raw::Map(&rows), 17, profile, &mut charge)?;
        assert_eq!(name, "ttl_ms");
        assert_eq!(ignored, [0, 255, b' ']);
        result
    };
    assert_eq!(
        copied,
        Some(RetentionOverride {
            ttl_ms: Some(60_000),
            history_max_events: None
        })
    );
    Ok(())
}

#[test]
fn retention_input_preserves_atom_priority_and_keyword_last_values() -> TestResult {
    let profile = limits(10_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    for (name, reason) in [
        ("ttl_ms", Reason::Ttl),
        ("history_max_events", Reason::History { maximum: 17 }),
        ("history_hot_max_events", Reason::HistoryHot),
    ] {
        for reversed in [false, true] {
            let mut rows = [
                (Key::Atom(name), Raw::Nil),
                (Key::Binary(name.as_bytes()), Raw::Integer(1)),
            ];
            if reversed {
                rows.reverse();
            }
            assert_eq!(
                normalize(&Raw::Map(&rows), 17, profile, &mut charge)?,
                Err(reason)
            );
        }
        for key in [Key::Atom(name), Key::Binary(name.as_bytes())] {
            let rows = [(key, Raw::Integer(1))];
            let actual = normalize(&Raw::Map(&rows), 17, profile, &mut charge)?;
            if name == "history_hot_max_events" {
                assert_eq!(actual, Err(Reason::HistoryHot));
            } else {
                assert_eq!(
                    actual,
                    Ok(Some(RetentionOverride {
                        ttl_ms: (name == "ttl_ms").then_some(1),
                        history_max_events: (name == "history_max_events").then_some(1),
                    }))
                );
            }
        }
    }
    for (name, bad, reason) in [
        ("ttl_ms", 0, Reason::Ttl),
        ("history_max_events", 18, Reason::History { maximum: 17 }),
    ] {
        for good_last in [true, false] {
            let mut rows = [(name, Raw::Integer(bad)), (name, Raw::Integer(1))];
            if !good_last {
                rows.reverse();
            }
            let actual = normalize(&Raw::Keyword(&rows), 17, profile, &mut charge)?;
            if good_last {
                let expected = RetentionOverride {
                    ttl_ms: (name == "ttl_ms").then_some(1),
                    history_max_events: (name == "history_max_events").then_some(1),
                };
                assert_eq!(actual, Ok(Some(expected)));
            } else {
                assert_eq!(actual, Err(reason));
            }
        }
    }
    for value in [Raw::Nil, Raw::Atom("nil"), Raw::Integer(0), Raw::Other] {
        let rows = [
            ("history_hot_max_events", Raw::Integer(3)),
            ("history_hot_max_events", value),
        ];
        assert_eq!(
            normalize(&Raw::Keyword(&rows), 17, profile, &mut charge)?,
            Err(Reason::HistoryHot)
        );
    }
    assert_priority_neighbors(profile, &mut charge)?;
    Ok(())
}

fn assert_priority_neighbors(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (name, bad, reason) in [
        ("ttl_ms", 0, Reason::Ttl),
        ("history_max_events", 18, Reason::History { maximum: 17 }),
    ] {
        let cases = [
            (Raw::Integer(1), Raw::Nil, true),
            (Raw::Integer(1), Raw::Integer(bad), true),
            (Raw::Integer(bad), Raw::Integer(1), false),
        ]
        .into_iter()
        .flat_map(|case| [false, true].map(|reversed| (case, reversed)));
        for ((atom, binary, accepted), reversed) in cases {
            let mut rows = [
                (Key::Atom(name), atom),
                (Key::Binary(name.as_bytes()), binary),
            ];
            if reversed {
                rows.reverse();
            }
            let actual = normalize(&Raw::Map(&rows), 17, profile, charge)?;
            if accepted {
                assert_eq!(
                    actual,
                    Ok(Some(RetentionOverride {
                        ttl_ms: (name == "ttl_ms").then_some(1),
                        history_max_events: (name == "history_max_events").then_some(1),
                    }))
                );
            } else {
                assert_eq!(actual, Err(reason));
            }
        }
    }
    Ok(())
}

#[test]
fn retention_input_preserves_ttl_internal_history_and_captured_error_order() -> TestResult {
    let profile = limits(10_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    // Original bb1cff7d source140 cause, without a substituted nil hot value.
    let original = [
        ("ttl_ms", Raw::Integer(5_000)),
        ("history_hot_max_events", Raw::Integer(3)),
    ];
    let result = normalize(&Raw::Keyword(&original), 1_000_000, profile, &mut charge)?;
    assert_eq!(result, Err(Reason::HistoryHot));
    assert_eq!(
        encode(result.err().ok_or("source140")?, profile, &mut charge)?,
        b"ERR flow retention history_hot_max_events is internal"
    );
    for (ttl, has_hot, expected) in [
        (0, true, Reason::Ttl),
        (1, true, Reason::HistoryHot),
        (1, false, Reason::History { maximum: 17 }),
    ] {
        let mut rows = vec![
            ("ttl_ms", Raw::Integer(ttl)),
            ("history_max_events", Raw::Integer(0)),
        ];
        if has_hot {
            rows.push(("history_hot_max_events", Raw::Nil));
        }
        assert_eq!(
            normalize(&Raw::Keyword(&rows), 17, profile, &mut charge)?,
            Err(expected)
        );
    }
    for input in [
        Raw::Other,
        Raw::Integer(0),
        Raw::Binary(b"nil"),
        Raw::Atom("false"),
    ] {
        let reason = normalize(&input, 17, profile, &mut charge)?
            .err()
            .ok_or("shape")?;
        assert_eq!(reason, Reason::Shape);
        assert_eq!(
            encode(reason, profile, &mut charge)?,
            b"ERR flow retention policy must be a map or keyword list"
        );
    }
    assert_numeric_boundaries(profile, &mut charge)?;
    Ok(())
}

fn assert_numeric_boundaries(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (name, maximum, reason, body) in [
        (
            "ttl_ms",
            31_536_000_000,
            Reason::Ttl,
            b"ERR flow retention ttl_ms must be between 1 and 31536000000".as_slice(),
        ),
        (
            "history_max_events",
            17,
            Reason::History { maximum: 17 },
            b"ERR flow retention history_max_events must be between 1 and 17",
        ),
    ] {
        for value in [
            Raw::Nil,
            Raw::Atom("nil"),
            Raw::Integer(0),
            Raw::Integer(-1),
            Raw::Integer(maximum + 1),
            Raw::Integer(i128::MIN),
            Raw::Integer(i128::MAX),
            Raw::Binary(b"1"),
            Raw::Atom("1"),
            Raw::Map(&[]),
            Raw::Keyword(&[]),
            Raw::Other,
        ] {
            let rows = [(Key::Atom(name), value)];
            let actual = normalize(&Raw::Map(&rows), 17, profile, charge)?;
            assert_eq!(actual, Err(reason));
            assert_eq!(
                encode(actual.err().ok_or("numeric")?, profile, charge)?,
                body
            );
        }
        for value in [1, maximum] {
            let rows = [(Key::Atom(name), Raw::Integer(value))];
            let expected = RetentionOverride {
                ttl_ms: if name == "ttl_ms" {
                    Some(u64::try_from(value)?)
                } else {
                    None
                },
                history_max_events: if name == "history_max_events" {
                    Some(u32::try_from(value)?)
                } else {
                    None
                },
            };
            assert_eq!(
                normalized(&Raw::Map(&rows), 17, profile, charge)?,
                Some(expected)
            );
        }
    }
    Ok(())
}

#[test]
fn retention_input_keeps_captured_limits_and_body_owners() -> TestResult {
    let profile = limits(10_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let rows = [(Key::Atom("history_max_events"), Raw::Integer(50))];
    assert_eq!(
        normalized(&Raw::Map(&rows), 100, profile, &mut charge)?,
        Some(RetentionOverride {
            ttl_ms: None,
            history_max_events: Some(50)
        })
    );
    let mut captured = 10;
    let reason = {
        let key = String::from("history_max_events");
        let owned_rows = [(Key::Atom(&key), Raw::Integer(50))];
        let result = normalize(&Raw::Map(&owned_rows), captured, profile, &mut charge)?;
        assert_eq!(result, Err(Reason::History { maximum: 10 }));
        result.err().ok_or("captured source684")?
    };
    captured = 100;
    assert_eq!(
        normalized(&Raw::Map(&rows), captured, profile, &mut charge)?,
        Some(RetentionOverride {
            ttl_ms: None,
            history_max_events: Some(50)
        })
    );
    assert_eq!(
        encode(reason, profile, &mut charge)?,
        b"ERR flow retention history_max_events must be between 1 and 10"
    );
    for (maximum, expected, fee) in [
        (
            1,
            b"ERR flow retention history_max_events must be between 1 and 1".as_slice(),
            318,
        ),
        (
            17,
            b"ERR flow retention history_max_events must be between 1 and 17",
            320,
        ),
        (
            1_000_000,
            b"ERR flow retention history_max_events must be between 1 and 1000000",
            330,
        ),
    ] {
        let rows = [(
            Key::Binary(b"history_max_events"),
            Raw::Integer(i128::from(maximum) + 1),
        )];
        let result = normalize(&Raw::Map(&rows), maximum, profile, &mut charge)?;
        assert_eq!(result, Err(Reason::History { maximum }));
        let before = (charge.g(), charge.p());
        assert_eq!(
            encode(result.err().ok_or("history")?, profile, &mut charge)?,
            expected
        );
        assert_eq!((charge.g(), charge.p()), (before.0 + fee, before.1));
        let accepted = [(
            Key::Binary(b"history_max_events"),
            Raw::Integer(i128::from(maximum)),
        )];
        assert_eq!(
            normalized(&Raw::Map(&accepted), maximum, profile, &mut charge)?,
            Some(RetentionOverride {
                ttl_ms: None,
                history_max_events: Some(maximum)
            })
        );
    }
    Ok(())
}
