//! Final state cardinality, removals and optional retention presence controls.

use std::collections::BTreeMap;

use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompletePolicyEncodingError, CompleteStatePolicyData,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, prepare_complete_core_policy_patch,
    prepare_complete_policy_value_v2, verify_complete_policy_value_v2,
};
use wf_domain::policy::definition::{
    FieldPatch, PolicyPatch, RetryPatch, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::policy::installation::PolicyError;
use wf_domain::retention::RetentionOverride;

use super::super::{TestResult, candidate, data};
use super::fixtures::preserved_core_patch_data;

pub(super) fn assert_core_patch_empty_retention_presence(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let merged = prepare_complete_core_policy_patch(
        None,
        0,
        CompleteCorePolicyPatchInput {
            workflow_type: b"r".to_vec(),
            expected_generation: Some(0),
            patch: PolicyPatch {
                retention: FieldPatch::Set(RetentionOverride::default()),
                states: BTreeMap::from([(
                    b"queued".to_vec(),
                    StateChange::Patch(StatePolicyPatch {
                        retention: FieldPatch::Set(RetentionOverride::default()),
                        ..StatePolicyPatch::default()
                    }),
                )]),
                ..PolicyPatch::default()
            },
        },
        64,
        profile,
        charge,
    )?;
    let mut expected = data(
        b"r".to_vec(),
        None,
        BTreeMap::from([(
            b"queued".to_vec(),
            CompleteStatePolicyData {
                mode: None,
                retry: None,
                retention: Some(RetentionOverride::default()),
                governance: None,
            },
        )]),
    );
    expected.retention = Some(RetentionOverride::default());
    let expected_bytes = [
        b"FWPV\x00\x02\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x28".as_slice(),
        b"FWPS\x00\x02\x00\x00\x00\x01r\x00\x00\x00\x01\x00\x00\x00\x00\x00\x01",
        b"\x00\x00\x00\x06queued\x00\x00\x01\x00\x00\x00\x00\x00\x00",
    ]
    .concat();
    assert!(merged.data() == &expected);
    assert_eq!(merged.generation(), 1);
    assert_eq!(merged.encoded(), expected_bytes);
    Ok(())
}

pub(super) fn assert_core_patch_state_removals(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for had_states in [false, true] {
        let mut original = preserved_core_patch_data();
        let mut expected_old = preserved_core_patch_data();
        for value in [&mut original, &mut expected_old] {
            if had_states {
                value.states.insert(
                    b"sentinel".to_vec(),
                    CompleteStatePolicyData {
                        mode: Some(StateMode::Parallel),
                        retry: None,
                        retention: None,
                        governance: None,
                    },
                );
            } else {
                value.states.clear();
            }
        }
        let old = prepare_complete_policy_value_v2(
            candidate(1, original, profile, charge)?,
            profile,
            charge,
        )?;
        let original_bytes = old.encoded().to_vec();
        for selector in [b"queued".as_slice(), b"absent", b"", b"running"] {
            let merged = prepare_complete_core_policy_patch(
                Some(&old),
                0,
                CompleteCorePolicyPatchInput {
                    workflow_type: b"r".to_vec(),
                    expected_generation: Some(1),
                    patch: PolicyPatch {
                        states: BTreeMap::from([(selector.to_vec(), StateChange::Remove)]),
                        ..PolicyPatch::default()
                    },
                },
                64,
                profile,
                charge,
            )?;
            let mut expected = preserved_core_patch_data();
            if had_states {
                expected.states.insert(
                    b"sentinel".to_vec(),
                    CompleteStatePolicyData {
                        mode: Some(StateMode::Parallel),
                        retry: None,
                        retention: None,
                        governance: None,
                    },
                );
                if selector == b"queued" {
                    expected.states.remove(b"queued".as_slice());
                }
            } else {
                expected.states.clear();
            }
            assert!(merged.data() == &expected);
            assert_eq!(merged.generation(), 2);
            verify_complete_policy_value_v2(&merged, profile, charge)?;
            assert!(old.data() == &expected_old);
            assert_eq!(old.generation(), 1);
            assert_eq!(old.encoded(), original_bytes);
        }
    }
    Ok(())
}

pub(super) fn assert_core_patch_invalid_state_operations(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let old = prepare_complete_policy_value_v2(
        candidate(
            1,
            data(b"r".to_vec(), None, BTreeMap::new()),
            profile,
            charge,
        )?,
        profile,
        charge,
    )?;
    let original_bytes = old.encoded().to_vec();
    for had_existing in [false, true] {
        let existing = if had_existing { Some(&old) } else { None };
        let operation = if had_existing {
            StateChange::Patch(StatePolicyPatch {
                mode: FieldPatch::Clear,
                retry: FieldPatch::Set(RetryPatch {
                    max_retries: Some(1_001),
                    ..RetryPatch::default()
                }),
                ..StatePolicyPatch::default()
            })
        } else {
            StateChange::Remove
        };
        let result = prepare_complete_core_policy_patch(
            existing,
            0,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation: None,
                patch: PolicyPatch {
                    states: BTreeMap::from([(b"queued".to_vec(), operation)]),
                    ..PolicyPatch::default()
                },
            },
            64,
            profile,
            charge,
        );
        let error = if had_existing {
            PolicyError::InvalidStateMode
        } else {
            PolicyError::InvalidStatePolicy
        };
        assert_eq!(
            result.err(),
            Some(CompletePolicyEncodingError::Schema(
                PolicySchemaError::Policy(error),
            ))
        );
        assert!(old.data() == &data(b"r".to_vec(), None, BTreeMap::new()));
        assert_eq!(old.generation(), 1);
        assert_eq!(old.encoded(), original_bytes);
    }
    Ok(())
}

pub(super) fn assert_core_patch_final_state_cardinality(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let mut original = data(b"r".to_vec(), None, BTreeMap::new());
    let mut expected_old = data(b"r".to_vec(), None, BTreeMap::new());
    for value in [&mut original, &mut expected_old] {
        for key in [b"c", b"d"] {
            value.states.insert(
                key.to_vec(),
                CompleteStatePolicyData {
                    mode: None,
                    retry: None,
                    retention: None,
                    governance: None,
                },
            );
        }
    }
    let old = prepare_complete_policy_value_v2(
        candidate(1, original, profile, charge)?,
        profile,
        charge,
    )?;
    let original_bytes = old.encoded().to_vec();
    for add_third in [false, true] {
        let mut patch = PolicyPatch {
            states: BTreeMap::from([
                (b"c".to_vec(), StateChange::Remove),
                (b"d".to_vec(), StateChange::Remove),
            ]),
            ..PolicyPatch::default()
        };
        for key in [b"a", b"b"] {
            patch.states.insert(
                key.to_vec(),
                StateChange::Patch(StatePolicyPatch::default()),
            );
        }
        if add_third {
            patch.states.insert(
                b"e".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Clear,
                    ..StatePolicyPatch::default()
                }),
            );
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
            charge,
        );
        if add_third {
            assert_eq!(
                result.err(),
                Some(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::Policy(PolicyError::StateBudget),
                ))
            );
        } else {
            let merged = result?;
            let mut expected = data(b"r".to_vec(), None, BTreeMap::new());
            for key in [b"a", b"b"] {
                expected.states.insert(
                    key.to_vec(),
                    CompleteStatePolicyData {
                        mode: None,
                        retry: None,
                        retention: None,
                        governance: None,
                    },
                );
            }
            let expected_bytes = [
                b"FWPV\x00\x02\x00\x00\x00\x00\x00\x00\x00\x02\x00\x00\x00\x28".as_slice(),
                b"FWPS\x00\x02\x00\x00\x00\x01r\x00\x00\x00\x00\x00\x00\x00\x02",
                b"\x00\x00\x00\x01a\x00\x00\x00\x00",
                b"\x00\x00\x00\x01b\x00\x00\x00\x00\x00\x00\x00",
            ]
            .concat();
            assert!(merged.data() == &expected);
            assert_eq!(merged.generation(), 2);
            assert_eq!(merged.encoded(), expected_bytes);
        }
        assert!(old.data() == &expected_old);
        assert_eq!(old.generation(), 1);
        assert_eq!(old.encoded(), original_bytes);
    }
    Ok(())
}
