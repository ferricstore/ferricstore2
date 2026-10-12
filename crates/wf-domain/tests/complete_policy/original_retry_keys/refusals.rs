//! Fixed descriptor/tariff boundaries; no raw ETF or source-decoder linkage.

use std::collections::BTreeMap;

use wf_domain::policy::complete::{RawPolicyIndexedValue, normalize_complete_policy_indexed_input};

use super::super::{candidate, data};
use super::{Error, Key, PolicySchemaCharge, PolicySchemaError, TestResult, Value};
use super::{limits, old_retry, original_complete_policy_uses_max_attempts};

#[test]
fn original_retry_keys_refuses_descriptor_bounds_before_predicate_visits() -> TestResult {
    let profile = limits(20_000_000)?;
    for entries in [2_047, 2_048] {
        let policy = vec![(Key::Other, Value::Other); entries];
        let mut charge = PolicySchemaCharge::new(profile);
        let result = original_complete_policy_uses_max_attempts(&policy, profile, &mut charge);
        if entries == 2_047 {
            assert!(!result?);
            assert_eq!(
                (charge.g(), charge.p(), charge.total()),
                (1_048_576, 128, 1_048_704)
            );
        } else {
            assert_eq!(result, Err(Error::ItemLimit));
            assert_eq!((charge.g(), charge.p()), (524_544, 0));
        }
        assert_eq!(policy.len(), entries);
    }
    for length in [262_144, 262_145] {
        let text = vec![b'x'; length];
        let policy = [(Key::Binary(&text), Value::Other)];
        let mut charge = PolicySchemaCharge::new(profile);
        let result = original_complete_policy_uses_max_attempts(&policy, profile, &mut charge);
        if length == 262_144 {
            assert!(!result?);
        } else {
            assert_eq!(result, Err(Error::ByteLimit));
            assert_eq!((charge.g(), charge.p()), (384, 0));
        }
        assert!(text.iter().all(|&byte| byte == b'x'));
    }
    assert_depth(&[], 64, 64)?;
    assert_depth(&[], 65, 65)?;
    assert_aliased_map_occurrences()?;
    assert_profile_and_duplicate()?;
    assert_empty_events()?;
    assert_old_events()?;
    Ok(())
}

fn assert_depth(map: &[(Key<'_>, Value<'_>)], remaining: usize, requested: usize) -> TestResult {
    if remaining != 0 {
        let parent = [(Key::Other, Value::Map(map))];
        return assert_depth(&parent, remaining - 1, requested);
    }
    let profile = limits(100_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let result = original_complete_policy_uses_max_attempts(map, profile, &mut charge);
    if requested == 64 {
        assert!(!result?);
        //129 descriptor nodes, no text/classification, two root key visits.
        assert_eq!((charge.g(), charge.p()), (17_152, 128));
    } else {
        assert_eq!(result, Err(Error::ItemLimit));
        assert_eq!((charge.g(), charge.p()), (16_768, 0));
    }
    Ok(())
}

fn assert_aliased_map_occurrences() -> TestResult {
    let leaf = [];
    for entries in [2_047, 2_048] {
        let policy = vec![(Key::Other, Value::Map(&leaf)); entries];
        let profile = limits(2_000_000)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let result = original_complete_policy_uses_max_attempts(&policy, profile, &mut charge);
        if entries == 2_047 {
            assert!(!result?);
            assert_eq!((charge.g(), charge.p()), (1_048_576, 128));
        } else {
            assert_eq!(result, Err(Error::ItemLimit));
            assert_eq!((charge.g(), charge.p()), (524_544, 0));
        }
        assert_eq!(leaf.len(), 0);
        assert_eq!(policy.len(), entries);
    }
    Ok(())
}

fn assert_profile_and_duplicate() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let other = limits(20_000_001)?;
    assert_eq!(
        original_complete_policy_uses_max_attempts(&[], other, &mut charge),
        Err(Error::Schema(PolicySchemaError::ProfileMismatch))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    let duplicate = [
        (Key::Atom("retry"), Value::Other),
        (Key::Atom("retry"), Value::Other),
    ];
    assert_eq!(
        original_complete_policy_uses_max_attempts(&duplicate, profile, &mut charge),
        Err(Error::Malformed)
    );
    assert_eq!((charge.g(), charge.p()), (926, 0));
    let old = old_retry();
    let policy = [
        (Key::Atom("retry"), Value::Map(&old)),
        (Key::Atom("states"), Value::Map(&duplicate)),
    ];
    assert_eq!(
        original_complete_policy_uses_max_attempts(&policy, profile, &mut charge),
        Err(Error::Malformed)
    );
    //Visible structural preflight precedes even an earlier true source condition.
    assert_eq!(charge.p(), 0);
    Ok(())
}

fn assert_empty_events() -> TestResult {
    let mut prior_g = 0;
    let mut prior_p = 0;
    for (fee, scratch) in [(128, 0), (128, 0), (0, 128), (128, 0), (128, 0)] {
        let next_p = prior_p.max(scratch);
        let target = prior_g + fee + next_p;
        let profile = limits(target - 1)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            original_complete_policy_uses_max_attempts(&[], profile, &mut charge),
            Err(Error::Schema(PolicySchemaError::WorkBytes {
                bytes: target,
                maximum: target - 1
            }))
        );
        assert_eq!((charge.g(), charge.p()), (prior_g, prior_p));
        prior_g += fee;
        prior_p = next_p;
    }
    assert_eq!((prior_g, prior_p), (512, 128));
    Ok(())
}

fn assert_old_events() -> TestResult {
    let old = old_retry();
    let policy = [(Key::Atom("retry"), Value::Map(&old))];
    let events = [
        (128, 0),
        (128, 0),
        (128, 0),
        (5, 0),
        (138, 0),
        (128, 0),
        (128, 0),
        (12, 0),
        (145, 0),
        (146, 0),
        (152, 0),
        (128, 0),
        (0, 128),
        (128, 0),
        (133, 0),
        (138, 0),
        (128, 0),
        (128, 0),
        (140, 0),
        (152, 0),
    ];
    let mut prior_g = 0;
    let mut prior_p = 0;
    for (fee, scratch) in events {
        let next_p = prior_p.max(scratch);
        let target = prior_g + fee + next_p;
        let profile = limits(target - 1)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            original_complete_policy_uses_max_attempts(&policy, profile, &mut charge),
            Err(Error::Schema(PolicySchemaError::WorkBytes {
                bytes: target,
                maximum: target - 1
            }))
        );
        assert_eq!((charge.g(), charge.p()), (prior_g, prior_p));
        prior_g += fee;
        prior_p = next_p;
    }
    assert_eq!((prior_g, prior_p), (2_313, 128));
    Ok(())
}

#[test]
fn original_retry_keys_preserves_one_account_refusals_and_prior_peak() -> TestResult {
    for (maximum, attempts, retained) in [
        (640, 1, 512),
        (639, 1, 384),
        (1_152, 2, 1_024),
        (1_151, 2, 896),
    ] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        for attempt in 0..attempts {
            let target = if attempts == 1 { 640 } else { 1_152 };
            let result = original_complete_policy_uses_max_attempts(&[], profile, &mut charge);
            if attempt + 1 == attempts && maximum < target {
                assert_eq!(
                    result,
                    Err(Error::Schema(PolicySchemaError::WorkBytes {
                        bytes: target,
                        maximum
                    }))
                );
            } else {
                assert!(!result?);
            }
        }
        assert_eq!((charge.g(), charge.p()), (retained, 128));
    }
    for maximum in [2_440, 2_441] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let old = old_retry();
        let result = original_complete_policy_uses_max_attempts(
            &[(Key::Atom("retry"), Value::Map(&old))],
            profile,
            &mut charge,
        );
        if maximum == 2_441 {
            assert!(result?);
            assert_eq!((charge.g(), charge.p()), (2_313, 128));
        } else {
            assert_eq!(
                result,
                Err(Error::Schema(PolicySchemaError::WorkBytes {
                    bytes: 2_441,
                    maximum
                }))
            );
            assert_eq!((charge.g(), charge.p()), (2_161, 128));
        }
    }
    assert_prior64()?;
    assert_prior192()?;
    Ok(())
}

fn assert_prior64() -> TestResult {
    for maximum in [2_567, 2_568] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let mut policy = data(b"r".to_vec(), None, BTreeMap::new());
        policy.indexed_attributes = Some(vec![b"x".to_vec()]);
        let held = candidate(1, policy, profile, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (1_928, 64));
        let result = original_complete_policy_uses_max_attempts(&[], profile, &mut charge);
        if maximum == 2_568 {
            assert!(!result?);
            assert_eq!(
                (charge.g(), charge.p(), charge.total()),
                (2_440, 128, 2_568)
            );
        } else {
            assert_eq!(
                result,
                Err(Error::Schema(PolicySchemaError::WorkBytes {
                    bytes: 2_568,
                    maximum
                }))
            );
            assert_eq!((charge.g(), charge.p()), (2_312, 128));
        }
        assert_eq!(held.data().indexed_attributes, Some(vec![b"x".to_vec()]));
        assert_eq!(held.generation(), 1);
    }
    Ok(())
}

fn assert_prior192() -> TestResult {
    for maximum in [5_487, 5_488] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let input = [
            RawPolicyIndexedValue::Binary(b"tenant"),
            RawPolicyIndexedValue::Atom("region"),
            RawPolicyIndexedValue::Binary(b"campaign"),
            RawPolicyIndexedValue::Binary(b"tenant"),
        ];
        let held = normalize_complete_policy_indexed_input(
            Some(RawPolicyIndexedValue::List(&input)),
            None,
            profile,
            &mut charge,
        )??;
        let expected = Some(vec![
            b"tenant".to_vec(),
            b"region".to_vec(),
            b"campaign".to_vec(),
        ]);
        assert_eq!(held, (expected.clone(), None));
        assert_eq!((charge.g(), charge.p()), (4_784, 192));
        let result = original_complete_policy_uses_max_attempts(&[], profile, &mut charge);
        if maximum == 5_488 {
            assert!(!result?);
            assert_eq!(
                (charge.g(), charge.p(), charge.total()),
                (5_296, 192, 5_488)
            );
        } else {
            assert_eq!(
                result,
                Err(Error::Schema(PolicySchemaError::WorkBytes {
                    bytes: 5_488,
                    maximum
                }))
            );
            assert_eq!((charge.g(), charge.p()), (5_168, 192));
        }
        assert_eq!(held, (expected, None));
        assert_eq!(input.len(), 4);
    }
    Ok(())
}
