//! Normalized core Patch semantics, ownership and one-account controls.

use std::collections::BTreeMap;

use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompletePolicyEncodingError, CompletePolicyValue,
    CompleteStatePolicyData, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    prepare_complete_core_policy_patch, prepare_complete_policy_value_v2,
    verify_complete_policy_value_v2,
};
use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicyPatch,
};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyLimits};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::{RetentionError, RetentionOverride};

use super::{TestResult, bind, candidate, data, limits};

mod assertions;
mod fixtures;
mod priorities;
mod states;

use assertions::{
    assert_core_patch_history_result, assert_core_patch_moved_input_owners,
    assert_core_patch_retry_overlay, assert_source_case8_core_policy,
};
use fixtures::preserved_core_patch_data;
use priorities::{
    assert_core_patch_guard_priorities, assert_core_patch_late_generation,
    assert_core_patch_none_type_priority, assert_core_patch_retry_error_priority,
};
use states::{
    assert_core_patch_empty_retention_presence, assert_core_patch_final_state_cardinality,
    assert_core_patch_invalid_state_operations, assert_core_patch_state_removals,
};

#[test]
fn core_patch_retains_independent_type_and_state_fields() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let mut original = data(
        b"r".to_vec(),
        None,
        BTreeMap::from([(
            b"queued".to_vec(),
            CompleteStatePolicyData {
                mode: Some(StateMode::Fifo),
                retry: Some(RetryOverride {
                    max_retries: Some(7),
                    backoff: None,
                    exhausted_to: None,
                }),
                retention: None,
                governance: None,
            },
        )]),
    );
    original.max_active_ms = Some(1_000);
    let old = prepare_complete_policy_value_v2(
        candidate(1, original, profile, &mut charge)?,
        profile,
        &mut charge,
    )?;
    let workflow_type = b"r".to_vec();
    let type_owner = workflow_type.as_ptr();
    let exhausted_to = b"dead".to_vec();
    let target_owner = exhausted_to.as_ptr();
    let review = b"review".to_vec();
    let review_owner = review.as_ptr();
    let patch = PolicyPatch {
        states: BTreeMap::from([
            (
                b"queued".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    retry: FieldPatch::Set(RetryPatch {
                        exhausted_to: Some(exhausted_to),
                        ..RetryPatch::default()
                    }),
                    ..StatePolicyPatch::default()
                }),
            ),
            (
                review,
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Fifo),
                    ..StatePolicyPatch::default()
                }),
            ),
        ]),
        ..PolicyPatch::default()
    };
    let merged = prepare_complete_core_policy_patch(
        Some(&old),
        0,
        CompleteCorePolicyPatchInput {
            workflow_type,
            expected_generation: None,
            patch,
        },
        64,
        profile,
        &mut charge,
    )?;
    assert_source_case8_core_policy(&old, &merged);
    let queued = merged
        .data()
        .states
        .get(b"queued".as_slice())
        .ok_or("queued")?;
    let target = queued
        .retry
        .as_ref()
        .ok_or("retry")?
        .exhausted_to
        .as_ref()
        .ok_or("target")?;
    let (review_key, _) = merged
        .data()
        .states
        .get_key_value(b"review".as_slice())
        .ok_or("review")?;
    assert_eq!(merged.data().workflow_type.as_ptr(), type_owner);
    assert_eq!(target.as_ptr(), target_owner);
    assert_eq!(review_key.as_ptr(), review_owner);
    assert_core_patch_moved_input_owners(profile, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_pays_semantic_copy_and_keeps_one_nonrollback_account() -> TestResult {
    // Candidate copy costs 1985; public decode costs 2153; old V2 control is 6495.
    let mut indexed_expected = data(b"r".to_vec(), None, BTreeMap::new());
    indexed_expected.indexed_attributes = Some(vec![b"a".to_vec()]);
    for (maximum, target, peak, has_base, attempts) in [
        (3_836, 3_836, false, false, 1_u64),
        (3_835, 3_836, false, false, 1),
        (10_678, 10_678, false, true, 1),
        (10_677, 10_678, false, true, 1),
        (18_800, 18_800, false, true, 2),
        (18_799, 18_800, false, true, 2),
        (12_670, 12_670, true, true, 1),
        (12_669, 12_670, true, true, 1),
    ] {
        let profile = limits(maximum)?;
        let mut charge = PolicySchemaCharge::new(profile);
        let held = if peak {
            let mut indexed = data(b"r".to_vec(), None, BTreeMap::new());
            indexed.indexed_attributes = Some(vec![b"a".to_vec()]);
            Some(candidate(1, indexed, profile, &mut charge)?)
        } else {
            None
        };
        if let Some(held) = &held {
            assert!(held.data() == &indexed_expected);
            assert_eq!(held.generation(), 1);
            assert_eq!((charge.g(), charge.p(), charge.total()), (1_928, 64, 1_992));
        }
        let base = if has_base {
            let minimal = data(b"r".to_vec(), None, BTreeMap::new());
            let current = candidate(1, minimal, profile, &mut charge)?;
            Some(prepare_complete_policy_value_v2(
                current,
                profile,
                &mut charge,
            )?)
        } else {
            None
        };
        let setup = (if peak { 1_928 } else { 0 }) + if has_base { 2_556 } else { 0 };
        let scratch = if peak { 64 } else { 0 };
        assert_eq!((charge.g(), charge.p()), (setup, scratch));
        let mut first = None;
        for index in 0..attempts {
            let workflow_type = b"r".to_vec();
            let descriptor = workflow_type.as_ptr();
            let input = CompleteCorePolicyPatchInput {
                workflow_type,
                expected_generation: None,
                patch: PolicyPatch::default(),
            };
            let result = prepare_complete_core_policy_patch(
                first.as_ref().or(base.as_ref()),
                0,
                input,
                1_000_000,
                profile,
                &mut charge,
            );
            let refused = index + 1 == attempts && maximum < target;
            let next = if refused {
                let error = result.err().ok_or("expected final emission work refusal")?;
                let expected_error = PolicySchemaError::WorkBytes {
                    bytes: target,
                    maximum,
                };
                assert_eq!(error, CompletePolicyEncodingError::Schema(expected_error));
                None
            } else {
                Some(result?)
            };
            let generation = (if has_base { 2 } else { 1 }) + index;
            assert_core_patch_minimal_values([
                (base.as_ref(), 1),
                (first.as_ref(), 2),
                (next.as_ref(), generation),
            ])?;
            if let Some(next) = &next {
                assert_eq!(next.data().workflow_type.as_ptr(), descriptor);
            }
            if index == 0 {
                first = next;
            }
            let delta = if has_base { 8_122 } else { 3_836 };
            let g = if refused {
                maximum - scratch
            } else {
                setup + (index + 1) * delta
            };
            let expected_charge = (g, scratch, g + scratch);
            assert_eq!((charge.g(), charge.p(), charge.total()), expected_charge);
        }
        if let Some(held) = &held {
            assert!(held.data() == &indexed_expected);
            assert_eq!(held.generation(), 1);
        }
    }
    Ok(())
}

fn assert_core_patch_minimal_values(
    values: [(Option<&CompletePolicyValue>, u64); 3],
) -> TestResult {
    let expected = data(b"r".to_vec(), None, BTreeMap::new());
    let minimal_e = [
        0x46, 0x57, 0x50, 0x56, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 22, 0x46, 0x57, 0x50, 0x53,
        0, 2, 0, 0, 0, 1, b'r', 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ];
    for (value, generation) in values {
        if let Some(value) = value {
            assert!(value.data() == &expected);
            assert_eq!(value.generation(), generation);
            let mut encoded = minimal_e;
            encoded[13] = u8::try_from(generation)?;
            assert_eq!(value.encoded(), encoded.as_slice());
        }
    }
    Ok(())
}

#[test]
fn core_patch_merges_backoff_without_expanding_omitted_overrides() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let previous = Backoff {
        kind: BackoffKind::Fixed,
        base_ms: 17,
        max_ms: 33,
        jitter_pct: 4,
    };
    for state_level in [false, true] {
        for case in 0..6 {
            let mut original = preserved_core_patch_data();
            let backoff = (case == 0).then_some(previous);
            let exhausted_to = (case == 3).then(|| b"dead".to_vec());
            let retry = if matches!(case, 0 | 3 | 5) {
                Some(RetryOverride {
                    max_retries: Some(7),
                    backoff,
                    exhausted_to,
                })
            } else {
                None
            };
            if state_level {
                original
                    .states
                    .get_mut(b"queued".as_slice())
                    .ok_or("queued")?
                    .retry = retry;
            } else {
                original.retry = retry;
            }
            let old = prepare_complete_policy_value_v2(
                candidate(1, original, profile, &mut charge)?,
                profile,
                &mut charge,
            )?;
            let original_bytes = old.encoded().to_vec();
            let retry_patch = match case {
                0 => FieldPatch::Set(RetryPatch {
                    backoff: Some(BackoffPatch {
                        jitter_pct: Some(25),
                        ..BackoffPatch::default()
                    }),
                    ..RetryPatch::default()
                }),
                1 => FieldPatch::Set(RetryPatch {
                    backoff: Some(BackoffPatch::default()),
                    ..RetryPatch::default()
                }),
                2 | 3 => FieldPatch::Set(RetryPatch::default()),
                4 => FieldPatch::Unchanged,
                5 => FieldPatch::Set(RetryPatch {
                    max_retries: Some(8),
                    ..RetryPatch::default()
                }),
                _ => return Err("unexpected retry case".into()),
            };
            let mut patch = PolicyPatch::default();
            if state_level {
                patch.states.insert(
                    b"queued".to_vec(),
                    StateChange::Patch(StatePolicyPatch {
                        retry: retry_patch,
                        ..StatePolicyPatch::default()
                    }),
                );
            } else {
                patch.retry = retry_patch;
            }
            let merged = prepare_complete_core_policy_patch(
                Some(&old),
                0,
                CompleteCorePolicyPatchInput {
                    workflow_type: b"r".to_vec(),
                    expected_generation: Some(1),
                    patch,
                },
                64,
                profile,
                &mut charge,
            )?;
            assert_core_patch_retry_overlay(case, state_level, &old, &merged, &original_bytes)?;
            verify_complete_policy_value_v2(&merged, profile, &mut charge)?;
        }
    }
    Ok(())
}

#[test]
fn core_patch_clears_optional_groups_and_deletes_states() -> TestResult {
    let profile = PolicySchemaLimits::new(PolicyLimits::new(2)?, 20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let old = prepare_complete_policy_value_v2(
        candidate(1, preserved_core_patch_data(), profile, &mut charge)?,
        profile,
        &mut charge,
    )?;
    let original_bytes = old.encoded().to_vec();
    for case in 0..3 {
        let patch = if case == 0 {
            PolicyPatch {
                version: FieldPatch::Clear,
                max_active_ms: FieldPatch::Clear,
                retry: FieldPatch::Clear,
                retention: FieldPatch::Clear,
                states: BTreeMap::from([(
                    b"queued".to_vec(),
                    StateChange::Patch(StatePolicyPatch {
                        retry: FieldPatch::Clear,
                        retention: FieldPatch::Clear,
                        ..StatePolicyPatch::default()
                    }),
                )]),
            }
        } else if case == 1 {
            PolicyPatch {
                retention: FieldPatch::Set(RetentionOverride::default()),
                states: BTreeMap::from([(
                    b"queued".to_vec(),
                    StateChange::Patch(StatePolicyPatch {
                        retention: FieldPatch::Set(RetentionOverride::default()),
                        ..StatePolicyPatch::default()
                    }),
                )]),
                ..PolicyPatch::default()
            }
        } else {
            PolicyPatch::default()
        };
        let merged = prepare_complete_core_policy_patch(
            Some(&old),
            0,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: Some(1),
                patch,
            },
            64,
            profile,
            &mut charge,
        )?;
        let mut expected = preserved_core_patch_data();
        if case == 0 {
            expected.version = None;
            expected.max_active_ms = None;
            expected.retry = None;
            expected.retention = None;
            let queued = expected
                .states
                .get_mut(b"queued".as_slice())
                .ok_or("queued")?;
            queued.retry = None;
            queued.retention = None;
        }
        assert!(merged.data() == &expected);
        assert_eq!(merged.generation(), 2);
        verify_complete_policy_value_v2(&merged, profile, &mut charge)?;
        assert!(old.data() == &preserved_core_patch_data());
        assert_eq!(old.generation(), 1);
        assert_eq!(old.encoded(), original_bytes);
    }
    assert_core_patch_empty_retention_presence(profile, &mut charge)?;
    assert_core_patch_state_removals(profile, &mut charge)?;
    assert_core_patch_invalid_state_operations(profile, &mut charge)?;
    assert_core_patch_final_state_cardinality(profile, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_uses_contextual_history_limits_in_source_order() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    for (state_level, old_history, history, ttl, error) in [
        (false, None, Some(64), None, None),
        (true, None, Some(64), None, None),
        (false, None, Some(65), None, Some(RetentionError::History)),
        (true, None, Some(65), None, Some(RetentionError::History)),
        (false, Some(65), None, None, Some(RetentionError::History)),
        (true, Some(65), None, None, Some(RetentionError::History)),
        (false, None, Some(65), Some(0), Some(RetentionError::Ttl)),
        (true, None, Some(65), Some(0), Some(RetentionError::Ttl)),
    ] {
        let mut original = data(b"r".to_vec(), None, BTreeMap::new());
        let mut expected_old = data(b"r".to_vec(), None, BTreeMap::new());
        let mut expected_new = data(b"r".to_vec(), None, BTreeMap::new());
        let stored = Some(RetentionOverride {
            ttl_ms: None,
            history_max_events: old_history,
        });
        for value in [&mut original, &mut expected_old, &mut expected_new] {
            if state_level {
                value.states.insert(
                    b"queued".to_vec(),
                    CompleteStatePolicyData {
                        mode: None,
                        retry: None,
                        retention: stored,
                        governance: None,
                    },
                );
            } else {
                value.retention = stored;
            }
        }
        let old = prepare_complete_policy_value_v2(
            candidate(1, original, profile, &mut charge)?,
            profile,
            &mut charge,
        )?;
        let original_bytes = old.encoded().to_vec();
        let incoming = if history.is_some() {
            FieldPatch::Set(RetentionOverride {
                ttl_ms: ttl,
                history_max_events: history,
            })
        } else {
            FieldPatch::Unchanged
        };
        let mut patch = PolicyPatch::default();
        if state_level && history.is_some() {
            patch.states.insert(
                b"queued".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    retention: incoming,
                    ..StatePolicyPatch::default()
                }),
            );
        } else if !state_level {
            patch.retention = incoming;
        }
        let result = prepare_complete_core_policy_patch(
            Some(&old),
            0,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: Some(1),
                patch,
            },
            64,
            profile,
            &mut charge,
        );
        assert_core_patch_history_result(
            result,
            &old,
            (&expected_old, expected_new),
            &original_bytes,
            (state_level, error),
            profile,
            &mut charge,
        )?;
    }
    assert_core_patch_retry_error_priority(profile, &mut charge)?;
    Ok(())
}

#[test]
fn core_patch_preserves_source_guard_and_late_high_water_priorities() -> TestResult {
    let profile = limits(20_000_000)?;
    let mut charge = PolicySchemaCharge::new(profile);
    let mut expected_bytes = [
        b"FWPV\x00\x02\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x16".as_slice(),
        b"FWPS\x00\x02\x00\x00\x00\x01r\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
    ]
    .concat();
    let old = prepare_complete_policy_value_v2(
        candidate(
            1,
            data(b"r".to_vec(), None, BTreeMap::new()),
            profile,
            &mut charge,
        )?,
        profile,
        &mut charge,
    )?;
    let mut corrupted = expected_bytes.clone();
    corrupted[39] = 2;
    let corrupt = bind(
        candidate(
            1,
            data(b"r".to_vec(), None, BTreeMap::new()),
            profile,
            &mut charge,
        )?,
        corrupted,
        18..40,
        profile,
        &mut charge,
    )?;
    let before = (charge.g(), charge.p(), charge.total());
    let mismatched = limits(19_999_999)?;
    let result = prepare_complete_core_policy_patch(
        Some(&corrupt),
        MAX_POLICY_GENERATION + 1,
        CompleteCorePolicyPatchInput {
            workflow_type: Vec::new(),
            expected_generation: Some(MAX_POLICY_GENERATION + 1),
            patch: PolicyPatch {
                version: FieldPatch::Set(PolicyVersion::Bytes(Vec::new())),
                retry: FieldPatch::Set(RetryPatch {
                    max_retries: Some(1_001),
                    ..RetryPatch::default()
                }),
                ..PolicyPatch::default()
            },
        },
        64,
        mismatched,
        &mut charge,
    );
    assert_eq!(
        result.err(),
        Some(CompletePolicyEncodingError::Schema(
            PolicySchemaError::ProfileMismatch,
        ))
    );
    assert_eq!((charge.g(), charge.p(), charge.total()), before);
    assert!(old.data() == &data(b"r".to_vec(), None, BTreeMap::new()));
    assert_eq!(old.generation(), 1);
    assert_eq!(old.encoded(), expected_bytes);
    assert!(corrupt.data() == &data(b"r".to_vec(), None, BTreeMap::new()));
    assert_eq!(corrupt.generation(), 1);
    expected_bytes[39] = 2;
    assert_eq!(corrupt.encoded(), expected_bytes);
    assert_core_patch_guard_priorities(&old, &corrupt, profile, &mut charge);
    assert_core_patch_none_type_priority(profile, &mut charge);
    assert_core_patch_late_generation(&old, profile, &mut charge)?;
    Ok(())
}
