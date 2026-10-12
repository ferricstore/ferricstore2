//! Independent complete value and byte observations for normalized core Patch.

use std::collections::BTreeMap;

use wf_domain::policy::complete::{
    CompleteCorePolicyPatchInput, CompletePolicyData, CompletePolicyEncodingError,
    CompletePolicyValue, CompleteStatePolicyData, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits, prepare_complete_core_policy_patch, prepare_complete_policy_value_v2,
    verify_complete_policy_value_v2,
};
use wf_domain::policy::definition::{FieldPatch, PolicyPatch, PolicyVersion, StateMode};
use wf_domain::policy::installation::PolicyError;
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::{RetentionError, RetentionOverride};

use super::super::{TestResult, candidate, data};
use super::fixtures::preserved_core_patch_data;

pub(super) fn assert_source_case8_core_policy(
    old: &CompletePolicyValue,
    merged: &CompletePolicyValue,
) {
    let mut expected_old = data(
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
    expected_old.max_active_ms = Some(1_000);
    let mut expected = data(
        b"r".to_vec(),
        None,
        BTreeMap::from([
            (
                b"queued".to_vec(),
                CompleteStatePolicyData {
                    mode: Some(StateMode::Fifo),
                    retry: Some(RetryOverride {
                        max_retries: Some(7),
                        backoff: None,
                        exhausted_to: Some(b"dead".to_vec()),
                    }),
                    retention: None,
                    governance: None,
                },
            ),
            (
                b"review".to_vec(),
                CompleteStatePolicyData {
                    mode: Some(StateMode::Fifo),
                    retry: None,
                    retention: None,
                    governance: None,
                },
            ),
        ]),
    );
    expected.max_active_ms = Some(1_000);
    let original_bytes = [
        b"FWPV".as_slice(),
        &2_u16.to_be_bytes(),
        &1_u64.to_be_bytes(),
        &50_u32.to_be_bytes(),
        b"FWPS\x00\x02\x00\x00\x00\x01r\x00\x01",
        &1_000_u64.to_be_bytes(),
        b"\x00\x00\x00\x00\x00\x01\x00\x00\x00\x06queued",
        b"\x01\x01\x01\x01\x00\x07\x00\x00\x00\x00",
        b"\x00\x00\x00",
    ]
    .concat();
    let expected_bytes = [
        b"FWPV".as_slice(),
        &2_u16.to_be_bytes(),
        &2_u64.to_be_bytes(),
        &73_u32.to_be_bytes(),
        b"FWPS\x00\x02\x00\x00\x00\x01r\x00\x01",
        &1_000_u64.to_be_bytes(),
        b"\x00\x00\x00\x00\x00\x02\x00\x00\x00\x06queued",
        b"\x01\x01\x01\x01\x00\x07\x00\x01\x00\x00\x00\x04dead\x00\x00",
        b"\x00\x00\x00\x06review\x01\x01\x00\x00\x00",
        b"\x00\x00\x00",
    ]
    .concat();
    assert!(merged.data() == &expected);
    assert_eq!(merged.generation(), 2);
    assert_eq!(merged.encoded(), expected_bytes);
    assert!(old.data() == &expected_old);
    assert_eq!(old.generation(), 1);
    assert_eq!(old.encoded(), original_bytes);
}

pub(super) fn assert_core_patch_moved_input_owners(
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    for had_existing in [false, true] {
        let original = data(b"r".to_vec(), None, BTreeMap::new());
        let old = prepare_complete_policy_value_v2(
            candidate(1, original, profile, charge)?,
            profile,
            charge,
        )?;
        let workflow_type = b"r".to_vec();
        let type_owner = workflow_type.as_ptr();
        let version = b"v".to_vec();
        let version_owner = version.as_ptr();
        let existing = if had_existing { Some(&old) } else { None };
        let merged = prepare_complete_core_policy_patch(
            existing,
            0,
            CompleteCorePolicyPatchInput {
                workflow_type,
                expected_generation: None,
                patch: PolicyPatch {
                    version: FieldPatch::Set(PolicyVersion::Bytes(version)),
                    ..PolicyPatch::default()
                },
            },
            64,
            profile,
            charge,
        )?;
        let expected = data(
            b"r".to_vec(),
            Some(PolicyVersion::Bytes(b"v".to_vec())),
            BTreeMap::new(),
        );
        let generation: u64 = if had_existing { 2 } else { 1 };
        let expected_bytes = [
            b"FWPV".as_slice(),
            &2_u16.to_be_bytes(),
            &generation.to_be_bytes(),
            &28_u32.to_be_bytes(),
            b"FWPS\x00\x02\x00\x00\x00\x01r\x01\x00\x00\x00\x00\x01v",
            b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
        ]
        .concat();
        let old_bytes = [
            b"FWPV".as_slice(),
            &2_u16.to_be_bytes(),
            &1_u64.to_be_bytes(),
            &22_u32.to_be_bytes(),
            b"FWPS\x00\x02\x00\x00\x00\x01r",
            b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00",
        ]
        .concat();
        assert!(merged.data() == &expected);
        assert_eq!(merged.generation(), generation);
        assert_eq!(merged.encoded(), expected_bytes);
        assert!(old.data() == &data(b"r".to_vec(), None, BTreeMap::new()));
        assert_eq!(old.generation(), 1);
        assert_eq!(old.encoded(), old_bytes);
        let Some(PolicyVersion::Bytes(version)) = merged.data().version.as_ref() else {
            return Err("missing moved byte version".into());
        };
        assert_eq!(merged.data().workflow_type.as_ptr(), type_owner);
        assert_eq!(version.as_ptr(), version_owner);
    }
    Ok(())
}

pub(super) fn assert_core_patch_retry_overlay(
    case: u8,
    state_level: bool,
    old: &CompletePolicyValue,
    merged: &CompletePolicyValue,
    original_bytes: &[u8],
) -> TestResult {
    let previous = Backoff {
        kind: BackoffKind::Fixed,
        base_ms: 17,
        max_ms: 33,
        jitter_pct: 4,
    };
    let old_retry = if matches!(case, 0 | 3 | 5) {
        Some(RetryOverride {
            max_retries: Some(7),
            backoff: if case == 0 { Some(previous) } else { None },
            exhausted_to: if case == 3 {
                Some(b"dead".to_vec())
            } else {
                None
            },
        })
    } else {
        None
    };
    let new_retry = match case {
        0 => Some(RetryOverride {
            max_retries: Some(7),
            backoff: Some(Backoff {
                jitter_pct: 25,
                ..previous
            }),
            exhausted_to: None,
        }),
        1 => Some(RetryOverride {
            max_retries: None,
            backoff: Some(Backoff {
                kind: BackoffKind::Exponential,
                base_ms: 1_000,
                max_ms: 30_000,
                jitter_pct: 20,
            }),
            exhausted_to: None,
        }),
        2 => Some(RetryOverride::default()),
        3 => Some(RetryOverride {
            max_retries: Some(7),
            backoff: None,
            exhausted_to: Some(b"dead".to_vec()),
        }),
        4 => None,
        5 => Some(RetryOverride {
            max_retries: Some(8),
            backoff: None,
            exhausted_to: None,
        }),
        _ => return Err("unexpected retry expectation".into()),
    };
    let mut expected_old = preserved_core_patch_data();
    let mut expected_new = preserved_core_patch_data();
    if state_level {
        old.data()
            .states
            .get(b"queued".as_slice())
            .ok_or("missing old queued state")?;
        merged
            .data()
            .states
            .get(b"queued".as_slice())
            .ok_or("missing merged queued state")?;
        expected_old
            .states
            .get_mut(b"queued".as_slice())
            .ok_or("old fixture state")?
            .retry = old_retry;
        expected_new
            .states
            .get_mut(b"queued".as_slice())
            .ok_or("new fixture state")?
            .retry = new_retry;
    } else {
        expected_old.retry = old_retry;
        expected_new.retry = new_retry;
    }
    assert!(merged.data() == &expected_new);
    assert_eq!(merged.generation(), 2);
    assert!(old.data() == &expected_old);
    assert_eq!(old.generation(), 1);
    assert_eq!(old.encoded(), original_bytes);
    Ok(())
}

pub(super) fn assert_core_patch_history_result(
    result: Result<CompletePolicyValue, CompletePolicyEncodingError>,
    old: &CompletePolicyValue,
    expected: (&CompletePolicyData, CompletePolicyData),
    original_bytes: &[u8],
    case: (bool, Option<RetentionError>),
    profile: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult {
    let (expected_old, mut expected_new) = expected;
    let (state_level, error) = case;
    if let Some(error) = error {
        let expected = PolicySchemaError::Policy(PolicyError::Retention(error));
        assert_eq!(
            result.err(),
            Some(CompletePolicyEncodingError::Schema(expected))
        );
    } else {
        let merged = result?;
        let expected = Some(RetentionOverride {
            ttl_ms: None,
            history_max_events: Some(64),
        });
        if state_level {
            expected_new
                .states
                .get_mut(b"queued".as_slice())
                .ok_or("queued")?
                .retention = expected;
        } else {
            expected_new.retention = expected;
        }
        assert!(merged.data() == &expected_new);
        assert_eq!(merged.generation(), 2);
        verify_complete_policy_value_v2(&merged, profile, charge)?;
    }
    assert!(old.data() == expected_old);
    assert_eq!(old.generation(), 1);
    assert_eq!(old.encoded(), original_bytes);
    Ok(())
}
