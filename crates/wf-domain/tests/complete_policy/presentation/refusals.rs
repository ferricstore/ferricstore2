//! Independent prospective projection/current/refusal boundaries.

use super::{
    All, BTreeMap, Backoff, PolicySchemaCharge, ProfileMismatch, ResolvedRetention, Schema, State,
    StateMode, TestResult, WorkBytes, assert_presented_retry, candidate, data, limits,
    present_complete_policy, presentation_state, presentation_value,
};

#[test]
fn presentation_preserves_profile_authority_and_continuous_account_refusals() -> TestResult {
    let other = limits(999_999)?;
    let p = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(p);
    let bad = ResolvedRetention {
        ttl_ms: 0,
        history_max_events: 0,
    };
    let query = vec![0; 262_145];
    let result = present_complete_policy(b"", None, Some(&query), bad, other, &mut charge);
    assert!(matches!(result, Err(Schema(ProfileMismatch))));
    assert_eq!((charge.g(), charge.p()), (0, 0));
    let defaults = ResolvedRetention::default();
    // Independent absent events128,1,128,128; refusals retain their actual prior work.
    for query in [None, Some(&b""[..])] {
        for (maximum, target, retained) in [
            (127, 128, 0),
            (128, 129, 128),
            (256, 257, 129),
            (384, 385, 257),
        ] {
            let p = limits(maximum)?;
            let mut charge = PolicySchemaCharge::new(p);
            let result = present_complete_policy(b"r", None, query, defaults, p, &mut charge);
            assert!(
                matches!(result, Err(Schema(WorkBytes { bytes, maximum: actual })) if bytes == target && actual == maximum)
            );
            assert_eq!((charge.g(), charge.p()), (retained, 0));
        }
    }
    for (maximum, attempts, target, retained) in [
        (4_856, 1, 4_856, 4_856),
        (4_855, 1, 4_856, 4_728),
        (7_156, 2, 7_156, 7_156),
        (7_155, 2, 7_156, 7_028),
    ] {
        let p = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(p);
        let input = data(b"r".to_vec(), None, BTreeMap::new());
        let value = presentation_value(1, input, p, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (2_556, 0));
        let mut held = None;
        for attempt in 1..=attempts {
            let result =
                present_complete_policy(b"r", Some(&value), None, defaults, p, &mut charge);
            if attempt == attempts && maximum < target {
                assert!(
                    matches!(result, Err(Schema(WorkBytes { bytes, maximum: actual })) if bytes == target && actual == maximum)
                );
            } else {
                held = Some(result?);
            }
        }
        assert_eq!((charge.g(), charge.p()), (retained, 0));
        assert!(!matches!(&held, Some(State { .. })));
        if let Some(All {
            generation,
            states,
            retry,
            ..
        }) = held
        {
            assert_eq!(generation, 1);
            assert!(states.is_empty());
            assert_presented_retry(&retry, 3, Backoff::default(), b"failed");
        }
        assert_eq!(value.data().workflow_type, b"r");
        assert_eq!(
            value.encoded(),
            b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x16FWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\0\0\0\0"
        );
    }
    for (maximum, retained, success) in [(2_377, 2_313, true), (2_376, 2_185, false)] {
        let p = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(p);
        let mut input = data(b"r".to_vec(), None, BTreeMap::new());
        input.indexed_attributes = Some(vec![b"a".to_vec()]);
        let seed = candidate(1, input, p, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (1_928, 64));
        let result = present_complete_policy(b"r", None, None, defaults, p, &mut charge);
        if success {
            assert!(matches!(result?, All { generation: 0, states, .. } if states.is_empty()));
        } else {
            assert!(matches!(
                result,
                Err(Schema(WorkBytes {
                    bytes: 2_377,
                    maximum: 2_376
                }))
            ));
        }
        let observed = (charge.g(), charge.p(), charge.total());
        assert_eq!(observed, (retained, 64, retained + 64));
        assert_eq!(seed.data().indexed_attributes, Some(vec![b"a".to_vec()]));
    }
    Ok(())
}

#[test]
fn presentation_charges_each_declared_state_before_reserve_and_emission() -> TestResult {
    let p = limits(10_000_000)?;
    let mut preparation = PolicySchemaCharge::new(p);
    let defaults = ResolvedRetention::default();
    let mut input = data(b"r".to_vec(), None, BTreeMap::new());
    input
        .states
        .insert(b"s".to_vec(), presentation_state(None, None, None, None));
    let value = presentation_value(1, input, p, &mut preparation)?;
    let bytes = b"FWPV\0\x02\0\0\0\0\0\0\0\x01\0\0\0\x1fFWPS\0\x02\0\0\0\x01r\0\0\0\0\0\0\0\x01\0\0\0\x01s\0\0\0\0\0\0\0";
    assert_eq!(value.encoded(), bytes);
    assert_eq!((preparation.g(), preparation.p()), (3_869, 0));
    // Admission1029; measure817; compare866; prefix129+comparison129+common128.
    for (maximum, target, retained) in [
        (3_353, 3_354, 3_098),
        (3_482, 3_483, 3_354),
        (3_483, 3_483, 3_483),
    ] {
        let p = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(p);
        let result = present_complete_policy(b"r", Some(&value), None, defaults, p, &mut charge);
        if maximum == target {
            let All {
                generation, states, ..
            } = result?
            else {
                return Err("expected all-state response".into());
            };
            assert_eq!(generation, 1);
            let [row] = states.as_slice() else {
                return Err("expected s only".into());
            };
            assert_eq!(row.state, b"s");
            assert_eq!(row.mode, StateMode::Parallel);
            assert!(row.max_active_ms.is_none());
            assert_presented_retry(&row.retry, 3, Backoff::default(), b"failed");
            assert_eq!(row.retention, defaults);
            assert!(row.governance.is_none());
        } else {
            assert_eq!(
                result.err(),
                Some(Schema(WorkBytes {
                    bytes: target,
                    maximum
                }))
            );
        }
        assert_eq!((charge.g(), charge.p()), (retained, 0));
        assert_eq!(value.encoded(), bytes);
    }
    let input = data(b"r".to_vec(), None, BTreeMap::new());
    let value = presentation_value(1, input, p, &mut preparation)?;
    // Fifty independent source events: prefix/current, measure, compare, type/common/reserve.
    let mut prior = 0_u64;
    for fee in [
        128, 1, 128, 128, 1, 257, 128, 128, 4, 2, 8, 4, 128, 4, 2, 128, 4, 1, 1, 1, 1, 1, 4, 128,
        1, 1, 1, 128, 8, 4, 16, 8, 128, 8, 4, 128, 8, 2, 2, 2, 2, 2, 8, 128, 2, 2, 2, 129, 128,
        128,
    ] {
        let target = prior + fee;
        let maximum = target - 1;
        let p = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(p);
        let result = present_complete_policy(b"r", Some(&value), None, defaults, p, &mut charge);
        assert_eq!(
            result.err(),
            Some(Schema(WorkBytes {
                bytes: target,
                maximum
            }))
        );
        assert_eq!((charge.g(), charge.p()), (prior, 0));
        prior = target;
    }
    assert_eq!(prior, 2_300);
    Ok(())
}
