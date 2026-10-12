use crate::commands::{IntentInput, IntentLimits, IntentValue, prepare_intent};
use wf_domain::policy::BackoffKind;
use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicyPatch,
};
use wf_domain::policy::installation::{InstallCommand, InstallMode};
use wf_domain::retention::RetentionOverride;
use wf_types::identity::TenantId;

use super::{Segment, canonical};

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Arguments = Vec<(&'static str, IntentValue<'static>)>;

const MIXED: [(&str, IntentValue<'_>); 9] = [
    ("z", IntentValue::Null),
    ("a", IntentValue::Bytes(b"\0\xff")),
    ("flag", IntentValue::True),
    ("neg", IntentValue::Signed(-1)),
    ("u", IntentValue::Unsigned(u64::MAX)),
    ("empty", IntentValue::List(&[])),
    ("text", IntentValue::String("é")),
    ("zero", IntentValue::Float(-0.0)),
    ("false", IntentValue::False),
];
const NESTED_MAP: [(&str, IntentValue<'_>); 2] = [
    ("b", IntentValue::Bytes(b"")),
    ("a", IntentValue::String("")),
];
const NESTED_LIST: [IntentValue<'_>; 3] = [
    IntentValue::Map(&NESTED_MAP),
    IntentValue::Null,
    IntentValue::Unsigned(0),
];
const FORWARD: [IntentValue<'_>; 2] = [IntentValue::Unsigned(1), IntentValue::Unsigned(2)];
const REVERSE: [IntentValue<'_>; 2] = [IntentValue::Unsigned(2), IntentValue::Unsigned(1)];

fn case(
    name: &str,
) -> Result<(&'static [u8], &'static str, Arguments), Box<dyn std::error::Error>> {
    let arguments = match name {
        "empty" | "absent" | "tenant_change" | "operation_change" => Vec::new(),
        "mixed" => MIXED.to_vec(),
        "reordered" => MIXED.iter().rev().copied().collect(),
        "nested" => vec![("items", IntentValue::List(&NESTED_LIST))],
        "null" => vec![("payload", IntentValue::Null)],
        "empty_bytes" => vec![("payload", IntentValue::Bytes(b""))],
        "zero" => vec![("payload", IntentValue::Unsigned(0))],
        "positive_zero" => vec![("n", IntentValue::Float(0.0))],
        "negative_zero" => vec![("n", IntentValue::Float(-0.0))],
        "signed_zero" => vec![("n", IntentValue::Signed(0))],
        "unsigned_zero" => vec![("n", IntentValue::Unsigned(0))],
        "string_zero" => vec![("n", IntentValue::String("0"))],
        "list_forward" => vec![("items", IntentValue::List(&FORWARD))],
        "list_reverse" => vec![("items", IntentValue::List(&REVERSE))],
        "unicode_keys" => vec![
            ("é", IntentValue::Null),
            ("e\u{301}", IntentValue::Null),
            ("a", IntentValue::Null),
            ("aa", IntentValue::Null),
            ("", IntentValue::Null),
        ],
        _ => return Err(format!("unknown independent vector {name}").into()),
    };
    let tenant: &[u8] = match name {
        "mixed" | "reordered" => b"t\0\xff",
        "nested" => "租户".as_bytes(),
        "tenant_change" => b"other",
        _ => b"tenant",
    };
    let operation = match name {
        "nested" => "atomic_group_batch",
        "operation_change" => "complete",
        _ => "create",
    };
    Ok((tenant, operation, arguments))
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 15)]),
            ]
        })
        .collect()
}

#[test]
fn every_tag_and_nested_order_match_independent_bytes_and_hashes() -> TestResult {
    let mut seen = 0;
    for row in include_str!("../../../../tests/intent/v1-vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let (tenant, operation, arguments) = case(fields[0])?;
        let tenant = TenantId::new(tenant.to_vec())?;
        let input = IntentInput {
            tenant: &tenant,
            operation,
            arguments: &arguments,
        };
        let exact = IntentLimits::new(fields[1].len() / 2, fields[3].parse()?, fields[4].parse()?)?;
        let plan = canonical(input, exact)?;
        let mut bytes = Vec::new();
        plan.write(|part| bytes.extend_from_slice(part));
        assert_eq!(hex(&bytes), fields[1], "{} bytes", fields[0]);
        assert_eq!(
            hex(prepare_intent(input, exact)?.digest()),
            fields[2],
            "{} hash",
            fields[0]
        );
        seen += 1;
    }
    assert_eq!(seen, 18);
    Ok(())
}

#[test]
fn preparation_borrows_large_payload_and_caches_only_bounded_metadata() -> TestResult {
    let tenant = TenantId::new(b"tenant".to_vec())?;
    let payload = vec![7; 1_000_000];
    let arguments = [("payload", IntentValue::Bytes(&payload))];
    let plan = canonical(
        IntentInput {
            tenant: &tenant,
            operation: "create",
            arguments: &arguments,
        },
        IntentLimits::default(),
    )?;
    assert!(plan.segments.iter().any(|segment| matches!(segment,
        Segment::Borrowed(bytes) if bytes.len() == payload.len() && bytes.as_ptr() == payload.as_ptr())));
    assert_eq!(plan.values, 13);
    assert!(plan.segments.len() <= 3 * plan.values + 1);
    let mut emitted = 0;
    plan.write(|bytes| emitted += bytes.len());
    assert_eq!(emitted, plan.bytes);
    assert!(plan.bytes > payload.len());
    Ok(())
}

fn complete_backoff(kind: BackoffKind, base_ms: u64, max_ms: u64, jitter_pct: u8) -> BackoffPatch {
    BackoffPatch {
        kind: Some(kind),
        base_ms: Some(base_ms),
        max_ms: Some(max_ms),
        jitter_pct: Some(jitter_pct),
    }
}

fn rich_policy_patch() -> PolicyPatch {
    let mut patch = PolicyPatch {
        version: FieldPatch::Set(PolicyVersion::Bytes(b"\0\xff".to_vec())),
        max_active_ms: FieldPatch::Set(31_536_000_000),
        retry: FieldPatch::Set(RetryPatch {
            max_retries: Some(0),
            backoff: Some(complete_backoff(BackoffKind::Fixed, 0, 7, 100)),
            exhausted_to: Some(b"\xff".to_vec()),
        }),
        retention: FieldPatch::Set(RetentionOverride {
            ttl_ms: Some(1),
            history_max_events: Some(1),
        }),
        ..PolicyPatch::default()
    };
    patch.states.insert(
        b"\0\xff".to_vec(),
        StateChange::Patch(StatePolicyPatch {
            mode: FieldPatch::Set(StateMode::Fifo),
            retention: FieldPatch::Set(RetentionOverride::default()),
            ..StatePolicyPatch::default()
        }),
    );
    patch.states.insert(
        b"ready".to_vec(),
        StateChange::Patch(StatePolicyPatch {
            mode: FieldPatch::Set(StateMode::Parallel),
            retry: FieldPatch::Set(RetryPatch {
                max_retries: Some(1_000),
                backoff: Some(complete_backoff(BackoffKind::None, 0, 0, 0)),
                exhausted_to: Some(b"done".to_vec()),
            }),
            retention: FieldPatch::Set(RetentionOverride {
                ttl_ms: Some(31_536_000_000),
                history_max_events: Some(1_000_000),
            }),
        }),
    );
    patch
}

fn clear_policy_patch() -> PolicyPatch {
    let mut patch = PolicyPatch {
        version: FieldPatch::Clear,
        max_active_ms: FieldPatch::Clear,
        retry: FieldPatch::Clear,
        retention: FieldPatch::Clear,
        ..PolicyPatch::default()
    };
    patch.states.insert(b"ready".to_vec(), StateChange::Remove);
    patch.states.insert(
        b"\0\xff".to_vec(),
        StateChange::Patch(StatePolicyPatch {
            mode: FieldPatch::Clear,
            retry: FieldPatch::Set(RetryPatch {
                backoff: Some(BackoffPatch::default()),
                ..RetryPatch::default()
            }),
            retention: FieldPatch::Clear,
        }),
    );
    patch
}

fn policy_case(name: &str) -> Result<InstallCommand, Box<dyn std::error::Error>> {
    let mut command = InstallCommand::low_level(b"t\xff".to_vec(), PolicyPatch::default());
    match name {
        "policy_empty" => {}
        "policy_number_zero" | "policy_number_max" => {
            command.mode = InstallMode::Replace;
            command.expected_generation = Some(0);
            command.patch.version =
                FieldPatch::Set(PolicyVersion::Number(if name == "policy_number_zero" {
                    0
                } else {
                    u64::MAX
                }));
        }
        "policy_replace" => {
            command.mode = InstallMode::Replace;
            command.expected_generation = Some(0);
            command.patch = rich_policy_patch();
        }
        "policy_clear" => {
            command.expected_generation = Some(1);
            command.patch = clear_policy_patch();
        }
        "policy_linear" | "policy_exponential" => {
            let kind = if name == "policy_linear" {
                BackoffKind::Linear
            } else {
                BackoffKind::Exponential
            };
            command.patch.retry = FieldPatch::Set(RetryPatch {
                backoff: Some(complete_backoff(kind, 11, 100, 0)),
                ..RetryPatch::default()
            });
        }
        _ => return Err(format!("unknown independent policy intent vector {name}").into()),
    }
    Ok(command)
}

#[test]
fn policy_registry_matches_all_independent_bytes_hashes_and_exact_limits() -> TestResult {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let mut seen = 0;
    for row in include_str!("../../../../tests/intent/policy-set-v1-vectors.tsv")
        .lines()
        .filter(|line| !line.starts_with('#'))
    {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 5);
        let command = policy_case(fields[0])?;
        let bytes = fields[1].len() / 2;
        let values = fields[3].parse()?;
        let depth = fields[4].parse()?;
        let limits = IntentLimits::new(bytes, values, depth)?;
        let plan = super::policy::canonical(&tenant, &command, limits)?;
        let mut emitted = Vec::new();
        plan.write(|part| emitted.extend_from_slice(part));
        assert_eq!(plan.values, values, "{} values", fields[0]);
        assert_eq!(hex(&emitted), fields[1], "{} bytes", fields[0]);
        assert_eq!(
            hex(crate::commands::prepare_policy_set_intent(&tenant, &command, limits)?.digest()),
            fields[2],
            "{} hash",
            fields[0]
        );
        for (limits, error) in [
            (
                IntentLimits::new(bytes - 1, values, depth)?,
                crate::error::PreparationError::ByteLimit,
            ),
            (
                IntentLimits::new(bytes, values - 1, depth)?,
                crate::error::PreparationError::ValueLimit,
            ),
            (
                IntentLimits::new(bytes, values, depth - 1)?,
                crate::error::PreparationError::DepthLimit,
            ),
        ] {
            assert_eq!(
                crate::commands::prepare_policy_set_intent(&tenant, &command, limits),
                Err(error),
                "{} bound",
                fields[0]
            );
        }
        seen += 1;
    }
    assert_eq!(seen, 7);
    Ok(())
}

#[test]
fn policy_registry_borrows_payload_state_keys_and_identity_without_full_frame() -> TestResult {
    let tenant = TenantId::new(b"\0\xff".to_vec())?;
    let mut command = policy_case("policy_empty")?;
    command.patch.version = FieldPatch::Set(PolicyVersion::Bytes(vec![77; 1_000_000]));
    command
        .patch
        .states
        .insert(b"binary\0\xff".to_vec(), StateChange::Remove);
    let FieldPatch::Set(PolicyVersion::Bytes(payload)) = &command.patch.version else {
        return Err("missing payload fixture".into());
    };
    let (state, _) = command
        .patch
        .states
        .first_key_value()
        .ok_or("missing state fixture")?;
    let plan = super::policy::canonical(&tenant, &command, IntentLimits::default())?;
    for borrowed in [
        payload.as_slice(),
        command.workflow_type.as_slice(),
        tenant.as_bytes(),
        state.as_slice(),
    ] {
        assert!(plan.segments.iter().any(|segment| matches!(segment,
            Segment::Borrowed(bytes) if bytes.len() == borrowed.len() && bytes.as_ptr() == borrowed.as_ptr())));
    }
    assert!(plan.segments.len() <= 3 * plan.values + 1);
    let mut emitted = 0;
    plan.write(|bytes| emitted += bytes.len());
    assert_eq!(emitted, plan.bytes);
    assert!(plan.bytes > payload.len());
    Ok(())
}
