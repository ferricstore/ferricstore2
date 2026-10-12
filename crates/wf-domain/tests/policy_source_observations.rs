//! Exact pure observations from pinned policy-generation and DSL declarations.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use wf_domain::policy::BackoffKind;
use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, RetryPatch, StateChange, StateMode, StatePolicyPatch,
    WorkflowDefinition,
};
use wf_domain::policy::installation::{
    InstallCommand, InstallMode, PolicyError, PolicyLimits, PolicyValue, decide_install,
    prepare_install,
};

type TestError = Box<dyn std::error::Error>;
type TestResult = Result<(), TestError>;

fn commit(
    existing: Option<&PolicyValue>,
    command: &InstallCommand,
) -> Result<PolicyValue, TestError> {
    let limits = PolicyLimits::default();
    let value = prepare_install(existing, 0, command, limits)?.bind_bytes(
        b"snapshot-length-fixture",
        b"envelope-length-fixture".to_vec(),
        limits,
    )?;
    assert_eq!(
        decide_install(existing, 0, command, &value, limits)?.value(),
        &value
    );
    Ok(value)
}

#[test]
fn source_generation_one_two_and_stale_cas_preserve_original_values() -> TestResult {
    // d4199dfa1b750255:14-26 and dea49ead483519f8:28-48.
    let mut command = InstallCommand::low_level(
        b"generation-cas".to_vec(),
        PolicyPatch {
            max_active_ms: FieldPatch::Set(1_000),
            ..PolicyPatch::default()
        },
    );
    let first = commit(None, &command)?;
    assert_eq!(first.generation(), 1);
    assert_eq!(first.policy().max_active_ms(), Some(1_000));
    command.expected_generation = Some(1);
    command.patch.max_active_ms = FieldPatch::Set(2_000);
    let second = commit(Some(&first), &command)?;
    assert_eq!(second.generation(), 2);
    assert_eq!(second.policy().max_active_ms(), Some(2_000));
    command.patch.max_active_ms = FieldPatch::Set(3_000);
    assert_eq!(
        prepare_install(Some(&second), 0, &command, PolicyLimits::default()),
        Err(PolicyError::StaleGeneration)
    );
    assert_eq!(second.generation(), 2);
    assert_eq!(second.policy().max_active_ms(), Some(2_000));
    assert_eq!(first.generation(), 1);
    Ok(())
}

#[test]
fn explicit_empty_replace_resets_omitted_policy_and_states() -> TestResult {
    // 17d8471407b0c04a:320-332.
    let first = commit(
        None,
        &InstallCommand::low_level(
            b"generation-replace".to_vec(),
            PolicyPatch {
                max_active_ms: FieldPatch::Set(1_000),
                states: BTreeMap::from([(
                    b"queued".to_vec(),
                    StateChange::Patch(StatePolicyPatch {
                        mode: FieldPatch::Set(StateMode::Fifo),
                        ..StatePolicyPatch::default()
                    }),
                )]),
                ..PolicyPatch::default()
            },
        ),
    )?;
    let mut replace =
        InstallCommand::low_level(b"generation-replace".to_vec(), PolicyPatch::default());
    replace.mode = InstallMode::Replace;
    let second = commit(Some(&first), &replace)?;
    assert_eq!(first.generation(), 1);
    assert_eq!(first.policy().max_active_ms(), Some(1_000));
    assert_eq!(first.policy().state_mode(b"queued"), StateMode::Fifo);
    assert_eq!(second.generation(), 2);
    assert_eq!(second.policy().max_active_ms(), None);
    assert!(second.policy().states().is_empty());
    Ok(())
}

#[test]
fn billing_definition_emits_every_declared_retry_and_execution_mode() -> TestResult {
    // 3cfa6de43c4341b3:425-443. Embedded API call remains a later counterpart.
    let created = StatePolicyPatch {
        mode: FieldPatch::Set(StateMode::Parallel),
        retry: FieldPatch::Set(RetryPatch {
            max_retries: Some(8),
            exhausted_to: Some(b"failed".to_vec()),
            backoff: Some(BackoffPatch {
                kind: Some(BackoffKind::Fixed),
                base_ms: Some(1_000),
                max_ms: Some(60_000),
                jitter_pct: None,
            }),
        }),
        ..StatePolicyPatch::default()
    };
    let charged = StatePolicyPatch {
        mode: FieldPatch::Set(StateMode::Parallel),
        ..StatePolicyPatch::default()
    };
    let states = BTreeMap::from([
        (b"created".to_vec(), created),
        (b"charged".to_vec(), charged),
    ]);
    let definition = WorkflowDefinition::new(
        b"billing".to_vec(),
        b"created".to_vec(),
        states.clone(),
        PolicyLimits::default(),
    )?;
    let command = definition.install_command();
    assert_eq!(command.workflow_type, b"billing");
    assert_eq!(command.mode, InstallMode::Replace);
    assert_eq!(
        command.patch.states,
        states
            .into_iter()
            .map(|(name, patch)| (name, StateChange::Patch(patch)))
            .collect()
    );
    assert_eq!(command.patch.max_active_ms, FieldPatch::Unchanged);
    Ok(())
}

#[test]
fn ordered_definition_emits_declared_modes_and_explicit_patch_options() -> TestResult {
    // d780726ed1b18ce8:445-459 and 0319c4450653a32b:461-468.
    let states = BTreeMap::from([
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
    ]);
    let definition = WorkflowDefinition::new(
        b"ordered".to_vec(),
        b"queued".to_vec(),
        states.clone(),
        PolicyLimits::default(),
    )?;
    let mut command = definition.install_command();
    assert_eq!(command.workflow_type, b"ordered");
    assert_eq!(command.mode, InstallMode::Replace);
    assert_eq!(
        command.patch.states,
        states
            .into_iter()
            .map(|(name, patch)| (name, StateChange::Patch(patch)))
            .collect()
    );
    command.mode = InstallMode::Patch;
    command.patch.max_active_ms = FieldPatch::Set(30_000);
    let installed = commit(None, &command)?;
    assert_eq!(command.mode, InstallMode::Patch);
    assert_eq!(installed.policy().max_active_ms(), Some(30_000));
    assert_eq!(installed.policy().state_mode(b"queued"), StateMode::Fifo);
    assert_eq!(
        installed.policy().state_mode(b"review"),
        StateMode::Parallel
    );
    Ok(())
}
