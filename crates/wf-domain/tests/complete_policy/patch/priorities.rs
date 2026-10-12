//! Source error priority, same-account guard and late generation controls.

use std::collections::BTreeMap;

use wf_domain::policy::BackoffKind;
use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompletePolicyEncodingError, CompletePolicyValue,
    PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits, prepare_complete_core_policy_patch,
    prepare_complete_policy_value_v2,
};
use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StatePolicyPatch,
};
use wf_domain::policy::installation::{MAX_POLICY_GENERATION, PolicyError};
use wf_domain::retention::RetentionOverride;
use wf_domain::value::{DomainError, InputField, RetryPolicyField};

use super::super::{TestResult, candidate, data};

pub(super) fn assert_core_patch_retry_error_priority(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for state_level in [false, true] {
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
        let retry = FieldPatch::Set(RetryPatch {
            max_retries: Some(1_001),
            backoff: Some(BackoffPatch {
                kind: Some(BackoffKind::Fixed),
                base_ms: Some(2_592_000_001),
                max_ms: Some(2_592_000_001),
                jitter_pct: Some(101),
            }),
            exhausted_to: Some(Vec::new()),
        });
        let retention = FieldPatch::Set(RetentionOverride {
            ttl_ms: Some(0),
            history_max_events: Some(65),
        });
        let mut patch = PolicyPatch::default();
        if state_level {
            patch.states.insert(
                b"queued".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    retry,
                    retention,
                    ..StatePolicyPatch::default()
                }),
            );
        } else {
            patch.retry = retry;
            patch.retention = retention;
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
        let expected = PolicySchemaError::Policy(PolicyError::Domain(
            DomainError::InvalidRetryPolicy(RetryPolicyField::MaxRetries),
        ));
        assert_eq!(
            result.err(),
            Some(CompletePolicyEncodingError::Schema(expected))
        );
        assert!(old.data() == &data(b"r".to_vec(), None, BTreeMap::new()));
        assert_eq!(old.generation(), 1);
        let bytes = [
            b"FWPV\x00\x02\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x16".as_slice(),
            b"FWPS\x00\x02\x00\x00\x00\x01r\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
        ]
        .concat();
        assert_eq!(old.encoded(), bytes);
    }
    Ok(())
}

pub(super) fn assert_core_patch_guard_priorities(
    old: &CompletePolicyValue,
    corrupt: &CompletePolicyValue,
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) {
    let old_bytes = old.encoded().to_vec();
    let corrupt_bytes = corrupt.encoded().to_vec();
    for case in 0..5 {
        let existing = if case <= 1 { corrupt } else { old };
        let expected_generation = match case {
            0 => Some(0),
            4 => Some(MAX_POLICY_GENERATION + 1),
            _ => Some(1),
        };
        let before = (charge.g(), charge.p(), charge.total());
        let result = prepare_complete_core_policy_patch(
            Some(existing),
            MAX_POLICY_GENERATION + 1,
            CompleteCorePolicyPatchInput {
                workflow_type: if case == 2 {
                    b"x".to_vec()
                } else {
                    b"r".to_vec()
                },
                expected_generation,
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
            profile,
            charge,
        );
        let (error, delta) = match case {
            0 => (
                CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(
                    PolicyError::StaleGeneration,
                )),
                128,
            ),
            1 => (CompletePolicyEncodingError::EncodingMismatch, 2_685),
            2 => (
                CompletePolicyEncodingError::Schema(PolicySchemaError::InvalidAuthority),
                899,
            ),
            3 => (
                CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(
                    PolicyError::InvalidVersion,
                )),
                5_054,
            ),
            _ => (
                CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(
                    PolicyError::InvalidGeneration,
                )),
                128,
            ),
        };
        assert_eq!(result.err(), Some(error));
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (before.0 + delta, before.1, before.2 + delta),
        );
        assert!(old.data() == &data(b"r".to_vec(), None, BTreeMap::new()));
        assert_eq!(old.generation(), 1);
        assert_eq!(old.encoded(), old_bytes);
        assert!(corrupt.data() == &data(b"r".to_vec(), None, BTreeMap::new()));
        assert_eq!(corrupt.generation(), 1);
        assert_eq!(corrupt.encoded(), corrupt_bytes);
    }
}

pub(super) fn assert_core_patch_none_type_priority(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) {
    let empty = PolicySchemaError::Policy(PolicyError::Domain(DomainError::EmptyValue(
        InputField::Type,
    )));
    for (workflow_type, error) in [
        (Vec::new(), empty),
        (vec![b'r'; 262_145], PolicySchemaError::SnapshotBytes),
    ] {
        let before = (charge.g(), charge.p(), charge.total());
        let result = prepare_complete_core_policy_patch(
            None,
            MAX_POLICY_GENERATION + 1,
            CompleteCorePolicyPatchInput {
                workflow_type,
                expected_generation: None,
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
            profile,
            charge,
        );
        assert_eq!(
            result.err(),
            Some(CompletePolicyEncodingError::Schema(error))
        );
        assert_eq!(
            (charge.g(), charge.p(), charge.total()),
            (before.0 + 256, before.1, before.2 + 256),
        );
    }
}

pub(super) fn assert_core_patch_late_generation(
    old: &CompletePolicyValue,
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let old_bytes = old.encoded().to_vec();
    let stale = CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(
        PolicyError::StaleGeneration,
    ));
    let exhausted = CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(
        PolicyError::GenerationExhausted,
    ));
    let authority = CompletePolicyEncodingError::Schema(PolicySchemaError::InvalidAuthority);
    for (existing, expected_generation, high_water, generation, error) in [
        (None, None, 0, Some(1_u64), None),
        (None, Some(0), 6, Some(7), None),
        (Some(old), None, 6, Some(7), None),
        (Some(old), Some(1), 6, Some(7), None),
        (Some(old), Some(1), 0, Some(2), None),
        (Some(old), Some(0), 0, None, Some(stale)),
        (None, Some(1), 0, None, Some(stale)),
        (
            Some(old),
            None,
            MAX_POLICY_GENERATION + 1,
            None,
            Some(authority),
        ),
        (
            Some(old),
            None,
            MAX_POLICY_GENERATION,
            None,
            Some(exhausted),
        ),
        (None, None, MAX_POLICY_GENERATION + 1, None, Some(authority)),
        (None, None, MAX_POLICY_GENERATION, None, Some(exhausted)),
    ] {
        let before = (charge.g(), charge.p(), charge.total());
        let result = prepare_complete_core_policy_patch(
            existing,
            high_water,
            CompleteCorePolicyPatchInput {
                workflow_type: b"r".to_vec(),
                expected_generation,
                patch: PolicyPatch::default(),
            },
            64,
            profile,
            charge,
        );
        if let Some(error) = error {
            assert_eq!(result.err(), Some(error));
            let delta = if error == stale {
                128
            } else if existing.is_some() {
                6_208
            } else {
                1_922
            };
            assert_eq!(
                (charge.g(), charge.p(), charge.total()),
                (before.0 + delta, before.1, before.2 + delta),
            );
        } else {
            let merged = result?;
            let generation = generation.ok_or("missing expected generation")?;
            let expected_bytes = [
                b"FWPV\x00\x02".as_slice(),
                generation.to_be_bytes().as_slice(),
                b"\x00\x00\x00\x16",
                b"FWPS\x00\x02\x00\x00\x00\x01r\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
            ]
            .concat();
            assert!(merged.data() == &data(b"r".to_vec(), None, BTreeMap::new()));
            assert_eq!(merged.generation(), generation);
            assert_eq!(merged.encoded(), expected_bytes);
        }
        assert!(old.data() == &data(b"r".to_vec(), None, BTreeMap::new()));
        assert_eq!(old.generation(), 1);
        assert_eq!(old.encoded(), old_bytes);
    }
    Ok(())
}
