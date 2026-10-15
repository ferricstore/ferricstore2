//! Prospective source-independent debit streams, caps and preserved neighbors.

use super::{
    Error, Key, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, Raw, Reason,
    RetryOverride, TestResult, defaults, full, limits, normalize, normalize_override, over,
};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    OriginalPolicyMapValue, RawPolicyIndexedValue, normalize_complete_policy_indexed_input,
    original_complete_policy_uses_max_attempts, restore_complete_policy,
};

#[test]
fn retry_input_preserves_descriptor_bounds_and_ignored_subtrees() -> TestResult {
    assert_depth(&[], 64, 64)?;
    assert_depth(&[], 65, 65)?;
    let profile = limits(20_000_000)?;
    for length in [262_144, 262_145] {
        let bytes = vec![0; length];
        let mut charge = PolicySchemaCharge::new(profile);
        let result = normalize_override(&Raw::Binary(&bytes), profile, &mut charge);
        if length == 262_144 {
            assert_eq!(result?, Err(Reason::Shape));
            assert_eq!((charge.g(), charge.p()), (262_528, 0));
        } else {
            assert_eq!(result, Err(Error::ByteLimit));
            assert_eq!((charge.g(), charge.p()), (256, 0));
        }
        assert!(bytes.iter().all(|&byte| byte == 0));
    }
    let shared = [];
    for length in [2_047, 2_048] {
        let rows = vec![(Key::Other, Raw::Map(&shared)); length];
        let mut charge = PolicySchemaCharge::new(profile);
        let result = normalize_override(&Raw::Map(&rows), profile, &mut charge);
        if length == 2_047 {
            assert_eq!(result?, Ok(Some(RetryOverride::default())));
            assert_eq!((charge.g(), charge.p()), (1_573_120, 128));
        } else {
            assert_eq!(result, Err(Error::ItemLimit));
            assert_eq!((charge.g(), charge.p()), (524_544, 0));
        }
        assert_eq!(rows.len(), length);
    }
    assert_nonempty_aliases(profile)?;
    let too_large = vec![0; 262_145];
    let duplicate = [
        (Key::Atom("max_retries"), Raw::Integer(0)),
        (Key::Atom("max_retries"), Raw::Binary(&too_large)),
    ];
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize_override(&Raw::Map(&duplicate), profile, &mut charge),
        Err(Error::Malformed)
    );
    assert_eq!((charge.g(), charge.p()), (1_520, 0));
    let shadowed = [
        (Key::Atom("max_retries"), Raw::Integer(0)),
        (Key::Binary(b"max_retries"), Raw::Binary(&too_large)),
    ];
    assert_eq!(
        normalize_override(&Raw::Map(&shadowed), profile, &mut charge),
        Err(Error::ByteLimit)
    );
    Ok(())
}

fn assert_nonempty_aliases(profile: PolicySchemaLimits) -> TestResult {
    let child = [(Key::Other, Raw::Nil)];
    for tail in [1, 2] {
        let mut rows = vec![(Key::Other, Raw::Map(&child)); 1_023];
        rows.extend(std::iter::repeat_n((Key::Other, Raw::Nil), tail));
        let mut charge = PolicySchemaCharge::new(profile);
        let result = normalize_override(&Raw::Map(&rows), profile, &mut charge);
        if tail == 1 {
            assert_eq!(result?, Ok(Some(RetryOverride::default())));
            assert_eq!((charge.g(), charge.p()), (1_049_344, 128));
        } else {
            assert_eq!(result, Err(Error::ItemLimit));
            assert_eq!((charge.g(), charge.p()), (524_544, 0));
        }
        assert_eq!(rows.len(), 1_023 + tail);
    }
    Ok(())
}

fn assert_depth(rows: &[(Key<'_>, Raw<'_>)], remaining: usize, requested: usize) -> TestResult {
    if remaining != 0 {
        let parent = [(Key::Other, Raw::Map(rows))];
        return assert_depth(&parent, remaining - 1, requested);
    }
    let profile = limits(100_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let result = normalize_override(&Raw::Map(rows), profile, &mut charge);
    if requested == 64 {
        assert_eq!(result?, Ok(Some(RetryOverride::default())));
        assert_eq!((charge.g(), charge.p()), (17_920, 128));
    } else {
        assert_eq!(result, Err(Error::ItemLimit));
        assert_eq!((charge.g(), charge.p()), (16_768, 0));
    }
    Ok(())
}

#[test]
fn retry_input_preserves_each_prospective_debit_and_continuous_owner() -> TestResult {
    let nil = [(128, 0); 4];
    assert_stream(&Raw::Nil, false, &nil, None, (512, 0))?;
    let mut nil_full = nil.to_vec();
    nil_full.push((134, 0));
    assert_stream(&Raw::Nil, true, &nil_full, None, (646, 0))?;
    for (is_full, expected_g) in [(false, 1_024), (true, 2_566)] {
        let mut events = vec![(128, 0); 3];
        events.push((0, 128));
        events.extend(std::iter::repeat_n((128, 0), if is_full { 16 } else { 5 }));
        if is_full {
            events.push((134, 0));
        }
        for value in [Raw::Map(&[]), Raw::Keyword(&[])] {
            assert_stream(
                &value,
                is_full,
                &events,
                Some(&RetryOverride::default()),
                (expected_g, 128),
            )?;
        }
    }
    let backoff = [(Key::Atom("backoff"), Raw::Nil)];
    let mut events = [128, 128, 128, 7, 147, 146, 142, 128, 128, 128]
        .map(|g| (g, 0))
        .to_vec();
    events.push((0, 128));
    events.extend([128, 135, 147, 128, 135, 146, 128, 135, 142, 128].map(|g| (g, 0)));
    events.extend([(128, 0); 8]);
    events.extend([128, 135, 147, 128].map(|g| (g, 0)));
    assert_stream(
        &Raw::Map(&backoff),
        false,
        &events,
        Some(&RetryOverride {
            backoff: Some(defaults().backoff),
            ..RetryOverride::default()
        }),
        (4_124, 128),
    )?;
    let target = [(Key::Atom("exhausted_to"), Raw::Binary(b"done"))];
    assert_stream(
        &Raw::Map(&target),
        false,
        &target_events(),
        Some(&RetryOverride {
            exhausted_to: Some(b"done".to_vec()),
            ..RetryOverride::default()
        }),
        (3_459, 128),
    )?;
    let maximum = [(Key::Atom("max_retries"), Raw::Integer(0))];
    let mut events = [128, 128, 128, 11, 151, 150, 128, 128, 128]
        .map(|g| (g, 0))
        .to_vec();
    events.push((0, 128));
    events.extend(
        [
            128, 139, 151, 128, 139, 150, 128, 128, 139, 146, 128, 139, 151, 128,
        ]
        .map(|g| (g, 0)),
    );
    assert_stream(
        &Raw::Map(&maximum),
        false,
        &events,
        Some(&RetryOverride {
            max_retries: Some(0),
            ..RetryOverride::default()
        }),
        (3_002, 128),
    )?;
    assert_repeats_and_seeds()?;
    assert_held_targets()?;
    assert_nil_spelling()?;
    super::assert_kind_work()?;
    Ok(())
}

fn target_events() -> Vec<(u64, u64)> {
    let mut events = [128, 128, 128, 12, 152, 151, 147, 152, 128, 128, 4, 128]
        .map(|g| (g, 0))
        .to_vec();
    events.push((0, 128));
    events.extend(
        [
            128, 140, 152, 128, 140, 151, 128, 140, 147, 128, 140, 152, 128, 11, 128, 132,
        ]
        .map(|g| (g, 0)),
    );
    assert_eq!(events.iter().map(|event| event.0).sum::<u64>(), 3_459);
    events
}

fn assert_stream(
    value: &Raw<'_>,
    is_full: bool,
    events: &[(u64, u64)],
    expected: Option<&RetryOverride>,
    final_account: (u64, u64),
) -> TestResult {
    let (mut g, mut p) = (0, 0);
    for &(fee, scratch) in events {
        let next_p = p.max(scratch);
        let target = g + fee + next_p;
        if target > g + p {
            let maximum = target - 1;
            let profile = limits(maximum)?;
            let mut charge = PolicySchemaCharge::new(profile);
            let error = if is_full {
                normalize(value, profile, &mut charge).err()
            } else {
                normalize_override(value, profile, &mut charge).err()
            };
            assert_eq!(error, Some(work(target, maximum)));
            assert_eq!((charge.g(), charge.p()), (g, p));
        }
        (g, p) = (g + fee, next_p);
    }
    assert_eq!((g, p), final_account);
    let profile = limits(g + p)?;
    let mut charge = PolicySchemaCharge::new(profile);
    if is_full {
        assert_eq!(full(value, profile, &mut charge)?, defaults());
    } else {
        assert_eq!(over(value, profile, &mut charge)?.as_ref(), expected);
    }
    assert_eq!((charge.g(), charge.p(), charge.total()), (g, p, g + p));
    Ok(())
}

pub(super) fn work(bytes: u64, maximum: u64) -> Error {
    Error::Schema(PolicySchemaError::WorkBytes { bytes, maximum })
}

fn assert_repeats_and_seeds() -> TestResult {
    for (maximum, attempts, seed, retained, peak) in [
        (2_176, 2, false, 2_048, 128),
        (2_175, 2, false, 1_920, 128),
        (3_080, 1, true, 2_952, 128),
        (3_079, 1, true, 2_824, 128),
    ] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let held = if seed {
            let mut data = super::super::data(b"r".to_vec(), None, BTreeMap::new());
            data.indexed_attributes = Some(vec![b"x".to_vec()]);
            let owner = restore_complete_policy(1, data, profile, &mut charge)?;
            assert_eq!((charge.g(), charge.p()), (1_928, 64));
            Some(owner)
        } else {
            None
        };
        for attempt in 0..attempts {
            let result = normalize_override(&Raw::Map(&[]), profile, &mut charge);
            if attempt + 1 == attempts && maximum.is_multiple_of(2) {
                assert_eq!(result?, Ok(Some(RetryOverride::default())));
            } else if attempt + 1 == attempts {
                assert_eq!(result, Err(work(maximum + 1, maximum)));
            } else {
                assert_eq!(result?, Ok(Some(RetryOverride::default())));
            }
        }
        assert_eq!((charge.g(), charge.p()), (retained, peak));
        if let Some(owner) = held {
            assert_eq!(owner.data().indexed_attributes, Some(vec![b"x".to_vec()]));
        }
    }
    assert_larger_peak()?;
    Ok(())
}

fn assert_larger_peak() -> TestResult {
    for maximum in [5_999, 6_000] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let names = [
            RawPolicyIndexedValue::Binary(b"tenant"),
            RawPolicyIndexedValue::Atom("region"),
            RawPolicyIndexedValue::Binary(b"campaign"),
            RawPolicyIndexedValue::Binary(b"tenant"),
        ];
        let owner = normalize_complete_policy_indexed_input(
            Some(RawPolicyIndexedValue::List(&names)),
            None,
            profile,
            &mut charge,
        )?
        .map_err(|_| "seed")?;
        assert_eq!((charge.g(), charge.p()), (4_784, 192));
        let result = normalize_override(&Raw::Keyword(&[]), profile, &mut charge);
        if maximum == 6_000 {
            assert_eq!(result?, Ok(Some(RetryOverride::default())));
        } else {
            assert_eq!(result, Err(work(6_000, maximum)));
        }
        assert_eq!(
            (charge.g(), charge.p()),
            (if maximum == 6_000 { 5_808 } else { 5_680 }, 192)
        );
        assert_eq!(
            owner,
            (
                Some(vec![
                    b"tenant".to_vec(),
                    b"region".to_vec(),
                    b"campaign".to_vec()
                ]),
                None
            )
        );
    }
    Ok(())
}

fn assert_held_targets() -> TestResult {
    let rows = [(Key::Atom("exhausted_to"), Raw::Binary(b"done"))];
    for maximum in [7_045, 7_046] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let first = over(&Raw::Map(&rows), profile, &mut charge)?.ok_or("first")?;
        let pointer = first.exhausted_to.as_ref().ok_or("first target")?.as_ptr();
        let second = normalize_override(&Raw::Map(&rows), profile, &mut charge);
        if maximum == 7_046 {
            assert_eq!(
                second?,
                Ok(Some(RetryOverride {
                    exhausted_to: Some(b"done".to_vec()),
                    ..RetryOverride::default()
                }))
            );
        } else {
            assert_eq!(second, Err(work(7_046, maximum)));
        }
        assert_eq!(
            (charge.g(), charge.p()),
            (if maximum == 7_046 { 6_918 } else { 6_786 }, 128)
        );
        assert_eq!(first.exhausted_to.as_deref(), Some(b"done".as_slice()));
        assert_eq!(first.exhausted_to.as_ref().ok_or("held")?.as_ptr(), pointer);
    }
    let profile = limits(10_504)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let first = over(&Raw::Map(&rows), profile, &mut charge)?.ok_or("first")?;
    let pointer = first.exhausted_to.as_ref().ok_or("first target")?.as_ptr();
    assert_eq!((charge.g(), charge.p()), (3_459, 128));
    let second = over(&Raw::Map(&rows), profile, &mut charge)?.ok_or("second")?;
    assert_eq!((charge.g(), charge.p()), (6_918, 128));
    assert_eq!(first.exhausted_to.as_deref(), Some(b"done".as_slice()));
    assert_eq!(first.exhausted_to.as_ref().ok_or("held")?.as_ptr(), pointer);
    assert_eq!(
        normalize_override(&Raw::Map(&rows), profile, &mut charge),
        Err(work(10_505, 10_504))
    );
    assert_eq!((charge.g(), charge.p()), (10_245, 128));
    assert_eq!(first.exhausted_to.as_deref(), Some(b"done".as_slice()));
    assert_eq!(first.exhausted_to.as_ref().ok_or("held")?.as_ptr(), pointer);
    assert_eq!(second.exhausted_to.as_deref(), Some(b"done".as_slice()));
    Ok(())
}

fn assert_nil_spelling() -> TestResult {
    let profile = limits(100_000)?;
    for (is_full, expected) in [(false, 515), (true, 649)] {
        let mut charge = PolicySchemaCharge::new(profile);
        if is_full {
            assert_eq!(full(&Raw::Atom("nil"), profile, &mut charge)?, defaults());
        } else {
            assert_eq!(over(&Raw::Atom("nil"), profile, &mut charge)?, None);
        }
        assert_eq!((charge.g(), charge.p()), (expected, 0));
    }
    Ok(())
}

#[test]
fn retry_input_keeps_existing_contracts_and_caller_errors_separate() -> TestResult {
    let profile = limits(100_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(&Raw::Nil, limits(100_001)?, &mut charge),
        Err(Error::Schema(PolicySchemaError::ProfileMismatch))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    assert_eq!(
        normalize_override(&Raw::Nil, limits(100_001)?, &mut charge),
        Err(Error::Schema(PolicySchemaError::ProfileMismatch))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    let old = [(Key::Atom("max_attempts"), OriginalPolicyMapValue::Other)];
    let retry = [(Key::Atom("retry"), OriginalPolicyMapValue::Map(&old))];
    assert!(original_complete_policy_uses_max_attempts(
        &retry,
        profile,
        &mut charge
    )?);
    let caller = [(Key::Atom("max_attempts"), Raw::Nil)];
    assert_eq!(
        normalize(&Raw::Map(&caller), profile, &mut charge)?,
        Err(Reason::DeprecatedMaxAttempts)
    );
    let data = super::super::data(b"r".to_vec(), None, BTreeMap::new());
    let owner = restore_complete_policy(1, data, profile, &mut charge)?;
    assert_eq!(owner.generation(), 1);
    assert_eq!(owner.data().workflow_type, b"r");
    Ok(())
}

pub(super) fn assert_source_order(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (base, maximum, jitter, target, expected) in [
        (-1, -1, 101, Raw::Nil, Reason::BaseMillis),
        (0, -1, 101, Raw::Nil, Reason::MaxMillis),
        (0, 0, 101, Raw::Nil, Reason::JitterPercent),
        (0, 0, 0, Raw::Nil, Reason::ExhaustedTo),
    ] {
        let backoff = [
            ("kind", Raw::Atom("none")),
            ("base_ms", Raw::Integer(base)),
            ("max_ms", Raw::Integer(maximum)),
            ("jitter_pct", Raw::Integer(jitter)),
        ];
        let rows = [
            ("backoff", Raw::Keyword(&backoff)),
            ("exhausted_to", target),
        ];
        assert_eq!(
            normalize(&Raw::Keyword(&rows), profile, charge)?,
            Err(expected)
        );
    }
    Ok(())
}

pub(super) fn assert_kind_spellings(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (name, expected) in [
        ("none", super::BackoffKind::None),
        ("fixed", super::BackoffKind::Fixed),
        ("linear", super::BackoffKind::Linear),
        ("exponential", super::BackoffKind::Exponential),
    ] {
        for value in [Raw::Atom(name), Raw::Binary(name.as_bytes())] {
            let kind = [("kind", value)];
            let rows = [("backoff", Raw::Keyword(&kind))];
            assert_eq!(
                full(&Raw::Keyword(&rows), profile, charge)?.backoff.kind,
                expected
            );
        }
    }
    for value in [
        Raw::Nil,
        Raw::Atom("nil"),
        Raw::Atom(""),
        Raw::Binary(b""),
        Raw::Atom("FIXED"),
        Raw::Binary(b"linear "),
        Raw::Binary(b" exponential"),
        Raw::Binary(&[255]),
    ] {
        let kind = [("kind", value)];
        let rows = [("backoff", Raw::Keyword(&kind))];
        assert_eq!(
            normalize(&Raw::Keyword(&rows), profile, charge)?,
            Err(Reason::BackoffKind)
        );
    }
    Ok(())
}

pub(super) fn assert_shadowed_fields(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for (name, bytes, bad, good, nested, reason) in [
        (
            "backoff",
            b"backoff".as_slice(),
            Raw::Other,
            Raw::Map(&[]),
            false,
            Reason::BackoffShape,
        ),
        (
            "exhausted_to",
            b"exhausted_to",
            Raw::Nil,
            Raw::Binary(b"done"),
            false,
            Reason::ExhaustedTo,
        ),
        (
            "base_ms",
            b"base_ms",
            Raw::Nil,
            Raw::Integer(0),
            true,
            Reason::BaseMillis,
        ),
        (
            "max_ms",
            b"max_ms",
            Raw::Nil,
            Raw::Integer(0),
            true,
            Reason::MaxMillis,
        ),
        (
            "jitter_pct",
            b"jitter_pct",
            Raw::Nil,
            Raw::Integer(0),
            true,
            Reason::JitterPercent,
        ),
    ] {
        for reversed in [false, true] {
            let mut fields = [(Key::Atom(name), bad), (Key::Binary(bytes), good)];
            if reversed {
                fields.reverse();
            }
            let root = [(Key::Atom("backoff"), Raw::Map(&fields))];
            let input = if nested {
                Raw::Map(&root)
            } else {
                Raw::Map(&fields)
            };
            assert_eq!(normalize_override(&input, profile, charge)?, Err(reason));
        }
    }
    let backoff = [
        (Key::Atom("backoff"), Raw::Nil),
        (Key::Binary(b"backoff"), Raw::Other),
    ];
    assert_eq!(
        over(&Raw::Map(&backoff), profile, charge)?
            .ok_or("masked")?
            .backoff,
        Some(defaults().backoff)
    );
    Ok(())
}
