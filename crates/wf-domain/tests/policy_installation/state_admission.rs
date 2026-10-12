//! Source state-policy admission, defaults and guard precedence.

use super::{
    BTreeMap, FieldPatch, InstallCommand, InstallMode, PolicyError, PolicyLimits, PolicyPatch,
    PolicyRejection, PolicyVersion, StateChange, StateMode, StatePolicyPatch, TestResult,
    WorkflowDefinition, commit, prepare_install,
};

#[test]
fn present_null_state_mode_rejects_patch_and_replace_before_candidate_creation() -> TestResult {
    let initial = InstallCommand::low_level(
        b"job".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([(
                b"queued".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Fifo),
                    ..StatePolicyPatch::default()
                }),
            )]),
            ..PolicyPatch::default()
        },
    );
    let stored = commit(None, 0, &initial)?;
    let results = [InstallMode::Patch, InstallMode::Replace].map(|mode| {
        let mut command = InstallCommand::low_level(
            b"job".to_vec(),
            PolicyPatch {
                states: BTreeMap::from([(
                    b"queued".to_vec(),
                    StateChange::Patch(StatePolicyPatch {
                        mode: FieldPatch::Clear,
                        ..StatePolicyPatch::default()
                    }),
                )]),
                ..PolicyPatch::default()
            },
        );
        command.mode = mode;
        prepare_install(Some(&stored), 0, &command, PolicyLimits::default())
    });
    assert_eq!(
        results,
        [
            Err(PolicyError::InvalidStateMode),
            Err(PolicyError::InvalidStateMode)
        ]
    );
    assert_eq!(stored.policy().state_mode(b"queued"), StateMode::Fifo);
    assert_eq!(stored.generation(), 1);
    Ok(())
}

#[test]
fn present_null_state_mode_rejects_borrowed_admission_and_definition() {
    let policy = StatePolicyPatch {
        mode: FieldPatch::Clear,
        ..StatePolicyPatch::default()
    };
    let command = InstallCommand::low_level(
        b"job".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([(b"queued".to_vec(), StateChange::Patch(policy.clone()))]),
            ..PolicyPatch::default()
        },
    );
    let admission = command.admit(PolicyLimits::default());
    let definition = WorkflowDefinition::new(
        b"job".to_vec(),
        b"queued".to_vec(),
        BTreeMap::from([(b"queued".to_vec(), policy)]),
        PolicyLimits::default(),
    );
    assert_eq!(
        (admission, definition),
        (
            Err(PolicyError::InvalidStateMode),
            Err(PolicyError::InvalidStateMode)
        )
    );
}

#[test]
fn invalid_state_mode_has_exact_source_text_and_business_classification() -> TestResult {
    let error = PolicyError::InvalidStateMode;
    assert_eq!(
        error.to_string(),
        "ERR flow state mode must be parallel or fifo"
    );
    assert!(!error.is_apply_failure());
    assert_eq!(PolicyRejection::new(error)?.error(), error);
    Ok(())
}

#[test]
fn omitted_parallel_and_fifo_modes_preserve_normalized_and_stored_defaults() -> TestResult {
    let initial = InstallCommand::low_level(
        b"job".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([(
                b"queued".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Set(StateMode::Fifo),
                    ..StatePolicyPatch::default()
                }),
            )]),
            ..PolicyPatch::default()
        },
    );
    let stored = commit(None, 0, &initial)?;
    for (mode, normalized) in [
        (FieldPatch::Unchanged, None),
        (
            FieldPatch::Set(StateMode::Parallel),
            Some(StateMode::Parallel),
        ),
        (FieldPatch::Set(StateMode::Fifo), Some(StateMode::Fifo)),
    ] {
        let policy = StatePolicyPatch {
            mode,
            ..StatePolicyPatch::default()
        };
        let command = InstallCommand::low_level(
            b"job".to_vec(),
            PolicyPatch {
                states: BTreeMap::from([(b"queued".to_vec(), StateChange::Patch(policy.clone()))]),
                ..PolicyPatch::default()
            },
        );
        command.admit(PolicyLimits::default())?;
        let value = commit(None, 0, &command)?;
        assert_eq!(
            value.policy().states()[b"queued".as_slice()].mode,
            normalized
        );
        assert_eq!(
            value.policy().state_mode(b"queued"),
            normalized.unwrap_or(StateMode::Parallel)
        );
        let patched = commit(Some(&stored), 0, &command)?;
        assert_eq!(
            patched.policy().state_mode(b"queued"),
            normalized.unwrap_or(StateMode::Fifo)
        );
        let definition = WorkflowDefinition::new(
            b"job".to_vec(),
            b"queued".to_vec(),
            BTreeMap::from([(b"queued".to_vec(), policy)]),
            PolicyLimits::default(),
        )?;
        let defined = commit(None, 0, &definition.install_command())?;
        assert_eq!(
            defined.policy().states()[b"queued".as_slice()].mode,
            normalized
        );
    }
    assert_eq!(stored.policy().state_mode(b"queued"), StateMode::Fifo);
    Ok(())
}

#[test]
fn stale_cas_still_precedes_present_null_state_mode_admission() -> TestResult {
    let initial = InstallCommand::low_level(b"job".to_vec(), PolicyPatch::default());
    let stored = commit(None, 0, &initial)?;
    let mut command = InstallCommand::low_level(
        b"job".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([(
                b"queued".to_vec(),
                StateChange::Patch(StatePolicyPatch {
                    mode: FieldPatch::Clear,
                    ..StatePolicyPatch::default()
                }),
            )]),
            ..PolicyPatch::default()
        },
    );
    command.expected_generation = Some(0);
    assert_eq!(
        prepare_install(Some(&stored), 0, &command, PolicyLimits::default()),
        Err(PolicyError::StaleGeneration)
    );
    command.expected_generation = Some(1);
    assert_eq!(
        prepare_install(Some(&stored), 0, &command, PolicyLimits::default()),
        Err(PolicyError::InvalidStateMode)
    );
    assert_eq!(stored.generation(), 1);
    assert!(stored.policy().states().is_empty());
    Ok(())
}

#[test]
fn replacement_null_state_policy_rejects_before_candidate_copy() -> TestResult {
    let mut command = InstallCommand::low_level(
        b"job".to_vec(),
        PolicyPatch {
            states: BTreeMap::from([(
                b"queued".to_vec(),
                StateChange::Patch(StatePolicyPatch::default()),
            )]),
            ..PolicyPatch::default()
        },
    );
    let stored = commit(None, 0, &command)?;
    let before = stored.clone();
    command.mode = InstallMode::Replace;
    command
        .patch
        .states
        .insert(b"queued".to_vec(), StateChange::Remove);
    let limits = PolicyLimits::default();
    let admission = command.admit(limits);
    let candidates =
        [None, Some(&stored)].map(|existing| prepare_install(existing, 0, &command, limits));
    let error = PolicyError::InvalidStatePolicy;
    assert_eq!(
        error.to_string(),
        "ERR flow state policy must be a map or keyword list"
    );
    assert!(!error.is_apply_failure());
    assert_eq!(PolicyRejection::new(error)?.error(), error);
    assert_eq!(stored, before);
    assert_eq!(stored.generation(), 1);
    assert_eq!(
        (admission, candidates),
        (Err(error), [Err(error), Err(error)])
    );
    Ok(())
}

#[test]
fn patch_state_deletion_and_replacement_cas_keep_source_boundaries() -> TestResult {
    let mut command = InstallCommand::low_level(
        b"job".to_vec(),
        PolicyPatch {
            version: FieldPatch::Set(PolicyVersion::Number(7)),
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
    );
    let stored = commit(None, 0, &command)?;
    let before = stored.clone();
    command.patch = PolicyPatch {
        states: BTreeMap::from([(b"queued".to_vec(), StateChange::Remove)]),
        ..PolicyPatch::default()
    };
    command.admit(PolicyLimits::default())?;
    let deleted = commit(Some(&stored), 0, &command)?;
    assert_eq!(deleted.generation(), 2);
    assert!(deleted.policy().states().is_empty());
    assert_eq!(deleted.policy().version(), Some(&PolicyVersion::Number(7)));
    assert_eq!(deleted.policy().max_active_ms(), Some(1_000));
    command.patch.states.insert(
        b"queued".to_vec(),
        StateChange::Patch(StatePolicyPatch::default()),
    );
    let patched = commit(Some(&stored), 0, &command)?;
    assert_eq!(patched.policy().state_mode(b"queued"), StateMode::Fifo);
    command.mode = InstallMode::Replace;
    let empty_state = commit(Some(&stored), 0, &command)?;
    assert_eq!(empty_state.policy().states().len(), 1);
    assert_eq!(
        empty_state.policy().states()[b"queued".as_slice()].mode,
        None
    );
    assert_eq!(
        empty_state.policy().state_mode(b"queued"),
        StateMode::Parallel
    );
    assert_eq!(empty_state.policy().version(), None);
    assert_eq!(empty_state.policy().max_active_ms(), None);
    command.patch.states.clear();
    let omitted = commit(Some(&stored), 0, &command)?;
    assert!(omitted.policy().states().is_empty());
    command
        .patch
        .states
        .insert(b"queued".to_vec(), StateChange::Remove);
    command.expected_generation = Some(0);
    assert_eq!(
        prepare_install(Some(&stored), 0, &command, PolicyLimits::default()),
        Err(PolicyError::StaleGeneration)
    );
    command.expected_generation = Some(1);
    let coherent = prepare_install(Some(&stored), 0, &command, PolicyLimits::default());
    assert_eq!(stored, before);
    assert_eq!(stored.generation(), 1);
    assert_eq!(coherent, Err(PolicyError::InvalidStatePolicy));
    Ok(())
}
