//! Fixed input/work/scratch boundaries on one nonrefunding schema account.

use super::{
    BTreeMap, CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError, Raw, TestResult,
    candidate, data, limits, names, normalize, normalize_complete_policy_indexed_input,
};

#[test]
fn indexed_input_refuses_raw_caps_and_work_before_owner_construction() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    for nodes in [4_095, 4_096] {
        let repeated = vec![Raw::Nil; nodes];
        let result = normalize_complete_policy_indexed_input(
            Some(Raw::List(&repeated)),
            None,
            profile,
            &mut charge,
        );
        if nodes == 4_095 {
            assert_eq!(result??, (Some(vec![b"nil".to_vec()]), None));
        } else {
            assert_eq!(result, Err(CompletePolicyEncodingError::ItemLimit));
        }
    }
    for length in [262_144, 262_145] {
        let mut bytes = vec![b' '; length];
        bytes[0] = b'k';
        let list = [Raw::Binary(&bytes)];
        let result = normalize_complete_policy_indexed_input(
            Some(Raw::List(&list)),
            None,
            profile,
            &mut charge,
        );
        if length == 262_144 {
            assert_eq!(result??, (Some(vec![b"k".to_vec()]), None));
        } else {
            assert_eq!(result, Err(CompletePolicyEncodingError::ByteLimit));
        }
        assert_eq!(bytes[0], b'k');
        assert!(bytes[1..].iter().all(|&byte| byte == b' '));
    }
    assert_nested_depth(Raw::Binary(b"k"), 64, 64)?;
    assert_nested_depth(Raw::Binary(b"k"), 65, 65)?;
    assert_exact_preflight_refusals()?;
    assert_combined_scratch()?;
    assert_shared_occurrences()?;
    assert_each_source_event()?;
    Ok(())
}

fn assert_combined_scratch() -> TestResult {
    let attributes = [Raw::Binary(b"a")];
    let meta = [Raw::Binary(b"k")];
    for (maximum, retained, peak) in [(897, 642, 0), (2_953, 2_569, 256), (2_954, 2_698, 256)] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let result = normalize_complete_policy_indexed_input(
            Some(Raw::List(&attributes)),
            Some(Raw::List(&meta)),
            profile,
            &mut charge,
        );
        if maximum == 2_954 {
            assert_eq!(result??, (Some(vec![b"a".to_vec()]), Some(b"k".to_vec())));
        } else {
            let bytes = if maximum == 897 { 898 } else { 2_954 };
            assert_eq!(
                result,
                Err(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::WorkBytes { bytes, maximum }
                ))
            );
        }
        assert_eq!((charge.g(), charge.p()), (retained, peak));
        assert_eq!(attributes, [Raw::Binary(b"a")]);
        assert_eq!(meta, [Raw::Binary(b"k")]);
    }
    Ok(())
}

fn assert_shared_occurrences() -> TestResult {
    let leaf = [Raw::Nil];
    for extra in [1, 2] {
        let mut shared = vec![Raw::List(&leaf); 2_047];
        shared.extend(std::iter::repeat_n(Raw::Nil, extra));
        let profile = limits(2_000_000)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let result = normalize_complete_policy_indexed_input(
            None,
            Some(Raw::List(&shared)),
            profile,
            &mut charge,
        );
        if extra == 1 {
            assert_eq!(result??, (None, None));
            assert_eq!((charge.g(), charge.p()), (1_048_832, 128));
        } else {
            assert_eq!(result, Err(CompletePolicyEncodingError::ItemLimit));
            assert_eq!((charge.g(), charge.p()), (524_544, 0));
        }
        assert_eq!(leaf, [Raw::Nil]);
        assert_eq!(shared.len(), 2_047 + extra);
    }
    Ok(())
}

fn assert_nested_depth(raw: Raw<'_>, remaining: usize, depth: usize) -> TestResult {
    if remaining != 0 {
        let list = [raw];
        return assert_nested_depth(Raw::List(&list), remaining - 1, depth);
    }
    let profile = limits(30_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let result = normalize_complete_policy_indexed_input(None, Some(raw), profile, &mut charge);
    if depth == 64 {
        assert_eq!(result??, (None, Some(b"k".to_vec())));
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (17_541, 4_096, 21_637)
        );
    } else {
        assert_eq!(result, Err(CompletePolicyEncodingError::ItemLimit));
        assert_eq!((charge.g(), charge.p()), (8_576, 0));
    }
    Ok(())
}

fn assert_exact_preflight_refusals() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let oversized = vec![b' '; 262_145];
    let list = [Raw::Binary(&oversized)];
    assert_eq!(
        normalize_complete_policy_indexed_input(Some(Raw::List(&list)), None, profile, &mut charge,),
        Err(CompletePolicyEncodingError::ByteLimit)
    );
    assert_eq!((charge.g(), charge.p()), (384, 0));
    let repeated = vec![Raw::Nil; 4_096];
    let before = charge.g();
    assert_eq!(
        normalize_complete_policy_indexed_input(
            Some(Raw::List(&repeated)),
            None,
            profile,
            &mut charge,
        ),
        Err(CompletePolicyEncodingError::ItemLimit)
    );
    assert_eq!((charge.g(), charge.p()), (before + 524_544, 0));
    let other = limits(20_000_001)?;
    let before = (charge.g(), charge.p());
    assert_eq!(
        normalize_complete_policy_indexed_input(
            Some(Raw::Other),
            Some(Raw::Other),
            other,
            &mut charge
        ),
        Err(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch
        ))
    );
    assert_eq!((charge.g(), charge.p()), before);
    // The second-tree operational bound precedes an admitted first-tree source shape error.
    assert_eq!(
        normalize_complete_policy_indexed_input(
            Some(Raw::Other),
            Some(Raw::Binary(&oversized)),
            profile,
            &mut charge,
        ),
        Err(CompletePolicyEncodingError::ByteLimit)
    );
    Ok(())
}

fn source_events() -> Vec<(u64, u64)> {
    vec![
        (128, 0),
        (128, 0),
        (128, 0),
        (6, 0),
        (128, 0),
        (6, 0),
        (128, 0),
        (8, 0),
        (128, 0),
        (6, 0),
        (0, 192),
        (128, 0),
        (128, 0),
        (134, 0),
        (134, 0),
        (134, 0),
        (128, 0),
        (134, 0),
        (134, 0),
        (134, 0),
        (140, 0),
        (128, 0),
        (136, 0),
        (136, 0),
        (136, 0),
        (142, 0),
        (142, 0),
        (128, 0),
        (134, 0),
        (134, 0),
        (134, 0),
        (140, 0),
        (128, 0),
        (128, 0),
        (512, 0),
        (134, 0),
        (134, 0),
        (136, 0),
    ]
}

fn assert_each_source_event() -> TestResult {
    let input = names();
    let mut prior_g = 0;
    let mut prior_p = 0;
    for (fee, scratch) in source_events() {
        let next_p = prior_p.max(scratch);
        let target = prior_g + fee + next_p;
        let maximum = target - 1;
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        assert_eq!(
            normalize_complete_policy_indexed_input(
                Some(Raw::List(&input)),
                None,
                profile,
                &mut charge
            ),
            Err(CompletePolicyEncodingError::Schema(
                PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum,
                }
            ))
        );
        assert_eq!((charge.g(), charge.p()), (prior_g, prior_p));
        assert_eq!(input, names());
        prior_g += fee;
        prior_p = next_p;
    }
    assert_eq!((prior_g, prior_p), (4_784, 192));
    Ok(())
}

#[test]
fn indexed_input_keeps_repeated_outputs_and_prior_peak_on_one_account() -> TestResult {
    for (maximum, attempts, retained) in [
        (4_976, 1, 4_784),
        (4_975, 1, 4_648),
        (9_760, 2, 9_568),
        (9_759, 2, 9_432),
    ] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let input = names();
        let expected = Some(vec![
            b"tenant".to_vec(),
            b"region".to_vec(),
            b"campaign".to_vec(),
        ]);
        let mut held = None;
        for attempt in 0..attempts {
            let result = normalize_complete_policy_indexed_input(
                Some(Raw::List(&input)),
                None,
                profile,
                &mut charge,
            );
            let target = if attempts == 1 { 4_976 } else { 9_760 };
            if attempt + 1 == attempts && maximum < target {
                assert_eq!(
                    result,
                    Err(CompletePolicyEncodingError::Schema(
                        PolicySchemaError::WorkBytes {
                            bytes: target,
                            maximum
                        },
                    ))
                );
            } else {
                let owners = result??;
                assert_eq!(owners, (expected.clone(), None));
                held = Some(owners);
            }
        }
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (retained, 192, retained + 192)
        );
        if let Some(owners) = held {
            assert_eq!(owners, (expected, None));
        }
        assert_eq!(input, names());
    }
    assert_absent_and_empty_fees()?;
    assert_real_prior_peak()?;
    Ok(())
}

fn assert_absent_and_empty_fees() -> TestResult {
    for (raw, fee) in [
        (None, 384),
        (Some(Raw::Nil), 768),
        (Some(Raw::List(&[])), 768),
    ] {
        let profile = limits(2 * fee)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let expected = raw.map(|_| Vec::new());
        let first = normalize(raw, None, profile, &mut charge)?;
        assert_eq!(first, (expected.clone(), None));
        assert_eq!((charge.g(), charge.p()), (fee, 0));
        let second = normalize(raw, None, profile, &mut charge)?;
        assert_eq!(first, second);
        assert_eq!((charge.g(), charge.p()), (2 * fee, 0));
    }
    let nested = [Raw::Atom("")];
    for (meta, fee, peak) in [(Raw::Atom(""), 512, 0), (Raw::List(&nested), 768, 64)] {
        let profile = limits(2 * fee + peak)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let first = normalize(None, Some(meta), profile, &mut charge)?;
        assert_eq!(first, (None, None));
        assert_eq!((charge.g(), charge.p()), (fee, peak));
        let second = normalize(None, Some(meta), profile, &mut charge)?;
        assert_eq!(first, second);
        assert_eq!((charge.g(), charge.p()), (2 * fee, peak));
    }
    Ok(())
}

fn assert_real_prior_peak() -> TestResult {
    for maximum in [2_375, 2_376] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let mut supplied = data(b"r".to_vec(), None, BTreeMap::new());
        supplied.indexed_attributes = Some(vec![b"x".to_vec()]);
        let held = candidate(1, supplied, profile, &mut charge)?;
        assert_eq!((charge.g(), charge.p()), (1_928, 64));
        let result = normalize_complete_policy_indexed_input(None, None, profile, &mut charge);
        if maximum == 2_375 {
            assert_eq!(
                result,
                Err(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::WorkBytes {
                        bytes: 2_376,
                        maximum
                    },
                ))
            );
            assert_eq!((charge.g(), charge.p(), charge.total()), (2_184, 64, 2_248));
        } else {
            assert_eq!(result??, (None, None));
            assert_eq!((charge.g(), charge.p(), charge.total()), (2_312, 64, 2_376));
        }
        assert_eq!(held.data().indexed_attributes, Some(vec![b"x".to_vec()]));
        assert_eq!(held.generation(), 1);
    }
    Ok(())
}
