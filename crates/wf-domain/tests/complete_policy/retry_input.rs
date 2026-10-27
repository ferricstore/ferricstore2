//! Original retry input classes, fixed source errors and normalized owners.

use super::{PolicySchemaCharge, PolicySchemaLimits, TestResult, limits};
use wf_domain::policy::complete::{
    CompletePolicyEncodingError as Error, OriginalPolicyMapKey as Key,
    PolicyRetryInputRejection as Reason, PolicySchemaError, RawPolicyRetryValue as Raw,
    normalize_complete_policy_retry_input as normalize,
    normalize_complete_policy_retry_override_input as normalize_override,
};
use wf_domain::policy::{
    Backoff, BackoffKind, InstalledRetryPolicy, RetryOverride, RetryPolicy, resolve,
};
use wf_domain::retry::JitterSample;
use wf_types::version::TimestampMillis;

mod refusals;

fn full(
    value: &Raw<'_>,
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<RetryPolicy> {
    normalize(value, profile, charge)?
        .map_err(|reason| format!("unexpected full {reason:?}").into())
}

fn over(
    value: &Raw<'_>,
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<Option<RetryOverride>> {
    normalize_override(value, profile, charge)?
        .map_err(|reason| format!("unexpected override {reason:?}").into())
}

fn defaults() -> RetryPolicy {
    RetryPolicy {
        max_retries: 3,
        backoff: Backoff {
            kind: BackoffKind::Exponential,
            base_ms: 1_000,
            max_ms: 30_000,
            jitter_pct: 20,
        },
        exhausted_to: b"failed".to_vec(),
    }
}

fn backoff(kind: &str, base: i128, maximum: i128, jitter: i128) -> [(&str, Raw<'_>); 4] {
    [
        ("kind", Raw::Atom(kind)),
        ("base_ms", Raw::Integer(base)),
        ("max_ms", Raw::Integer(maximum)),
        ("jitter_pct", Raw::Integer(jitter)),
    ]
}

fn fields<'a>(
    backoff: &'a [(&'a str, Raw<'a>)],
    retries: i128,
    target: &'a [u8],
) -> [(&'a str, Raw<'a>); 3] {
    [
        ("max_retries", Raw::Integer(retries)),
        ("backoff", Raw::Keyword(backoff)),
        ("exhausted_to", Raw::Binary(target)),
    ]
}

#[test]
fn retry_input_preserves_full_defaults_and_override_presence() -> TestResult {
    let profile = limits(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    for value in [Raw::Nil, Raw::Atom("nil")] {
        assert_eq!(full(&value, profile, &mut charge)?, defaults());
        assert_eq!(over(&value, profile, &mut charge)?, None);
    }
    for value in [Raw::Map(&[]), Raw::Keyword(&[])] {
        assert_eq!(full(&value, profile, &mut charge)?, defaults());
        assert_eq!(
            over(&value, profile, &mut charge)?,
            Some(RetryOverride::default())
        );
    }
    for value in [Raw::Nil, Raw::Atom("nil"), Raw::Map(&[]), Raw::Keyword(&[])] {
        let rows = [(Key::Atom("backoff"), value)];
        assert_eq!(
            over(&Raw::Map(&rows), profile, &mut charge)?,
            Some(RetryOverride {
                max_retries: None,
                backoff: Some(defaults().backoff),
                exhausted_to: None,
            })
        );
        assert_eq!(full(&Raw::Map(&rows), profile, &mut charge)?, defaults());
    }
    let partial = [("kind", Raw::Binary(b"none")), ("base_ms", Raw::Integer(5))];
    let rows = [("backoff", Raw::Keyword(&partial))];
    assert_eq!(
        over(&Raw::Keyword(&rows), profile, &mut charge)?,
        Some(RetryOverride {
            max_retries: None,
            backoff: Some(Backoff {
                kind: BackoffKind::None,
                base_ms: 5,
                max_ms: 30_000,
                jitter_pct: 20
            }),
            exhausted_to: None,
        })
    );
    let owned = {
        let bytes = vec![0, 255, b' '];
        let rows = [(Key::Atom("exhausted_to"), Raw::Binary(&bytes))];
        let value = full(&Raw::Map(&rows), profile, &mut charge)?;
        assert_eq!(bytes, [0, 255, b' ']);
        assert_ne!(value.exhausted_to.as_ptr(), bytes.as_ptr());
        value
    };
    assert_eq!(owned.exhausted_to, [0, 255, b' ']);
    Ok(())
}

#[test]
fn retry_input_preserves_atom_precedence_and_keyword_last_values() -> TestResult {
    let profile = limits(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    for reversed in [false, true] {
        let mut rows = [
            (Key::Atom("max_retries"), Raw::Nil),
            (Key::Binary(b"max_retries"), Raw::Integer(0)),
        ];
        if reversed {
            rows.reverse();
        }
        assert_eq!(
            normalize_override(&Raw::Map(&rows), profile, &mut charge)?,
            Err(Reason::MaxRetries)
        );
        let mut kinds = [
            (Key::Atom("kind"), Raw::Other),
            (Key::Binary(b"kind"), Raw::Atom("fixed")),
        ];
        if reversed {
            kinds.reverse();
        }
        let retry = [(Key::Atom("backoff"), Raw::Map(&kinds))];
        assert_eq!(
            normalize(&Raw::Map(&retry), profile, &mut charge)?,
            Err(Reason::BackoffKind)
        );
    }
    for (first, last, accepted) in [(1_001, 0, true), (0, 1_001, false)] {
        let pairs = [
            ("max_retries", Raw::Integer(first)),
            ("max_retries", Raw::Integer(last)),
        ];
        let result = normalize_override(&Raw::Keyword(&pairs), profile, &mut charge)?;
        if accepted {
            assert_eq!(
                result,
                Ok(Some(RetryOverride {
                    max_retries: Some(0),
                    ..RetryOverride::default()
                }))
            );
        } else {
            assert_eq!(result, Err(Reason::MaxRetries));
        }
    }
    let pairs = [("kind", Raw::Other), ("kind", Raw::Atom("linear"))];
    let retry = [("backoff", Raw::Keyword(&pairs)), ("unknown", Raw::Other)];
    assert_eq!(
        over(&Raw::Keyword(&retry), profile, &mut charge)?
            .ok_or("present")?
            .backoff,
        Some(Backoff {
            kind: BackoffKind::Linear,
            ..defaults().backoff
        })
    );
    assert_each_key_class(&mut charge, profile)?;
    refusals::assert_shadowed_fields(profile, &mut charge)?;
    refusals::assert_kind_spellings(profile, &mut charge)?;
    Ok(())
}

fn assert_each_key_class(
    charge: &mut PolicySchemaCharge,
    profile: PolicySchemaLimits,
) -> TestResult {
    for binary in [false, true] {
        let key = |atom, bytes| {
            if binary {
                Key::Binary(bytes)
            } else {
                Key::Atom(atom)
            }
        };
        let backoff = [
            (key("kind", b"kind"), Raw::Binary(b"fixed")),
            (key("base_ms", b"base_ms"), Raw::Integer(200)),
            (key("max_ms", b"max_ms"), Raw::Integer(100)),
            (key("jitter_pct", b"jitter_pct"), Raw::Integer(0)),
        ];
        let rows = [
            (key("max_retries", b"max_retries"), Raw::Integer(7)),
            (key("backoff", b"backoff"), Raw::Map(&backoff)),
            (key("exhausted_to", b"exhausted_to"), Raw::Binary(b"done")),
        ];
        assert_eq!(
            full(&Raw::Map(&rows), profile, charge)?,
            RetryPolicy {
                max_retries: 7,
                backoff: Backoff {
                    kind: BackoffKind::Fixed,
                    base_ms: 200,
                    max_ms: 100,
                    jitter_pct: 0
                },
                exhausted_to: b"done".to_vec(),
            }
        );
        let old = [(key("max_attempts", b"max_attempts"), Raw::Nil)];
        assert_eq!(
            normalize(&Raw::Map(&old), profile, charge)?,
            Err(Reason::DeprecatedMaxAttempts)
        );
    }
    Ok(())
}

#[test]
fn retry_input_preserves_source_diagnostics_and_field_order() -> TestResult {
    let profile = limits(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let backoff = backoff("fixed", 100, 1_000, 0);
    let old = [
        ("max_attempts", Raw::Integer(3)),
        ("backoff", Raw::Keyword(&backoff)),
        ("exhausted_to", Raw::Binary(b"failed")),
    ];
    let result = normalize(&Raw::Keyword(&old), profile, &mut charge)?;
    assert_eq!(result, Err(Reason::DeprecatedMaxAttempts));
    assert_eq!(
        result.err().ok_or("source584")?.as_bytes(),
        b"ERR flow retry max_attempts was renamed to max_retries"
    );
    assert_diagnostics(profile, &mut charge)?;
    refusals::assert_source_order(profile, &mut charge)?;
    let bad_backoff = [
        ("kind", Raw::Other),
        ("base_ms", Raw::Integer(-1)),
        ("max_ms", Raw::Integer(-1)),
        ("jitter_pct", Raw::Integer(101)),
    ];
    for (old, maximum, expected) in [
        (true, 1_001, Reason::DeprecatedMaxAttempts),
        (false, 1_001, Reason::MaxRetries),
        (false, 0, Reason::BackoffKind),
    ] {
        let mut rows = vec![
            (Key::Atom("max_retries"), Raw::Integer(maximum)),
            (Key::Atom("backoff"), Raw::Keyword(&bad_backoff)),
            (Key::Atom("exhausted_to"), Raw::Nil),
        ];
        if old {
            rows.push((Key::Binary(b"max_attempts"), Raw::Nil));
        }
        assert_eq!(
            normalize_override(&Raw::Map(&rows), profile, &mut charge)?,
            Err(expected)
        );
    }
    for value in [
        Raw::Nil,
        Raw::Atom("nil"),
        Raw::Integer(-1),
        Raw::Integer(1_001),
        Raw::Integer(i128::MAX),
        Raw::Integer(i128::MIN),
        Raw::Binary(b"0"),
        Raw::Other,
    ] {
        let rows = [("max_retries", value)];
        assert_eq!(
            normalize(&Raw::Keyword(&rows), profile, &mut charge)?,
            Err(Reason::MaxRetries)
        );
    }
    Ok(())
}

fn assert_diagnostics(profile: PolicySchemaLimits, charge: &mut PolicySchemaCharge) -> TestResult {
    for (input, reason, body) in [
        (
            Raw::Other,
            Reason::Shape,
            b"ERR flow retry policy must be a map or keyword list".as_slice(),
        ),
        (
            Raw::Map(&[(Key::Atom("max_attempts"), Raw::Nil)]),
            Reason::DeprecatedMaxAttempts,
            b"ERR flow retry max_attempts was renamed to max_retries",
        ),
        (
            Raw::Map(&[(Key::Atom("max_retries"), Raw::Nil)]),
            Reason::MaxRetries,
            b"ERR flow retry max_retries must be between 0 and 1000",
        ),
        (
            Raw::Map(&[(Key::Atom("backoff"), Raw::Other)]),
            Reason::BackoffShape,
            b"ERR flow retry backoff must be a map or keyword list",
        ),
        (
            Raw::Map(&[(
                Key::Atom("backoff"),
                Raw::Map(&[(Key::Atom("kind"), Raw::Binary(b"FIXED"))]),
            )]),
            Reason::BackoffKind,
            b"ERR flow retry backoff kind must be none, fixed, linear, or exponential",
        ),
        (
            Raw::Map(&[(
                Key::Atom("backoff"),
                Raw::Map(&[(Key::Atom("base_ms"), Raw::Integer(2_592_000_001))]),
            )]),
            Reason::BaseMillis,
            b"ERR flow retry base_ms must be between 0 and 2592000000",
        ),
        (
            Raw::Map(&[(
                Key::Atom("backoff"),
                Raw::Map(&[(Key::Atom("max_ms"), Raw::Integer(-1))]),
            )]),
            Reason::MaxMillis,
            b"ERR flow retry max_ms must be between 0 and 2592000000",
        ),
        (
            Raw::Map(&[(
                Key::Atom("backoff"),
                Raw::Map(&[(Key::Atom("jitter_pct"), Raw::Integer(101))]),
            )]),
            Reason::JitterPercent,
            b"ERR flow retry jitter_pct must be between 0 and 100",
        ),
        (
            Raw::Map(&[(Key::Atom("exhausted_to"), Raw::Atom("failed"))]),
            Reason::ExhaustedTo,
            b"ERR flow retry exhausted_to must be a non-empty string",
        ),
        (
            Raw::Map(&[(Key::Atom("exhausted_to"), Raw::Binary(b"running"))]),
            Reason::RunningExhaustion,
            b"ERR flow retry exhausted_to cannot be running",
        ),
    ] {
        let result = normalize(&input, profile, charge)?;
        assert_eq!(result, Err(reason));
        assert_eq!(result.err().ok_or("diagnostic")?.as_bytes(), body);
        assert_eq!(normalize_override(&input, profile, charge)?, Err(reason));
    }
    for maximum in [0, 1_000] {
        let rows = [("max_retries", Raw::Integer(maximum))];
        assert_eq!(
            full(&Raw::Keyword(&rows), profile, charge)?.max_retries,
            u16::try_from(maximum)?
        );
    }
    for value in [0, 2_592_000_000] {
        let backoff = backoff("none", value, value, 100);
        assert!(
            over(
                &Raw::Keyword(&fields(&backoff, 0, b"done")),
                profile,
                charge
            )?
            .is_some()
        );
    }
    Ok(())
}

#[test]
fn retry_input_reconstructs_original_normalize_cases_before_timing() -> TestResult {
    let profile = limits(1_000_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let cap = backoff("exponential", 2_592_000_000, 2_592_000_000, 100);
    let policy = full(
        &Raw::Keyword(&fields(&cap, 1_000, b"needs_review")),
        profile,
        &mut charge,
    )?;
    assert_eq!(policy.backoff.base_ms, 2_592_000_000);
    assert_eq!(policy.backoff.max_ms, 2_592_000_000);
    for (kind, expected) in [
        ("fixed", [1_100, 1_100, 1_100]),
        ("linear", [1_100, 1_200, 1_300]),
        ("exponential", [1_100, 1_200, 1_250]),
    ] {
        let backoff = backoff(
            kind,
            100,
            if kind == "exponential" { 250 } else { 1_000 },
            0,
        );
        let policy = full(
            &Raw::Keyword(&fields(&backoff, 3, b"failed")),
            profile,
            &mut charge,
        )?;
        for (attempt, expected) in [1, 2, 3].into_iter().zip(expected) {
            assert_eq!(
                policy
                    .next_run_at(TimestampMillis::new(1_000), attempt, JitterSample::new(0))?
                    .get(),
                expected
            );
        }
    }
    let jittered = backoff("exponential", 100, 250, 100);
    let policy = full(
        &Raw::Keyword(&fields(&jittered, 3, b"failed")),
        profile,
        &mut charge,
    )?;
    let time = policy
        .next_run_at(TimestampMillis::new(1_000), 3, JitterSample::new(123_456))?
        .get();
    assert!((1_000..=1_250).contains(&time));
    let mut count = 0;
    let cases = ["none", "fixed", "linear", "exponential"]
        .into_iter()
        .flat_map(|kind| {
            [0, 1, 100, 60_000, 2_592_000_000]
                .into_iter()
                .flat_map(move |base| {
                    [0, 1, 50, 1_000, 2_592_000_000]
                        .into_iter()
                        .map(move |maximum| (kind, base, maximum))
                })
        });
    for (kind, base, maximum) in cases {
        for jitter in [0, 25, 100] {
            for attempt in [0, 1, 2, 3, 10, 1_000] {
                let raw = backoff(kind, base, maximum, jitter);
                let policy = full(
                    &Raw::Keyword(&fields(&raw, 1_000, b"failed")),
                    profile,
                    &mut charge,
                )?;
                let now = TimestampMillis::new(123_456);
                let sample = JitterSample::new(42_731_729);
                let first = policy.next_run_at(now, attempt, sample)?;
                let second = policy.next_run_at(now, attempt, sample)?;
                assert_eq!(first, second);
                assert!(first >= now);
                assert!(first.get() <= 123_456 + u64::try_from(maximum)?);
                count += 1;
            }
        }
    }
    assert_eq!(count, 1_800);
    let policy = full(&Raw::Nil, profile, &mut charge)?;
    assert!(policy.attempt_allowed(3));
    assert!(!policy.attempt_allowed(4));
    let zero = [
        ("max_retries", Raw::Integer(0)),
        ("exhausted_to", Raw::Binary(b"failed")),
    ];
    assert!(!full(&Raw::Keyword(&zero), profile, &mut charge)?.attempt_allowed(1));
    Ok(())
}

#[test]
fn retry_input_keeps_override_layering_defaults_and_opaque_targets() -> TestResult {
    let profile = limits(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let fixed = backoff("fixed", 10_000, 30_000, 0);
    let type_retry = over(
        &Raw::Keyword(&fields(&fixed, 5, b"failed")),
        profile,
        &mut charge,
    )?
    .ok_or("type")?;
    let raw_state = [
        ("max_retries", Raw::Integer(2)),
        ("exhausted_to", Raw::Binary(b"payment_failed")),
    ];
    let state_retry = over(&Raw::Keyword(&raw_state), profile, &mut charge)?.ok_or("state")?;
    let installed = InstalledRetryPolicy::new(
        b"payment".to_vec(),
        type_retry,
        std::collections::BTreeMap::from([(b"charge_card".to_vec(), state_retry)]),
    )?;
    let state = resolve(
        Some(&installed),
        b"payment",
        b"charge_card",
        &RetryOverride::default(),
    )?;
    assert_eq!(
        state,
        RetryPolicy {
            max_retries: 2,
            backoff: Backoff {
                kind: BackoffKind::Fixed,
                base_ms: 10_000,
                max_ms: 30_000,
                jitter_pct: 0
            },
            exhausted_to: b"payment_failed".to_vec()
        }
    );
    let command = backoff("fixed", 5_000, 5_000, 0);
    let command = over(
        &Raw::Keyword(&fields(&command, 3, b"needs_review")),
        profile,
        &mut charge,
    )?
    .ok_or("command")?;
    assert_eq!(
        resolve(Some(&installed), b"payment", b"charge_card", &command)?,
        RetryPolicy {
            max_retries: 3,
            backoff: Backoff {
                kind: BackoffKind::Fixed,
                base_ms: 5_000,
                max_ms: 5_000,
                jitter_pct: 0
            },
            exhausted_to: b"needs_review".to_vec()
        }
    );
    let reset = [("backoff", Raw::Nil)];
    let reset = over(&Raw::Keyword(&reset), profile, &mut charge)?.ok_or("present reset")?;
    assert_eq!(
        resolve(Some(&installed), b"payment", b"charge_card", &reset)?.backoff,
        defaults().backoff
    );
    for target in [b"running ".as_slice(), &[0, 255], b" "] {
        let rows = [("exhausted_to", Raw::Binary(target))];
        assert_eq!(
            over(&Raw::Keyword(&rows), profile, &mut charge)?
                .ok_or("opaque")?
                .exhausted_to
                .as_deref(),
            Some(target)
        );
    }
    Ok(())
}

fn assert_kind_work() -> TestResult {
    for (kind, maximum, expected_g) in [
        (Raw::Atom("fixed"), 6_480, 6_352),
        (Raw::Atom("fixed"), 4_358, 4_185),
        (Raw::Nil, 100_000, 4_180),
        (Raw::Atom("nil"), 100_000, 4_183),
    ] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let inner = [(Key::Atom("kind"), kind)];
        let rows = [(Key::Atom("backoff"), Raw::Map(&inner))];
        let result = normalize_override(&Raw::Map(&rows), profile, &mut charge);
        if maximum == 6_480 {
            assert_eq!(
                result?,
                Ok(Some(RetryOverride {
                    backoff: Some(Backoff {
                        kind: BackoffKind::Fixed,
                        ..defaults().backoff
                    }),
                    ..RetryOverride::default()
                }))
            );
        } else if maximum == 4_358 {
            assert_eq!(result, Err(refusals::work(4_359, maximum)));
        } else {
            assert_eq!(result?, Err(Reason::BackoffKind));
        }
        assert_eq!((charge.g(), charge.p()), (expected_g, 128));
    }
    Ok(())
}
