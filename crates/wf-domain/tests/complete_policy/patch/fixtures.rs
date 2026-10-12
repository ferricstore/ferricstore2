//! Independent complete presence fixture for normalized core Patch tests.

use std::collections::BTreeMap;

use wf_domain::policy::complete::{
    ApprovalsData, AuditData, CompletePolicyData, CompleteStatePolicyData, DenialAuditMode,
    EffectsData, GovernanceData, GovernanceMode, LimitData,
};
use wf_domain::policy::definition::{PolicyVersion, StateMode};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::RetentionOverride;

use super::super::data;

pub(super) fn preserved_core_patch_data() -> CompletePolicyData {
    let mut value = data(
        b"r".to_vec(),
        Some(PolicyVersion::Number(0)),
        BTreeMap::from([(
            b"queued".to_vec(),
            CompleteStatePolicyData {
                mode: Some(StateMode::Fifo),
                retry: Some(RetryOverride::default()),
                retention: Some(RetentionOverride::default()),
                governance: Some(GovernanceData {
                    mode: None,
                    effects: None,
                    audit: None,
                    limits: None,
                    budgets: None,
                    circuits: None,
                    approvals: None,
                }),
            },
        )]),
    );
    value.max_active_ms = Some(1_000);
    value.retry = Some(RetryOverride {
        max_retries: Some(7),
        backoff: Some(Backoff {
            kind: BackoffKind::Fixed,
            base_ms: 17,
            max_ms: 33,
            jitter_pct: 4,
        }),
        exhausted_to: Some(b"dead".to_vec()),
    });
    value.retention = Some(RetentionOverride {
        ttl_ms: Some(1),
        history_max_events: Some(64),
    });
    value.indexed_attributes = Some(vec![b"z".to_vec(), b"a".to_vec()]);
    value.indexed_state_meta = Some(b"m".to_vec());
    value.governance = Some(GovernanceData {
        mode: Some(GovernanceMode::Minimal),
        effects: Some(EffectsData {
            allowed: Some(vec![b"z".to_vec(), b"a".to_vec()]),
            denied: Some(Vec::new()),
            require_idempotency_key: Some(false),
        }),
        audit: Some(AuditData {
            effect_events: Some(false),
            denials: Some(DenialAuditMode::Off),
        }),
        limits: Some(BTreeMap::from([(
            b"zero".to_vec(),
            LimitData {
                limit: 0,
                enforcement: None,
                lease_size: None,
            },
        )])),
        budgets: Some(BTreeMap::new()),
        circuits: Some(BTreeMap::new()),
        approvals: Some(ApprovalsData {
            effects: Some(vec![b"z".to_vec(), b"a".to_vec()]),
            states: Some(Vec::new()),
            assignees: Some(vec![b"z".to_vec(), b"a".to_vec()]),
            timeout_ms: Some(1),
        }),
    });
    value
}
