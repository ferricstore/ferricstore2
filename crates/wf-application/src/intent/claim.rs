//! Flat borrowed public claim comparison; no clock, scope or apply authority.

use wf_domain::claim::caller::{
    ClaimPartitionMode, ClaimReplyMode, ClaimValuesMode, NormalizedClaimDue,
};
use wf_domain::workflow::request::ClientIntent;
use wf_types::identity::TenantId;

use super::{COUNT_BYTES, CanonicalPlan, HEADER_BYTES, PREFIX, Segment, digest, format_length};
use crate::commands::{IntentLimits, IntentValue};
use crate::error::PreparationError;

pub(crate) fn prepare_claim_due(
    tenant: &TenantId,
    request: &NormalizedClaimDue<'_>,
    limits: IntentLimits,
) -> Result<ClientIntent, PreparationError> {
    let options = [
        request.priority().is_some(),
        request.payload_max_bytes().is_some(),
        request.governance_limit_scope().is_some(),
        request.governance_shard_id().is_some(),
        request.requested_now_ms().is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    let values = options
        .checked_mul(2)
        .and_then(|count| count.checked_add(45))
        .and_then(|count| count.checked_add(request.states().1.len()))
        .and_then(|count| count.checked_add(request.partitions().1.len()))
        .and_then(|count| count.checked_add(request.values().1.len()))
        .ok_or(PreparationError::LengthOverflow)?;
    if values > limits.values {
        return Err(PreparationError::ValueLimit);
    }
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
            arguments(plan, request, depth)?;
            plan.entry("normalization_version", IntentValue::String("1"), depth)?;
            plan.entry(
                "operation",
                IntentValue::String("workflow.claim_due"),
                depth,
            )?;
            plan.entry("schema_version", IntentValue::String("1"), depth)?;
            plan.entry("tenant_id", IntentValue::Bytes(tenant.as_bytes()), depth)
        },
    )?;
    digest(&plan)
}

fn arguments<'a>(
    plan: &mut CanonicalPlan<'a>,
    request: &'a NormalizedClaimDue<'_>,
    depth: usize,
) -> Result<(), PreparationError> {
    let first = [
        (
            "governance_limit_scope",
            request.governance_limit_scope().map(IntentValue::Bytes),
        ),
        (
            "governance_shard_id",
            request.governance_shard_id().map(IntentValue::Unsigned),
        ),
        ("lease_ms", Some(IntentValue::Unsigned(request.lease_ms()))),
        ("limit", Some(IntentValue::Unsigned(request.limit()))),
        (
            "now_ms",
            request.requested_now_ms().map(IntentValue::Unsigned),
        ),
    ];
    let reply = match request.reply_mode() {
        ClaimReplyMode::Records => "records",
        ClaimReplyMode::Jobs => "jobs",
        ClaimReplyMode::Compact => "jobs_compact",
        ClaimReplyMode::CompactAttrs => "jobs_compact_attrs",
        ClaimReplyMode::CompactState => "jobs_compact_state",
        ClaimReplyMode::CompactStateAttrs => "jobs_compact_state_attrs",
    };
    let middle = [
        (
            "payload",
            Some(if request.payload_enabled() {
                IntentValue::True
            } else {
                IntentValue::False
            }),
        ),
        (
            "payload_max_bytes",
            request.payload_max_bytes().map(IntentValue::Unsigned),
        ),
        ("priority", request.priority().map(IntentValue::Unsigned)),
        (
            "reclaim_expired",
            Some(if request.reclaim_expired() {
                IntentValue::True
            } else {
                IntentValue::False
            }),
        ),
        (
            "reclaim_ratio",
            Some(IntentValue::Unsigned(request.reclaim_ratio())),
        ),
        ("reply_mode", Some(IntentValue::String(reply))),
    ];
    plan.explicit_map(
        &[
            ("governance_limit_scope", first[0].1.is_some()),
            ("governance_shard_id", first[1].1.is_some()),
            ("lease_ms", true),
            ("limit", true),
            ("now_ms", first[4].1.is_some()),
            ("partitions", true),
            ("payload", true),
            ("payload_max_bytes", middle[1].1.is_some()),
            ("priority", middle[2].1.is_some()),
            ("reclaim_expired", true),
            ("reclaim_ratio", true),
            ("reply_mode", true),
            ("states", true),
            ("values", true),
            ("worker", true),
            ("workflow_type", true),
        ],
        depth,
        |plan, depth| {
            for (key, value) in first {
                optional(plan, key, value, depth)?;
            }
            plan.value(IntentValue::String("partitions"), depth)?;
            partitions(plan, request, depth)?;
            for (key, value) in middle {
                optional(plan, key, value, depth)?;
            }
            plan.value(IntentValue::String("states"), depth)?;
            states(plan, request, depth)?;
            plan.value(IntentValue::String("values"), depth)?;
            values(plan, request, depth)?;
            plan.entry("worker", IntentValue::Bytes(request.worker()), depth)?;
            plan.entry(
                "workflow_type",
                IntentValue::Bytes(request.workflow_type()),
                depth,
            )
        },
    )
}

fn states<'a>(
    plan: &mut CanonicalPlan<'a>,
    request: &'a NormalizedClaimDue<'_>,
    depth: usize,
) -> Result<(), PreparationError> {
    let (any, items) = request.states();
    plan.explicit_map(&[("any", true), ("names", true)], depth, |plan, depth| {
        plan.entry(
            "any",
            if any {
                IntentValue::True
            } else {
                IntentValue::False
            },
            depth,
        )?;
        plan.value(IntentValue::String("names"), depth)?;
        names(plan, items, depth)
    })
}

fn partitions<'a>(
    plan: &mut CanonicalPlan<'a>,
    request: &'a NormalizedClaimDue<'_>,
    depth: usize,
) -> Result<(), PreparationError> {
    let (mode, items) = request.partitions();
    let mode = match mode {
        ClaimPartitionMode::Auto => "auto",
        ClaimPartitionMode::Any => "any",
        ClaimPartitionMode::Global => "global",
        ClaimPartitionMode::Only => "only",
    };
    plan.explicit_map(&[("keys", true), ("mode", true)], depth, |plan, depth| {
        plan.value(IntentValue::String("keys"), depth)?;
        names(plan, items, depth)?;
        plan.entry("mode", IntentValue::String(mode), depth)
    })
}

fn values<'a>(
    plan: &mut CanonicalPlan<'a>,
    request: &'a NormalizedClaimDue<'_>,
    depth: usize,
) -> Result<(), PreparationError> {
    let (mode, items) = request.values();
    let mode = match mode {
        ClaimValuesMode::Omitted => "omitted",
        ClaimValuesMode::All => "all",
        ClaimValuesMode::Only => "only",
    };
    plan.explicit_map(&[("mode", true), ("names", true)], depth, |plan, depth| {
        plan.entry("mode", IntentValue::String(mode), depth)?;
        plan.value(IntentValue::String("names"), depth)?;
        names(plan, items, depth)
    })
}

fn names<'a>(
    plan: &mut CanonicalPlan<'a>,
    items: &[&'a [u8]],
    depth: usize,
) -> Result<(), PreparationError> {
    let (header, before) = plan.begin(depth)?;
    plan.children(items.len(), depth)?;
    let mut minimum = items
        .len()
        .checked_mul(HEADER_BYTES)
        .and_then(|bytes| bytes.checked_add(COUNT_BYTES))
        .ok_or(PreparationError::LengthOverflow)?;
    plan.check_bytes(minimum)?;
    for item in items {
        minimum = minimum
            .checked_add(item.len())
            .ok_or(PreparationError::LengthOverflow)?;
        plan.check_bytes(minimum)?;
    }
    plan.push(Segment::Number(format_length(items.len())?), COUNT_BYTES)?;
    for item in items {
        plan.value(IntentValue::Bytes(item), depth + 1)?;
    }
    plan.finish(header, before, 7)
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
