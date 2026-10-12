//! Private policy-set comparison mapping; no source admission or apply authority.

use wf_domain::policy::definition::{
    BackoffPatch, FieldPatch, PolicyPatch, PolicyVersion, RetryPatch, StateChange, StateMode,
    StatePolicyPatch,
};
use wf_domain::policy::installation::{InstallCommand, InstallMode};
use wf_domain::retention::RetentionOverride;
use wf_types::identity::TenantId;

use super::{COUNT_BYTES, CanonicalPlan, HEADER_BYTES, PREFIX, Segment, format_length};
use crate::commands::{IntentLimits, IntentValue};
use crate::error::PreparationError;

pub(super) fn canonical<'a>(
    tenant: &'a TenantId,
    command: &'a InstallCommand,
    limits: IntentLimits,
) -> Result<CanonicalPlan<'a>, PreparationError> {
    let mut plan = CanonicalPlan {
        segments: Vec::new(),
        bytes: 0,
        values: 0,
        limits,
    };
    plan.push(Segment::Borrowed(PREFIX), PREFIX.len())?;
    plan.explicit_map(
        &[
            ("args", true),
            ("normalization_version", true),
            ("operation", true),
            ("schema_version", true),
            ("tenant_id", true),
        ],
        1,
        |plan, depth| {
            plan.value(IntentValue::String("args"), depth)?;
            arguments(plan, command, depth)?;
            plan.entry("normalization_version", IntentValue::String("1"), depth)?;
            plan.entry(
                "operation",
                IntentValue::String("workflow.policy_set"),
                depth,
            )?;
            plan.entry("schema_version", IntentValue::String("1"), depth)?;
            plan.entry("tenant_id", IntentValue::Bytes(tenant.as_bytes()), depth)
        },
    )?;
    Ok(plan)
}

fn arguments<'a>(
    plan: &mut CanonicalPlan<'a>,
    command: &'a InstallCommand,
    depth: usize,
) -> Result<(), PreparationError> {
    plan.explicit_map(
        &[
            ("expected_generation", command.expected_generation.is_some()),
            ("mode", true),
            ("patch", true),
            ("workflow_type", true),
        ],
        depth,
        |plan, depth| {
            optional(
                plan,
                "expected_generation",
                command.expected_generation.map(IntentValue::Unsigned),
                depth,
            )?;
            let mode = match command.mode {
                InstallMode::Patch => "patch",
                InstallMode::Replace => "replace",
            };
            plan.entry("mode", IntentValue::String(mode), depth)?;
            plan.value(IntentValue::String("patch"), depth)?;
            patch(plan, &command.patch, depth)?;
            plan.entry(
                "workflow_type",
                IntentValue::Bytes(&command.workflow_type),
                depth,
            )
        },
    )
}

fn optional<'a>(
    plan: &mut CanonicalPlan<'a>,
    key: &'static str,
    value: Option<IntentValue<'a>>,
    depth: usize,
) -> Result<(), PreparationError> {
    if let Some(value) = value {
        plan.entry(key, value, depth)?;
    }
    Ok(())
}

fn present<T>(field: &FieldPatch<T>) -> bool {
    !matches!(field, FieldPatch::Unchanged)
}

fn field<'a, T>(
    plan: &mut CanonicalPlan<'a>,
    key: &'static str,
    patch: &'a FieldPatch<T>,
    depth: usize,
    write: impl FnOnce(&mut CanonicalPlan<'a>, &'a T, usize) -> Result<(), PreparationError>,
) -> Result<(), PreparationError> {
    match patch {
        FieldPatch::Unchanged => Ok(()),
        FieldPatch::Clear => plan.entry(key, IntentValue::Null, depth),
        FieldPatch::Set(value) => {
            plan.value(IntentValue::String(key), depth)?;
            write(plan, value, depth)
        }
    }
}

fn patch<'a>(
    plan: &mut CanonicalPlan<'a>,
    patch: &'a PolicyPatch,
    depth: usize,
) -> Result<(), PreparationError> {
    plan.explicit_map(
        &[
            ("max_active_ms", present(&patch.max_active_ms)),
            ("retention", present(&patch.retention)),
            ("retry", present(&patch.retry)),
            ("states", true),
            ("version", present(&patch.version)),
        ],
        depth,
        |plan, depth| {
            field(
                plan,
                "max_active_ms",
                &patch.max_active_ms,
                depth,
                |plan, number, depth| plan.value(IntentValue::Unsigned(*number), depth),
            )?;
            field(plan, "retention", &patch.retention, depth, retention)?;
            field(plan, "retry", &patch.retry, depth, retry)?;
            plan.value(IntentValue::String("states"), depth)?;
            states(plan, patch, depth)?;
            field(
                plan,
                "version",
                &patch.version,
                depth,
                |plan, version, depth| {
                    let value = match version {
                        PolicyVersion::Bytes(bytes) => IntentValue::Bytes(bytes),
                        PolicyVersion::Number(number) => IntentValue::Unsigned(*number),
                    };
                    plan.value(value, depth)
                },
            )
        },
    )
}

fn states<'a>(
    plan: &mut CanonicalPlan<'a>,
    patch: &'a PolicyPatch,
    depth: usize,
) -> Result<(), PreparationError> {
    let count = patch.states.len();
    // Every row has a map, two keys and two values, even for Remove/null.
    let minimum_values = count
        .checked_mul(5)
        .and_then(|count| count.checked_add(1))
        .and_then(|count| plan.values.checked_add(count))
        .ok_or(PreparationError::LengthOverflow)?;
    if minimum_values > plan.limits.values {
        return Err(PreparationError::ValueLimit);
    }
    // Row minimum64 = map header/count + change key/null + state key/Bytes header.
    let mut minimum_bytes = count
        .checked_mul(64)
        .and_then(|bytes| bytes.checked_add(HEADER_BYTES + COUNT_BYTES))
        .ok_or(PreparationError::LengthOverflow)?;
    plan.check_bytes(minimum_bytes)?;
    for state in patch.states.keys() {
        minimum_bytes = minimum_bytes
            .checked_add(state.len())
            .ok_or(PreparationError::LengthOverflow)?;
        plan.check_bytes(minimum_bytes)?;
    }
    let (header, before) = plan.begin(depth)?;
    plan.children(count, depth)?;
    plan.push(Segment::Number(format_length(count)?), COUNT_BYTES)?;
    for (state, change) in &patch.states {
        plan.explicit_map(
            &[("change", true), ("state", true)],
            depth + 1,
            |plan, depth| {
                plan.value(IntentValue::String("change"), depth)?;
                match change {
                    StateChange::Remove => plan.value(IntentValue::Null, depth)?,
                    StateChange::Patch(patch) => state_patch(plan, patch, depth)?,
                }
                plan.entry("state", IntentValue::Bytes(state), depth)
            },
        )?;
    }
    plan.finish(header, before, 7)
}

fn state_patch<'a>(
    plan: &mut CanonicalPlan<'a>,
    patch: &'a StatePolicyPatch,
    depth: usize,
) -> Result<(), PreparationError> {
    plan.explicit_map(
        &[
            ("mode", present(&patch.mode)),
            ("retention", present(&patch.retention)),
            ("retry", present(&patch.retry)),
        ],
        depth,
        |plan, depth| {
            field(plan, "mode", &patch.mode, depth, |plan, mode, depth| {
                let mode = match mode {
                    StateMode::Parallel => "parallel",
                    StateMode::Fifo => "fifo",
                };
                plan.value(IntentValue::String(mode), depth)
            })?;
            field(plan, "retention", &patch.retention, depth, retention)?;
            field(plan, "retry", &patch.retry, depth, retry)
        },
    )
}

fn retry<'a>(
    plan: &mut CanonicalPlan<'a>,
    patch: &'a RetryPatch,
    depth: usize,
) -> Result<(), PreparationError> {
    plan.explicit_map(
        &[
            ("backoff", patch.backoff.is_some()),
            ("exhausted_to", patch.exhausted_to.is_some()),
            ("max_retries", patch.max_retries.is_some()),
        ],
        depth,
        |plan, depth| {
            if let Some(backoff_patch) = &patch.backoff {
                plan.value(IntentValue::String("backoff"), depth)?;
                backoff(plan, backoff_patch, depth)?;
            }
            optional(
                plan,
                "exhausted_to",
                patch.exhausted_to.as_deref().map(IntentValue::Bytes),
                depth,
            )?;
            optional(
                plan,
                "max_retries",
                patch
                    .max_retries
                    .map(|value| IntentValue::Unsigned(u64::from(value))),
                depth,
            )
        },
    )
}

fn backoff<'a>(
    plan: &mut CanonicalPlan<'a>,
    patch: &'a BackoffPatch,
    depth: usize,
) -> Result<(), PreparationError> {
    plan.explicit_map(
        &[
            ("base_ms", patch.base_ms.is_some()),
            ("jitter_pct", patch.jitter_pct.is_some()),
            ("kind", patch.kind.is_some()),
            ("max_ms", patch.max_ms.is_some()),
        ],
        depth,
        |plan, depth| {
            optional(
                plan,
                "base_ms",
                patch.base_ms.map(IntentValue::Unsigned),
                depth,
            )?;
            optional(
                plan,
                "jitter_pct",
                patch
                    .jitter_pct
                    .map(|value| IntentValue::Unsigned(u64::from(value))),
                depth,
            )?;
            optional(
                plan,
                "kind",
                patch.kind.map(|kind| IntentValue::String(kind.as_str())),
                depth,
            )?;
            optional(
                plan,
                "max_ms",
                patch.max_ms.map(IntentValue::Unsigned),
                depth,
            )
        },
    )
}

fn retention<'a>(
    plan: &mut CanonicalPlan<'a>,
    patch: &'a RetentionOverride,
    depth: usize,
) -> Result<(), PreparationError> {
    plan.explicit_map(
        &[
            ("history_max_events", patch.history_max_events.is_some()),
            ("ttl_ms", patch.ttl_ms.is_some()),
        ],
        depth,
        |plan, depth| {
            optional(
                plan,
                "history_max_events",
                patch
                    .history_max_events
                    .map(|value| IntentValue::Unsigned(u64::from(value))),
                depth,
            )?;
            optional(
                plan,
                "ttl_ms",
                patch.ttl_ms.map(IntentValue::Unsigned),
                depth,
            )
        },
    )
}
