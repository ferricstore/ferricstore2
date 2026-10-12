//! Independent positive event streams and real held-owner account controls.

use std::collections::BTreeMap;

use wf_domain::policy::complete::{RawPolicyIndexedValue, normalize_complete_policy_indexed_input};

use super::super::{candidate, data};
use super::cases::{MIN, TAG, TYPE_T, binary, envelope, fun, hex, list, map, pid, work};
use super::{
    Error, OriginalEtfLocalFun, OriginalEtfSafety, PolicySchemaCharge, PolicySchemaError,
    TestResult, decode, limits,
};

#[test]
fn original_entry_preserves_each_reached_admission_and_owned_index_refusal() -> TestResult {
    assert_minimum_events()?;
    assert_k1_sum_reservations()?;
    assert_function_owner_events()?;
    assert_function_capture_and_reuse_events()?;
    assert_comparison_admission()?;
    assert_profile_and_operational_bounds()?;
    assert_repeats_and_held64()?;
    assert_held192()?;
    Ok(())
}

fn minimum_events() -> Vec<(u64, u64)> {
    // Version is read before the root attempted span; each later span precedes its tag.
    let lexical = [1, 128, 1, 1, 128, 1, 1, 14, 156, 128, 1, 1, 128, 1, 4];
    let mut events = vec![(128, 0), (1, 0), (1, 0), (128, 0), (142, 0)];
    events.extend(lexical.map(|fee| (fee, 0)));
    events.extend([(0, 256), (256, 0), (128, 0), (128, 0)]);
    events.extend(lexical.map(|fee| (fee, 0)));
    events.extend([
        (128, 0), // Map uniqueness.
        (128, 0),
        (156, 0),
        (128, 0),
        (128, 0), // Envelope shape.
        (128, 0),
        (128, 0),
        (0, 128),
        (128, 0),
        (128, 0), // Fresh predicate.
        (128, 0),
        (128, 0), // Atom-only type lookup and Entry metadata.
    ]);
    events
}

fn assert_minimum_events() -> TestResult {
    let bytes = hex(MIN)?;
    let safety = OriginalEtfSafety::new(&[TAG], &[], None, &[]);
    let (mut prior_g, mut prior_p) = (0, 0);
    for (fee, scratch) in minimum_events() {
        let next_p = prior_p.max(scratch);
        let target = prior_g + fee + next_p;
        if target > prior_g + prior_p {
            let profile = limits(target - 1)?;
            let mut charge = PolicySchemaCharge::new(profile);
            assert_eq!(
                decode(&bytes, &safety, profile, &mut charge).err(),
                Some(work(target, target - 1))
            );
            assert_eq!((charge.g(), charge.p()), (prior_g, prior_p));
        }
        prior_g += fee;
        prior_p = next_p;
    }
    assert_eq!((prior_g, prior_p, prior_g + prior_p), (3_736, 256, 3_992));
    let profile = limits(3_992)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let entry = decode(&bytes, &safety, profile, &mut charge)?.ok_or("minimum absent")?;
    assert_eq!(entry.encoded(), bytes);
    assert_eq!(entry.generation(), 0);
    assert_eq!(entry.policy_bytes(), &[116, 0, 0, 0, 0]);
    assert_eq!(entry.workflow_type(), None);
    assert_eq!((charge.g(), charge.p()), (3_736, 256));
    Ok(())
}

fn assert_k1_sum_reservations() -> TestResult {
    let bytes = hex(TYPE_T)?;
    let safety = OriginalEtfSafety::new(&[TAG, "type"], &[], None, &[]);
    // Pass1 G1776; simultaneous N6 + K1 + F0 scratch is448, then320/132/128 reserves.
    for (target, prior_g, prior_p) in [
        (2_224, 1_776, 0),
        (2_544, 1_776, 448),
        (2_676, 2_096, 448),
        (2_804, 2_228, 448),
        (7_226, 6_650, 448),
    ] {
        let profile = limits(target - 1)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            decode(&bytes, &safety, profile, &mut charge).err(),
            Some(work(target, target - 1))
        );
        assert_eq!((charge.g(), charge.p()), (prior_g, prior_p));
    }
    let profile = limits(7_226)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let entry = decode(&bytes, &safety, profile, &mut charge)?.ok_or("type map absent")?;
    assert_eq!(entry.encoded(), bytes);
    assert_eq!(entry.policy_bytes(), &bytes[21..]);
    assert_eq!(entry.workflow_type(), Some(&b"t"[..]));
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (6_778, 448, 7_226)
    );
    Ok(())
}

fn assert_function_owner_events() -> TestResult {
    let safety = OriginalEtfSafety::new(&[TAG, "m", "n"], &[], Some(("n", 0)), &[]);
    let original = fun((0, 0, 0, 7), [0; 16], &pid("n", 0, 0, 0)?, &[], 0)?;
    let policy = map(&[(binary(b"x")?, original)])?;
    let bytes = envelope(&policy, &[97, 0])?;
    assert_eq!(bytes.len(), 85);
    // Catalog661; each byte/node/safety pass2463; reserved first-insert256.
    // N11/K1/F1 coexist: P832, never max(704,64,64).
    for (target, prior_g, prior_p) in [
        (4_086, 3_254, 0),
        (4_566, 3_254, 832),
        (4_698, 3_734, 832),
        (4_890, 3_866, 832),
        (7_481, 6_521, 832),
        (7_609, 6_649, 832),
        (10_496, 9_536, 832),
    ] {
        let profile = limits(target - 1)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            decode(&bytes, &safety, profile, &mut charge).err(),
            Some(work(target, target - 1))
        );
        assert_eq!((charge.g(), charge.p()), (prior_g, prior_p));
    }
    let profile = limits(10_496)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let entry = decode(&bytes, &safety, profile, &mut charge)?.ok_or("function map absent")?;
    assert_eq!(entry.encoded(), bytes);
    assert_eq!(entry.policy_bytes(), policy);
    assert_eq!(entry.generation(), 0);
    assert_eq!(entry.workflow_type(), None);
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (9_664, 832, 10_496)
    );
    Ok(())
}

fn assert_profile_and_operational_bounds() -> TestResult {
    let safety = OriginalEtfSafety::new(&[TAG], &[], None, &[]);
    let bytes = hex(MIN)?;
    let profile = limits(100_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        decode(&bytes, &safety, limits(99_999_999)?, &mut charge).err(),
        Some(Error::Schema(PolicySchemaError::ProfileMismatch))
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    let duplicate_catalog = OriginalEtfSafety::new(&[TAG, TAG], &[], None, &[]);
    assert!(decode(&bytes, &duplicate_catalog, profile, &mut charge).is_err());
    for layers in [62, 63] {
        let mut nested = vec![106];
        for _ in 0..layers {
            nested = list(&[], &nested)?;
        }
        let bytes = envelope(&map(&[(binary(b"x")?, nested)])?, &[97, 0])?;
        let mut account = PolicySchemaCharge::new(profile);
        let result = decode(&bytes, &safety, profile, &mut account);
        if layers == 62 {
            assert!(result?.is_some());
        } else {
            assert_eq!(result.err(), Some(Error::ItemLimit));
            assert_eq!((account.g(), account.p()), (9_735, 0));
        }
    }
    for count in [2_046_u32, 2_047] {
        let rows = (0..count)
            .map(|index| Ok((binary(&index.to_be_bytes())?, vec![106])))
            .collect::<TestResult<Vec<_>>>()?;
        let bytes = envelope(&map(&rows)?, &[97, 0])?;
        let mut account = PolicySchemaCharge::new(profile);
        let result = decode(&bytes, &safety, profile, &mut account);
        if count == 2_046 {
            assert!(result?.is_some());
            assert_eq!(account.p(), 64 * (4_096 + 2_046));
        } else {
            assert_eq!(result.err(), Some(Error::ItemLimit));
            assert_eq!((account.g(), account.p()), (545_458, 0));
        }
        assert_eq!(rows.len(), usize::try_from(count)?);
    }
    Ok(())
}

fn assert_function_capture_and_reuse_events() -> TestResult {
    let creator = pid("n", 0, 0, 0)?;
    let first = fun((0, 0, 0, 7), [0; 16], &creator, &[], 0)?;
    let bytes = envelope(&map(&[(binary(b"x")?, first.clone())])?, &[97, 0])?;
    let captured = [OriginalEtfLocalFun::new("m", 0, 0, [0; 16], 17)];
    let safety = OriginalEtfSafety::new(&[TAG, "m", "n"], &[], Some(("n", 0)), &captured);
    // Catalog row154; normalization root128 + matched captured lookup172, no insertion.
    let profile = limits(7_806)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        decode(&bytes, &safety, profile, &mut charge).err(),
        Some(work(7_807, 7_806))
    );
    assert_eq!((charge.g(), charge.p()), (6_803, 832));
    let profile = limits(10_694)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let entry = decode(&bytes, &safety, profile, &mut charge)?.ok_or("captured fun absent")?;
    assert_eq!(entry.encoded(), bytes);
    assert_eq!(entry.workflow_type(), None);
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (9_862, 832, 10_694)
    );
    let second = fun((0, 0, 0, 9), [0; 16], &creator, &[], 0)?;
    let policy = map(&[(binary(b"x")?, first), (binary(b"y")?, second)])?;
    let bytes = envelope(&policy, &[97, 0])?;
    assert_eq!(bytes.len(), 144);
    let safety = OriginalEtfSafety::new(&[TAG, "m", "n"], &[], Some(("n", 0)), &[]);
    // N18/K2/F2: lex4232 twice, reserve1096, first insert then later virtual reuse.
    for (target, retained) in [
        (10_118, 8_582),
        (10_246, 8_710),
        (12_143, 10_607),
        (12_315, 10_735),
    ] {
        let profile = limits(target - 1)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            decode(&bytes, &safety, profile, &mut charge).err(),
            Some(work(target, target - 1))
        );
        assert_eq!((charge.g(), charge.p()), (retained, 1_408));
    }
    let profile = limits(1_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let entry = decode(&bytes, &safety, profile, &mut charge)?.ok_or("reused fun absent")?;
    assert_eq!(entry.encoded(), bytes);
    assert_eq!(entry.policy_bytes(), policy);
    assert_eq!(entry.generation(), 0);
    assert_eq!(entry.workflow_type(), None);
    Ok(())
}

fn assert_repeats_and_held64() -> TestResult {
    let bytes = hex(MIN)?;
    let safety = OriginalEtfSafety::new(&[TAG], &[], None, &[]);
    for (maximum, attempts, seeded, expected_g) in [
        (3_992, 1, false, 3_736),
        (3_991, 1, false, 3_608),
        (7_728, 2, false, 7_472),
        (7_727, 2, false, 7_344),
        (5_920, 1, true, 5_664),
        (5_919, 1, true, 5_536),
    ] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let held = if seeded {
            let mut policy = data(b"r".to_vec(), None, BTreeMap::new());
            policy.indexed_attributes = Some(vec![b"x".to_vec()]);
            let seed = candidate(1, policy, profile, &mut charge)?;
            assert_eq!((charge.g(), charge.p()), (1_928, 64));
            Some(seed)
        } else {
            None
        };
        let mut held_entries = Vec::new();
        for attempt in 0..attempts {
            let target = if seeded {
                5_920
            } else if attempts == 2 {
                7_728
            } else {
                3_992
            };
            let result = decode(&bytes, &safety, profile, &mut charge);
            if attempt + 1 == attempts && maximum < target {
                assert_eq!(result.err(), Some(work(target, maximum)));
            } else {
                held_entries.push(result?.ok_or("successful decode absent")?);
            }
        }
        assert_eq!((charge.g(), charge.p()), (expected_g, 256));
        for entry in held_entries {
            assert_eq!(entry.encoded(), bytes);
            assert_eq!(entry.policy_bytes(), &[116, 0, 0, 0, 0]);
            assert_eq!(entry.generation(), 0);
            assert_eq!(entry.workflow_type(), None);
        }
        if let Some(held) = held {
            assert_eq!(held.generation(), 1);
            assert_eq!(held.data().indexed_attributes, Some(vec![b"x".to_vec()]));
        }
    }
    Ok(())
}

fn assert_held192() -> TestResult {
    let profile = limits(8_776)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let leaves = [
        RawPolicyIndexedValue::Binary(b"tenant"),
        RawPolicyIndexedValue::Atom("region"),
        RawPolicyIndexedValue::Binary(b"campaign"),
        RawPolicyIndexedValue::Binary(b"tenant"),
    ];
    let held = normalize_complete_policy_indexed_input(
        Some(RawPolicyIndexedValue::List(&leaves)),
        None,
        profile,
        &mut charge,
    )??;
    let expected = (
        Some(vec![
            b"tenant".to_vec(),
            b"region".to_vec(),
            b"campaign".to_vec(),
        ]),
        None,
    );
    assert_eq!(held, expected);
    assert_eq!((charge.g(), charge.p()), (4_784, 192));
    let bytes = hex(MIN)?;
    let safety = OriginalEtfSafety::new(&[TAG], &[], None, &[]);
    let entry = decode(&bytes, &safety, profile, &mut charge)?.ok_or("seeded decode absent")?;
    assert_eq!(entry.encoded(), bytes);
    assert_eq!(entry.workflow_type(), None);
    assert_eq!(
        (charge.g(), charge.p(), charge.total()),
        (8_520, 256, 8_776)
    );
    assert_eq!(held, expected);
    assert_eq!(leaves.len(), 4);
    Ok(())
}

fn assert_comparison_admission() -> TestResult {
    let policy = map(&[(binary(b"x")?, vec![106]), (binary(b"y")?, vec![106])])?;
    let bytes = envelope(&policy, &[97, 0])?;
    assert_eq!(bytes.len(), 40);
    let safety = OriginalEtfSafety::new(&[TAG], &[], None, &[]);
    // N8/K2: each lex1220, reserves648, SUM P640; root/placements endG3872.
    // First binary comparison has independent canonical widths5/5 and costs138.
    let profile = limits(4_649)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        decode(&bytes, &safety, profile, &mut charge).err(),
        Some(work(4_650, 4_649))
    );
    assert_eq!((charge.g(), charge.p()), (3_872, 640));
    assert_eq!(&bytes[21..], policy);
    Ok(())
}
