//! Policy identity compares typed caller fields, including business-invalid ones.
//! Source admission, authorization and stored policy resolution remain separate.

#![forbid(unsafe_code)]

use wf_application::commands::{
    IntentInput, IntentLimits, IntentValue, prepare_intent, prepare_policy_set_intent,
};
use wf_application::error::PreparationError;
use wf_domain::policy::BackoffKind;
use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicyPatch,
};
use wf_domain::policy::installation::{InstallCommand, InstallMode};
use wf_domain::retention::RetentionOverride;
use wf_types::identity::TenantId;

fn empty_command() -> InstallCommand {
    InstallCommand::low_level(b"t\xff".to_vec(), PolicyPatch::default())
}

fn assert_distinct(commands: &[InstallCommand]) -> Result<(), Box<dyn std::error::Error>> {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let mut seen = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        let intent = prepare_policy_set_intent(&tenant, command, IntentLimits::default())?;
        assert!(!seen.contains(&intent), "collapsed caller variant {index}");
        seen.push(intent);
    }
    Ok(())
}

#[test]
fn empty_registry_entry_equals_explicit_generic_slots() -> Result<(), Box<dyn std::error::Error>> {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let command = empty_command();
    let patch = [("states", IntentValue::List(&[]))];
    let arguments = [
        ("workflow_type", IntentValue::Bytes(&command.workflow_type)),
        ("patch", IntentValue::Map(&patch)),
        ("mode", IntentValue::String("patch")),
    ];
    let expected = prepare_intent(
        IntentInput {
            tenant: &tenant,
            operation: "workflow.policy_set",
            arguments: &arguments,
        },
        IntentLimits::default(),
    )?;
    assert_eq!(
        prepare_policy_set_intent(&tenant, &command, IntentLimits::default())?,
        expected
    );
    let other_tenant = TenantId::new(b"\xff\0".to_vec())?;
    assert_ne!(
        prepare_policy_set_intent(&other_tenant, &command, IntentLimits::default())?,
        expected
    );
    Ok(())
}

#[test]
fn guards_versions_presence_and_scalar_categories_remain_distinct()
-> Result<(), Box<dyn std::error::Error>> {
    let base = empty_command();
    let mut commands = vec![base.clone()];
    for expected in [Some(0), Some(u64::MAX)] {
        let mut command = base.clone();
        command.expected_generation = expected;
        commands.push(command);
    }
    let mut replacement = base.clone();
    replacement.mode = InstallMode::Replace;
    commands.push(replacement);
    for version in [
        FieldPatch::Clear,
        FieldPatch::Set(PolicyVersion::Bytes(Vec::new())),
        FieldPatch::Set(PolicyVersion::Bytes(b"0".to_vec())),
        FieldPatch::Set(PolicyVersion::Bytes(b"\0\xff".to_vec())),
        FieldPatch::Set(PolicyVersion::Number(0)),
        FieldPatch::Set(PolicyVersion::Number(u64::MAX)),
    ] {
        let mut command = base.clone();
        command.patch.version = version;
        commands.push(command);
    }
    for max_active in [
        FieldPatch::Clear,
        FieldPatch::Set(0),
        FieldPatch::Set(u64::MAX),
    ] {
        let mut command = base.clone();
        command.patch.max_active_ms = max_active;
        commands.push(command);
    }
    for retry in [FieldPatch::Clear, FieldPatch::Set(RetryPatch::default())] {
        let mut command = base.clone();
        command.patch.retry = retry;
        commands.push(command);
    }
    for retention in [
        FieldPatch::Clear,
        FieldPatch::Set(RetentionOverride::default()),
    ] {
        let mut command = base.clone();
        command.patch.retention = retention;
        commands.push(command);
    }
    let mut other_type = base;
    other_type.workflow_type = b"\xfft".to_vec();
    commands.push(other_type);
    assert_distinct(&commands)
}

fn retry_variants() -> Vec<RetryPatch> {
    let mut patches = vec![RetryPatch::default()];
    for max_retries in [0, u16::MAX] {
        patches.push(RetryPatch {
            max_retries: Some(max_retries),
            ..RetryPatch::default()
        });
    }
    patches.push(RetryPatch {
        backoff: Some(BackoffPatch::default()),
        ..RetryPatch::default()
    });
    for kind in [
        BackoffKind::None,
        BackoffKind::Fixed,
        BackoffKind::Linear,
        BackoffKind::Exponential,
    ] {
        patches.push(RetryPatch {
            backoff: Some(BackoffPatch {
                kind: Some(kind),
                ..BackoffPatch::default()
            }),
            ..RetryPatch::default()
        });
    }
    for number in [0, u64::MAX] {
        for backoff in [
            BackoffPatch {
                base_ms: Some(number),
                ..BackoffPatch::default()
            },
            BackoffPatch {
                max_ms: Some(number),
                ..BackoffPatch::default()
            },
        ] {
            patches.push(RetryPatch {
                backoff: Some(backoff),
                ..RetryPatch::default()
            });
        }
    }
    for jitter_pct in [0, u8::MAX] {
        patches.push(RetryPatch {
            backoff: Some(BackoffPatch {
                jitter_pct: Some(jitter_pct),
                ..BackoffPatch::default()
            }),
            ..RetryPatch::default()
        });
    }
    for target in [b"".as_slice(), b"\0\xff".as_slice(), b"\xff\0".as_slice()] {
        patches.push(RetryPatch {
            exhausted_to: Some(target.to_vec()),
            ..RetryPatch::default()
        });
    }
    patches
}

fn retention_variants() -> [RetentionOverride; 5] {
    [
        RetentionOverride::default(),
        RetentionOverride {
            ttl_ms: Some(0),
            ..RetentionOverride::default()
        },
        RetentionOverride {
            ttl_ms: Some(u64::MAX),
            ..RetentionOverride::default()
        },
        RetentionOverride {
            history_max_events: Some(0),
            ..RetentionOverride::default()
        },
        RetentionOverride {
            history_max_events: Some(u32::MAX),
            ..RetentionOverride::default()
        },
    ]
}

#[test]
fn every_recursive_optional_field_preserves_empty_zero_max_and_kind()
-> Result<(), Box<dyn std::error::Error>> {
    let commands: Vec<_> = retry_variants()
        .into_iter()
        .map(|retry| {
            let mut command = empty_command();
            command.patch.retry = FieldPatch::Set(retry);
            command
        })
        .collect();
    assert_distinct(&commands)?;
    let commands: Vec<_> = retention_variants()
        .into_iter()
        .map(|retention| {
            let mut command = empty_command();
            command.patch.retention = FieldPatch::Set(retention);
            command
        })
        .collect();
    assert_distinct(&commands)
}

fn state_command(change: StateChange) -> InstallCommand {
    let mut command = empty_command();
    command.patch.states.insert(b"\0\xff".to_vec(), change);
    command
}

#[test]
fn state_actions_and_all_nested_fields_remain_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let mut commands = vec![empty_command(), state_command(StateChange::Remove)];
    let mut patches = vec![StatePolicyPatch::default()];
    for mode in [
        FieldPatch::Clear,
        FieldPatch::Set(StateMode::Parallel),
        FieldPatch::Set(StateMode::Fifo),
    ] {
        patches.push(StatePolicyPatch {
            mode,
            ..StatePolicyPatch::default()
        });
    }
    patches.push(StatePolicyPatch {
        retry: FieldPatch::Clear,
        ..StatePolicyPatch::default()
    });
    for retry in retry_variants() {
        patches.push(StatePolicyPatch {
            retry: FieldPatch::Set(retry),
            ..StatePolicyPatch::default()
        });
    }
    patches.push(StatePolicyPatch {
        retention: FieldPatch::Clear,
        ..StatePolicyPatch::default()
    });
    for retention in retention_variants() {
        patches.push(StatePolicyPatch {
            retention: FieldPatch::Set(retention),
            ..StatePolicyPatch::default()
        });
    }
    commands.extend(
        patches
            .into_iter()
            .map(|patch| state_command(StateChange::Patch(patch))),
    );
    assert_distinct(&commands)
}

#[test]
fn state_map_insertion_order_is_irrelevant_and_binary_names_are_preserved()
-> Result<(), Box<dyn std::error::Error>> {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let rows = [
        (b"\0\xff".to_vec(), StateChange::Remove),
        (
            b"ready".to_vec(),
            StateChange::Patch(StatePolicyPatch::default()),
        ),
    ];
    let mut forward = empty_command();
    forward.patch.states.extend(rows.clone());
    let mut reverse = empty_command();
    reverse.patch.states.extend(rows.into_iter().rev());
    assert_eq!(
        prepare_policy_set_intent(&tenant, &forward, IntentLimits::default())?,
        prepare_policy_set_intent(&tenant, &reverse, IntentLimits::default())?
    );
    reverse.patch.states.remove(b"\0\xff".as_slice());
    reverse
        .patch
        .states
        .insert(b"\xff\0".to_vec(), StateChange::Remove);
    assert_distinct(&[forward, reverse])
}

#[test]
fn state_work_checks_count_before_key_bytes_and_obeys_exact_value_depth()
-> Result<(), Box<dyn std::error::Error>> {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let command = state_command(StateChange::Remove);
    assert!(
        prepare_policy_set_intent(&tenant, &command, IntentLimits::new(1_048_576, 24, 6)?).is_ok()
    );
    assert_eq!(
        prepare_policy_set_intent(&tenant, &command, IntentLimits::new(1_048_576, 23, 6)?),
        Err(PreparationError::ValueLimit)
    );
    assert_eq!(
        prepare_policy_set_intent(&tenant, &command, IntentLimits::new(1_048_576, 24, 5)?),
        Err(PreparationError::DepthLimit)
    );
    let mut large = empty_command();
    large
        .patch
        .states
        .insert(vec![0; 1_048_577], StateChange::Remove);
    assert_eq!(
        prepare_policy_set_intent(&tenant, &large, IntentLimits::default()),
        Err(PreparationError::ByteLimit)
    );
    for number in 0_u64..4_000 {
        large
            .patch
            .states
            .insert(number.to_be_bytes().to_vec(), StateChange::Remove);
    }
    assert_eq!(
        prepare_policy_set_intent(&tenant, &large, IntentLimits::default()),
        Err(PreparationError::ValueLimit)
    );
    Ok(())
}
