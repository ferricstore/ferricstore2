//! Cohesive normalized recursive policy patch helpers.

use crate::retention::RetentionOverride;

use super::RetryOverride;
use super::definition::{FieldPatch, PolicyPatch, PolicySnapshot, RetryPatch, StateChange};

pub(super) fn merge(policy: &mut PolicySnapshot, patch: &PolicyPatch) {
    update(&mut policy.version, &patch.version);
    update(&mut policy.max_active_ms, &patch.max_active_ms);
    retry(&mut policy.retry, &patch.retry);
    retention(&mut policy.retention, &patch.retention);
    // Distinct map keys make removal order irrelevant; remove before inserting
    // so candidate state count stays within admitted old/final cardinality.
    for (state, change) in &patch.states {
        if matches!(change, StateChange::Remove) {
            policy.states.remove(state);
        }
    }
    for (state, change) in &patch.states {
        match change {
            StateChange::Remove => {}
            StateChange::Patch(patch) => {
                let state = policy.states.entry(state.clone()).or_default();
                update(&mut state.mode, &patch.mode);
                retry(&mut state.retry, &patch.retry);
                retention(&mut state.retention, &patch.retention);
            }
        }
    }
}

fn update<T: Clone>(value: &mut Option<T>, patch: &FieldPatch<T>) {
    match patch {
        FieldPatch::Unchanged => {}
        FieldPatch::Set(next) => *value = Some(next.clone()),
        FieldPatch::Clear => *value = None,
    }
}

fn retry(value: &mut RetryOverride, patch: &FieldPatch<RetryPatch>) {
    match patch {
        FieldPatch::Unchanged => {}
        FieldPatch::Clear => *value = RetryOverride::default(),
        FieldPatch::Set(patch) => {
            if let Some(limit) = patch.max_retries {
                value.max_retries = Some(limit);
            }
            if let Some(target) = &patch.exhausted_to {
                value.exhausted_to = Some(target.clone());
            }
            if let Some(patch) = patch.backoff {
                let mut backoff = value.backoff.unwrap_or_default();
                if let Some(kind) = patch.kind {
                    backoff.kind = kind;
                }
                if let Some(base) = patch.base_ms {
                    backoff.base_ms = base;
                }
                if let Some(max) = patch.max_ms {
                    backoff.max_ms = max;
                }
                if let Some(jitter) = patch.jitter_pct {
                    backoff.jitter_pct = jitter;
                }
                value.backoff = Some(backoff);
            }
        }
    }
}

fn retention(value: &mut RetentionOverride, patch: &FieldPatch<RetentionOverride>) {
    match patch {
        FieldPatch::Unchanged => {}
        FieldPatch::Clear => *value = RetentionOverride::default(),
        FieldPatch::Set(patch) => {
            if let Some(ttl) = patch.ttl_ms {
                value.ttl_ms = Some(ttl);
            }
            if let Some(events) = patch.history_max_events {
                value.history_max_events = Some(events);
            }
        }
    }
}
