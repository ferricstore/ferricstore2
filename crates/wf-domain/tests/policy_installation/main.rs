//! Source policy defaults, recursive patch, replace and authoritative generation.
#![forbid(unsafe_code)]

mod state_admission;

use std::collections::BTreeMap;

use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicyPatch, WorkflowDefinition,
};
use wf_domain::policy::installation::{
    InstallCommand, InstallMode, PolicyError, PolicyLimits, PolicyValue, decide_install,
    prepare_install,
};
use wf_domain::policy::{BackoffKind, RetryOverride};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::request::PolicyRejection;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn commit(
    existing: Option<&PolicyValue>,
    migration: u64,
    command: &InstallCommand,
) -> Result<PolicyValue, Box<dyn std::error::Error>> {
    let candidate = prepare_install(existing, migration, command, PolicyLimits::default())?;
    let value = candidate.bind_bytes(
        b"verified-snapshot",
        b"verified-envelope".to_vec(),
        PolicyLimits::default(),
    )?;
    let plan = decide_install(
        existing,
        migration,
        command,
        &value,
        PolicyLimits::default(),
    )?;
    assert_eq!(plan.value(), &value);
    assert_eq!(
        plan.expected_generation(),
        existing.map_or(0, PolicyValue::generation)
    );
    Ok(value)
}

#[test]
fn policy_exposes_type_state_retry_retention_and_max_active_defaults() -> TestResult {
    let command = InstallCommand::low_level(
        b"flow-policy".to_vec(),
        PolicyPatch {
            max_active_ms: FieldPatch::Set(120_000),
            retry: FieldPatch::Set(RetryPatch {
                max_retries: Some(5),
                exhausted_to: Some(b"failed".to_vec()),
                backoff: Some(BackoffPatch {
                    kind: Some(BackoffKind::Fixed),
                    base_ms: Some(10_000),
                    max_ms: Some(30_000),
                    jitter_pct: Some(0),
                }),
            }),
            retention: FieldPatch::Set(RetentionOverride {
                ttl_ms: Some(60_000),
                history_max_events: Some(512),
            }),
            states: BTreeMap::from([(
                b"charge_card".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    retry: FieldPatch::Set(RetryPatch {
                        max_retries: Some(2),
                        exhausted_to: Some(b"payment_failed".to_vec()),
                        ..RetryPatch::default()
                    }),
                    retention: FieldPatch::Set(RetentionOverride {
                        ttl_ms: Some(30_000),
                        history_max_events: Some(256),
                    }),
                    ..StatePolicyPatch::default()
                }),
            )]),
            ..PolicyPatch::default()
        },
    );
    let value = commit(None, 0, &command)?;
    assert_eq!(value.generation(), 1);
    assert_eq!(value.policy().max_active_ms(), Some(120_000));
    let type_retry = value
        .policy()
        .resolve_retry(None, &RetryOverride::default())?;
    assert_eq!(type_retry.max_retries, 5);
    assert_eq!(value.policy().resolve_retention(None).ttl_ms, 60_000);
    assert_eq!(
        value.policy().resolve_retention(None).history_max_events,
        512
    );
    let state = value
        .policy()
        .resolve_retry(Some(b"charge_card"), &RetryOverride::default())?;
    assert_eq!(state.max_retries, 2);
    assert_eq!(state.backoff.kind, BackoffKind::Fixed);
    assert_eq!(state.backoff.base_ms, 10_000);
    assert_eq!(state.exhausted_to, b"payment_failed");
    let retention = value.policy().resolve_retention(Some(b"charge_card"));
    assert_eq!(
        (retention.ttl_ms, retention.history_max_events),
        (30_000, 256)
    );
    assert_eq!(value.policy().state_mode(b"unknown"), StateMode::Parallel);
    Ok(())
}

#[test]
fn low_level_patch_preserves_independent_fields_deep_backoff_and_explicit_state_delete()
-> TestResult {
    let first = InstallCommand::low_level(
        b"generation-merge".to_vec(),
        PolicyPatch {
            max_active_ms: FieldPatch::Set(1_000),
            states: BTreeMap::from([(
                b"queued".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Fifo),
                    retry: FieldPatch::Set(RetryPatch {
                        max_retries: Some(7),
                        backoff: Some(BackoffPatch {
                            kind: Some(BackoffKind::Fixed),
                            base_ms: Some(123),
                            ..BackoffPatch::default()
                        }),
                        ..RetryPatch::default()
                    }),
                    ..StatePolicyPatch::default()
                }),
            )]),
            ..PolicyPatch::default()
        },
    );
    assert_eq!(first.mode, InstallMode::Patch);
    let first = commit(None, 0, &first)?;
    let second = InstallCommand::low_level(
        b"generation-merge".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([
                (
                    b"queued".to_vec(),
                    StateChange::Patch(StatePolicyPatch {
                        retry: FieldPatch::Set(RetryPatch {
                            exhausted_to: Some(b"dead".to_vec()),
                            backoff: Some(BackoffPatch {
                                max_ms: Some(456),
                                ..BackoffPatch::default()
                            }),
                            ..RetryPatch::default()
                        }),
                        ..StatePolicyPatch::default()
                    }),
                ),
                (
                    b"review".to_vec(),
                    StateChange::Patch(StatePolicyPatch {
                        mode: FieldPatch::Set(StateMode::Fifo),
                        ..StatePolicyPatch::default()
                    }),
                ),
            ]),
            ..PolicyPatch::default()
        },
    );
    let second = commit(Some(&first), 0, &second)?;
    assert_eq!(second.generation(), 2);
    assert_eq!(second.policy().max_active_ms(), Some(1_000));
    assert_eq!(second.policy().state_mode(b"queued"), StateMode::Fifo);
    assert_eq!(second.policy().state_mode(b"review"), StateMode::Fifo);
    let retry = second
        .policy()
        .resolve_retry(Some(b"queued"), &RetryOverride::default())?;
    assert_eq!(retry.max_retries, 7);
    assert_eq!(retry.exhausted_to, b"dead");
    assert_eq!(
        (
            retry.backoff.kind,
            retry.backoff.base_ms,
            retry.backoff.max_ms
        ),
        (BackoffKind::Fixed, 123, 456)
    );
    let delete = InstallCommand::low_level(
        b"generation-merge".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([(b"queued".to_vec(), StateChange::Remove)]),
            ..PolicyPatch::default()
        },
    );
    let deleted = commit(Some(&second), 0, &delete)?;
    assert_eq!(deleted.policy().state_mode(b"queued"), StateMode::Parallel);
    assert_eq!(deleted.policy().states().len(), 1);
    assert_eq!(second.policy().states().len(), 2);
    Ok(())
}

#[test]
fn definition_install_replaces_and_explicit_patch_preserves_while_cas_rejects() -> TestResult {
    let old = InstallCommand::low_level(
        b"ordered".to_vec(),
        PolicyPatch {
            max_active_ms: FieldPatch::Set(1_000),
            version: FieldPatch::Set(PolicyVersion::Bytes(b"customer-v1".to_vec())),
            states: BTreeMap::from([(
                b"old".to_vec(),
                StateChange::Patch(StatePolicyPatch::default()),
            )]),
            ..PolicyPatch::default()
        },
    );
    let old = commit(None, 0, &old)?;
    let definition = WorkflowDefinition::new(
        b"ordered".to_vec(),
        b"queued".to_vec(),
        BTreeMap::from([
            (
                b"queued".to_vec(),
                StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Fifo),
                    retry: FieldPatch::Set(RetryPatch {
                        max_retries: Some(2),
                        ..RetryPatch::default()
                    }),
                    ..StatePolicyPatch::default()
                },
            ),
            (
                b"review".to_vec(),
                StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Parallel),
                    ..StatePolicyPatch::default()
                },
            ),
        ]),
        PolicyLimits::default(),
    )?;
    assert_eq!(definition.initial_state(), b"queued");
    let install = definition.install_command();
    assert_eq!(install.mode, InstallMode::Replace);
    let installed = commit(Some(&old), 3, &install)?;
    assert_eq!(
        installed.generation(),
        4,
        "migration high-water is authoritative"
    );
    assert_eq!(installed.policy().max_active_ms(), None);
    assert_eq!(installed.policy().states().len(), 2);
    assert!(installed.policy().version().is_none());
    let mut patch = install.clone();
    patch.mode = InstallMode::Patch;
    let patched = commit(Some(&old), 0, &patch)?;
    assert_eq!(patched.policy().max_active_ms(), Some(1_000));
    assert_eq!(patched.policy().states().len(), 3);
    patch.expected_generation = Some(0);
    assert_eq!(
        prepare_install(Some(&old), 0, &patch, PolicyLimits::default()),
        Err(PolicyError::StaleGeneration)
    );
    patch.expected_generation = Some(1);
    assert_eq!(commit(Some(&old), 0, &patch)?.generation(), 2);
    assert_eq!(old.generation(), 1);
    Ok(())
}

#[test]
fn final_encoded_policy_bounds_and_changed_preparation_are_checked_at_commit() -> TestResult {
    let command = InstallCommand::low_level(b"type".to_vec(), PolicyPatch::default());
    let candidate = prepare_install(None, 0, &command, PolicyLimits::default())?;
    assert_eq!(
        candidate
            .clone()
            .bind_bytes(&vec![0; 262_145], vec![], PolicyLimits::default()),
        Err(PolicyError::SnapshotTooLarge)
    );
    assert_eq!(
        candidate
            .clone()
            .bind_bytes(b"snapshot", vec![0; 263_169], PolicyLimits::default()),
        Err(PolicyError::EncodedPolicyTooLarge)
    );
    let prepared =
        candidate.bind_bytes(b"snapshot", b"envelope".to_vec(), PolicyLimits::default())?;
    assert_eq!(
        decide_install(None, 1, &command, &prepared, PolicyLimits::default()),
        Err(PolicyError::PreparedMismatch)
    );
    let mut changed = command.clone();
    changed.patch.max_active_ms = FieldPatch::Set(1);
    assert_eq!(
        decide_install(None, 0, &changed, &prepared, PolicyLimits::default()),
        Err(PolicyError::PreparedMismatch)
    );
    assert_eq!(
        prepare_install(
            None,
            9_007_199_254_740_991,
            &command,
            PolicyLimits::default()
        ),
        Err(PolicyError::GenerationExhausted)
    );
    let malformed = InstallCommand::low_level(
        b"type".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([(
                b"running".to_vec(),
                StateChange::Patch(StatePolicyPatch::default()),
            )]),
            ..PolicyPatch::default()
        },
    );
    assert!(prepare_install(None, 0, &malformed, PolicyLimits::default()).is_err());
    Ok(())
}

#[test]
fn empty_patch_checks_actual_type_and_final_merged_cardinality_before_copy() -> TestResult {
    let too_large = InstallCommand::low_level(vec![b'x'; 262_145], PolicyPatch::default());
    assert_eq!(
        too_large.admit(PolicyLimits::default()),
        Err(PolicyError::SnapshotTooLarge)
    );
    let first = InstallCommand::low_level(
        b"type".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([(
                b"a".to_vec(),
                StateChange::Patch(StatePolicyPatch::default()),
            )]),
            ..PolicyPatch::default()
        },
    );
    let first = commit(None, 0, &first)?;
    let add = InstallCommand::low_level(
        b"type".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([(
                b"b".to_vec(),
                StateChange::Patch(StatePolicyPatch::default()),
            )]),
            ..PolicyPatch::default()
        },
    );
    assert_eq!(
        prepare_install(Some(&first), 0, &add, PolicyLimits::new(1)?),
        Err(PolicyError::StateBudget)
    );
    let swap = InstallCommand::low_level(
        b"type".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([
                (b"a".to_vec(), StateChange::Remove),
                (
                    b"b".to_vec(),
                    StateChange::Patch(StatePolicyPatch::default()),
                ),
            ]),
            ..PolicyPatch::default()
        },
    );
    assert!(prepare_install(Some(&first), 0, &swap, PolicyLimits::new(2)?).is_ok());
    Ok(())
}
