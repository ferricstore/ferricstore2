//! Literal prospective work, visible bounds and preserved existing owners.

use super::{
    Error, Key, PolicySchemaCharge, PolicySchemaError, Raw, Reason, RetentionOverride, TestResult,
    encode, limits, normalize,
};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    RawPolicyIndexedValue, normalize_complete_policy_indexed_input,
    normalize_complete_policy_retry_input, normalize_complete_policy_retry_override_input,
    restore_complete_policy,
};

#[test]
fn retention_input_preserves_descriptor_bounds_and_each_reached_work_refusal() -> TestResult {
    assert_literal_streams()?;
    assert_body_streams()?;
    assert_depth(&[], 64, 64)?;
    assert_depth(&[], 65, 65)?;
    assert_aliases()?;
    assert_bytes_and_duplicates()?;
    Ok(())
}

fn assert_literal_streams() -> TestResult {
    assert_stream(&Raw::Nil, 17, &[(128, 0); 4], Ok(None), (512, 0))?;
    assert_stream(
        &Raw::Atom("nil"),
        17,
        &[(128, 0), (128, 0), (3, 0), (128, 0), (128, 0)],
        Ok(None),
        (515, 0),
    )?;
    for input in [Raw::Map(&[]), Raw::Keyword(&[])] {
        assert_stream(
            &input,
            17,
            &scratch(&[128; 7], 3),
            Ok(Some(RetentionOverride::default())),
            (896, 64),
        )?;
    }
    let ttl = [(Key::Atom("ttl_ms"), Raw::Integer(1))];
    let fees = [
        128, 128, 128, 6, 140, 128, 128, 128, 128, 134, 140, 128, 128, 134, 156, 128, 134, 152, 128,
    ];
    assert_stream(
        &Raw::Map(&ttl),
        17,
        &scratch(&fees, 8),
        Ok(Some(RetentionOverride {
            ttl_ms: Some(1),
            history_max_events: None,
        })),
        (2_404, 64),
    )?;
    let hot = [(Key::Atom("history_hot_max_events"), Raw::Nil)];
    let fees = [
        128, 128, 128, 22, 156, 172, 128, 128, 128, 128, 150, 156, 128, 150, 172,
    ];
    assert_stream(
        &Raw::Map(&hot),
        17,
        &scratch(&fees, 9),
        Err(Reason::HistoryHot),
        (2_002, 64),
    )?;
    let history = [(Key::Atom("history_max_events"), Raw::Integer(50))];
    let fees = [
        128, 128, 128, 18, 152, 168, 164, 128, 128, 128, 128, 146, 152, 128, 146, 168, 128, 146,
        164, 128,
    ];
    assert_stream(
        &Raw::Map(&history),
        10,
        &scratch(&fees, 10),
        Err(Reason::History { maximum: 10 }),
        (2_704, 64),
    )?;
    let mut accepted = fees.to_vec();
    accepted.push(128);
    assert_stream(
        &Raw::Map(&history),
        100,
        &scratch(&accepted, 10),
        Ok(Some(RetentionOverride {
            ttl_ms: None,
            history_max_events: Some(50),
        })),
        (2_832, 64),
    )?;
    assert_pair_streams()?;
    Ok(())
}

fn assert_pair_streams() -> TestResult {
    let fees = [
        128, 128, 128, 6, 140, 128, 128, 128, 18, 152, 168, 164, 128, 128, 128, 128, 134, 140, 146,
        152, 128, 128, 134, 156, 146, 168, 128, 134, 152, 146, 164, 128, 128,
    ];
    for (ttl, history) in [(60_000, 512), (30_000, 256)] {
        let rows = [
            ("ttl_ms", Raw::Integer(ttl)),
            ("history_max_events", Raw::Integer(history)),
        ];
        assert_stream(
            &Raw::Keyword(&rows),
            1_000_000,
            &scratch(&fees, 15),
            Ok(Some(RetentionOverride {
                ttl_ms: Some(u64::try_from(ttl)?),
                history_max_events: Some(u32::try_from(history)?),
            })),
            (4_340, 64),
        )?;
    }
    let rows = [
        ("ttl_ms", Raw::Integer(5_000)),
        ("history_hot_max_events", Raw::Integer(3)),
    ];
    let fees = [
        128, 128, 128, 6, 140, 128, 128, 128, 22, 156, 172, 128, 128, 128, 128, 134, 140, 150, 156,
        128, 128, 134, 156, 150, 172,
    ];
    assert_stream(
        &Raw::Keyword(&rows),
        1_000_000,
        &scratch(&fees, 14),
        Err(Reason::HistoryHot),
        (3_224, 64),
    )?;
    Ok(())
}

fn scratch(fees: &[u64], after: usize) -> Vec<(u64, u64)> {
    let mut events = Vec::new();
    for (index, &fee) in fees.iter().enumerate() {
        if index == after {
            events.push((0, 64));
        }
        events.push((fee, 0));
    }
    events
}

fn assert_stream(
    input: &Raw<'_>,
    maximum: u32,
    events: &[(u64, u64)],
    expected: Result<Option<RetentionOverride>, Reason>,
    final_account: (u64, u64),
) -> TestResult {
    let (mut g, mut p) = (0, 0);
    for &(fee, peak) in events {
        let next_p = p.max(peak);
        let target = g + fee + next_p;
        if target > g + p {
            let profile = limits(target - 1)?;
            let mut charge = PolicySchemaCharge::new(profile);
            assert_eq!(
                normalize(input, maximum, profile, &mut charge),
                Err(work(target, target - 1))
            );
            assert_eq!((charge.g(), charge.p()), (g, p));
        }
        (g, p) = (g + fee, next_p);
    }
    assert_eq!((g, p), final_account);
    let profile = limits(g + p)?;
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(normalize(input, maximum, profile, &mut charge)?, expected);
    assert_eq!((charge.g(), charge.p()), final_account);
    Ok(())
}

fn assert_body_streams() -> TestResult {
    for (reason, body, digits) in [
        (
            Reason::Shape,
            b"ERR flow retention policy must be a map or keyword list".as_slice(),
            0,
        ),
        (
            Reason::Ttl,
            b"ERR flow retention ttl_ms must be between 1 and 31536000000",
            0,
        ),
        (
            Reason::HistoryHot,
            b"ERR flow retention history_hot_max_events is internal",
            0,
        ),
        (
            Reason::History { maximum: 1 },
            b"ERR flow retention history_max_events must be between 1 and 1",
            1,
        ),
        (
            Reason::History { maximum: 17 },
            b"ERR flow retention history_max_events must be between 1 and 17",
            2,
        ),
        (
            Reason::History { maximum: 1_000_000 },
            b"ERR flow retention history_max_events must be between 1 and 1000000",
            7,
        ),
    ] {
        let fees = [128, digits, 128 + u64::try_from(body.len())?];
        let mut g = 0;
        for fee in fees {
            if fee != 0 {
                let target = g + fee;
                let profile = limits(target - 1)?;
                let mut charge = PolicySchemaCharge::new(profile);
                assert_eq!(
                    encode(reason, profile, &mut charge),
                    Err(work(target, target - 1))
                );
                assert_eq!((charge.g(), charge.p()), (g, 0));
            }
            g += fee;
        }
        let profile = limits(g)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(encode(reason, profile, &mut charge)?, body);
        assert_eq!(
            (charge.g(), charge.p()),
            (256 + u64::try_from(body.len())? + digits, 0)
        );
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
    let result = normalize(&Raw::Map(rows), 17, profile, &mut charge);
    if requested == 64 {
        assert_eq!(result?, Ok(Some(RetentionOverride::default())));
        assert_eq!((charge.g(), charge.p()), (17_664, 64));
    } else {
        assert_eq!(result, Err(Error::ItemLimit));
        assert_eq!((charge.g(), charge.p()), (16_768, 0));
    }
    Ok(())
}

fn assert_aliases() -> TestResult {
    let profile = limits(10_000_000)?;
    for count in [2_047, 2_048] {
        let rows = vec![(Key::Other, Raw::Map(&[])); count];
        let mut charge = PolicySchemaCharge::new(profile);
        let result = normalize(&Raw::Map(&rows), 17, profile, &mut charge);
        if count == 2_047 {
            assert_eq!(result?, Ok(Some(RetentionOverride::default())));
            assert_eq!((charge.g(), charge.p()), (1_310_976, 64));
        } else {
            assert_eq!(result, Err(Error::ItemLimit));
            assert_eq!((charge.g(), charge.p()), (524_544, 0));
        }
        assert_eq!(rows.len(), count);
    }
    let shared = [(Key::Other, Raw::Nil)];
    for tail in [1, 2] {
        let mut rows = vec![(Key::Other, Raw::Map(&shared)); 1_023];
        rows.extend(std::iter::repeat_n((Key::Other, Raw::Nil), tail));
        let mut charge = PolicySchemaCharge::new(profile);
        let result = normalize(&Raw::Map(&rows), 17, profile, &mut charge);
        if tail == 1 {
            assert_eq!(result?, Ok(Some(RetentionOverride::default())));
            assert_eq!((charge.g(), charge.p()), (918_144, 64));
        } else {
            assert_eq!(result, Err(Error::ItemLimit));
            assert_eq!((charge.g(), charge.p()), (524_544, 0));
        }
        assert_eq!(rows.len(), 1_023 + tail);
    }
    Ok(())
}

fn assert_bytes_and_duplicates() -> TestResult {
    let profile = limits(10_000_000)?;
    for length in [262_144, 262_145] {
        let bytes = vec![0; length];
        let mut charge = PolicySchemaCharge::new(profile);
        let actual = normalize(&Raw::Binary(&bytes), 17, profile, &mut charge);
        if length == 262_144 {
            assert_eq!(actual?, Err(Reason::Shape));
            assert_eq!((charge.g(), charge.p()), (262_528, 0));
        } else {
            assert_eq!(actual, Err(Error::ByteLimit));
            assert_eq!((charge.g(), charge.p()), (256, 0));
        }
        assert!(bytes.iter().all(|&byte| byte == 0));
    }
    let bytes = vec![0; 262_145];
    let duplicate = [
        (Key::Atom("ttl_ms"), Raw::Integer(1)),
        (Key::Atom("ttl_ms"), Raw::Binary(&bytes)),
    ];
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(&Raw::Map(&duplicate), 17, profile, &mut charge),
        Err(Error::Malformed)
    );
    assert_eq!((charge.g(), charge.p()), (1_188, 0));
    let shadowed = [
        (Key::Atom("ttl_ms"), Raw::Integer(1)),
        (Key::Binary(b"ttl_ms"), Raw::Binary(&bytes)),
    ];
    let mut charge = PolicySchemaCharge::new(profile);
    assert_eq!(
        normalize(&Raw::Map(&shadowed), 17, profile, &mut charge),
        Err(Error::ByteLimit)
    );
    Ok(())
}

fn work(bytes: u64, maximum: u64) -> Error {
    Error::Schema(PolicySchemaError::WorkBytes { bytes, maximum })
}

#[test]
fn retention_input_keeps_one_account_prior_peak_and_old_contracts() -> TestResult {
    assert_profiles_and_captures()?;
    assert_continuous()?;
    assert_larger_peak()?;
    assert_held_bodies()?;
    for (full, expected_g) in [(false, 512), (true, 646)] {
        let profile = limits(10_000)?;
        let mut charge = PolicySchemaCharge::new(profile);
        if full {
            assert_eq!(
                normalize_complete_policy_retry_input(&Raw::Nil, profile, &mut charge)?
                    .map(|retry| retry.max_retries),
                Ok(3)
            );
        } else {
            assert_eq!(
                normalize_complete_policy_retry_override_input(&Raw::Nil, profile, &mut charge)?,
                Ok(None)
            );
        }
        assert_eq!((charge.g(), charge.p()), (expected_g, 0));
    }
    Ok(())
}

fn assert_profiles_and_captures() -> TestResult {
    let profile = limits(10_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let mismatch = Error::Schema(PolicySchemaError::ProfileMismatch);
    assert_eq!(
        normalize(&Raw::Nil, 17, limits(10_001)?, &mut charge),
        Err(mismatch)
    );
    assert_eq!(
        encode(
            Reason::History { maximum: 17 },
            limits(10_001)?,
            &mut charge
        ),
        Err(mismatch)
    );
    assert_eq!((charge.g(), charge.p()), (0, 0));
    for maximum in [0, 1_000_001, u32::MAX] {
        for input in [Raw::Nil, Raw::Atom("nil"), Raw::Other] {
            let mut charge = PolicySchemaCharge::new(profile);
            assert_eq!(
                normalize(&input, maximum, profile, &mut charge),
                Err(Error::Schema(PolicySchemaError::InvalidAuthority))
            );
            assert_eq!((charge.g(), charge.p()), (128, 0));
        }
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            encode(Reason::History { maximum }, profile, &mut charge),
            Err(Error::Schema(PolicySchemaError::InvalidAuthority))
        );
        assert_eq!((charge.g(), charge.p()), (128, 0));
    }
    Ok(())
}

fn assert_continuous() -> TestResult {
    for (maximum, repeats, seed, expected_g) in [
        (1_856, 2, false, 1_792),
        (1_855, 2, false, 1_664),
        (2_888, 1, true, 2_824),
        (2_887, 1, true, 2_696),
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
        for attempt in 0..repeats {
            let actual = normalize(&Raw::Map(&[]), 17, profile, &mut charge);
            if attempt + 1 == repeats && !maximum.is_multiple_of(2) {
                assert_eq!(actual, Err(work(maximum + 1, maximum)));
            } else {
                assert_eq!(actual?, Ok(Some(RetentionOverride::default())));
            }
        }
        assert_eq!((charge.g(), charge.p()), (expected_g, 64));
        if let Some(owner) = held {
            assert_eq!(owner.data().indexed_attributes, Some(vec![b"x".to_vec()]));
        }
    }
    Ok(())
}

fn assert_larger_peak() -> TestResult {
    for maximum in [5_871, 5_872] {
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
        .map_err(|_| "indexed seed")?;
        assert_eq!((charge.g(), charge.p()), (4_784, 192));
        let actual = normalize(&Raw::Keyword(&[]), 17, profile, &mut charge);
        if maximum == 5_872 {
            assert_eq!(actual?, Ok(Some(RetentionOverride::default())));
        } else {
            assert_eq!(actual, Err(work(5_872, maximum)));
        }
        assert_eq!(
            (charge.g(), charge.p()),
            (if maximum == 5_872 { 5_680 } else { 5_552 }, 192)
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

fn assert_held_bodies() -> TestResult {
    let profile = limits(959)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let reason = Reason::History { maximum: 17 };
    let expected = b"ERR flow retention history_max_events must be between 1 and 17";
    let first = encode(reason, profile, &mut charge)?;
    let first_pointer = first.as_ptr();
    assert_eq!((charge.g(), charge.p()), (320, 0));
    let second = encode(reason, profile, &mut charge)?;
    let second_pointer = second.as_ptr();
    assert_eq!((charge.g(), charge.p()), (640, 0));
    assert_eq!(first, expected);
    assert_eq!(first.as_ptr(), first_pointer);
    assert_eq!(encode(reason, profile, &mut charge), Err(work(960, 959)));
    assert_eq!((charge.g(), charge.p()), (770, 0));
    assert_eq!(first, expected);
    assert_eq!(second, expected);
    assert_eq!(
        (first.as_ptr(), second.as_ptr()),
        (first_pointer, second_pointer)
    );
    Ok(())
}
