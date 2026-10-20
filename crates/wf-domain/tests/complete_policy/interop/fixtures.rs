//! Actual V2 inputs and independently packed original ETF expectations.

use super::super::{TestResult, candidate, data};
use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    ApprovalsData, AuditData, BudgetData, CircuitData, CompletePolicyData, CompletePolicyValue,
    CompleteStatePolicyData, DenialAuditMode, EffectsData, GovernanceData, GovernanceMode,
    LimitData, PolicySchemaCharge, PolicySchemaLimits, prepare_complete_policy_value_v2,
};
use wf_domain::policy::definition::{PolicyVersion, StateMode};
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::RetentionOverride;
use wf_domain::workflow::active::LimitEnforcement;

pub(super) fn value(
    generation: u64,
    input: CompletePolicyData,
    schema: PolicySchemaLimits,
) -> TestResult<CompletePolicyValue> {
    let mut setup = PolicySchemaCharge::new(schema);
    Ok(prepare_complete_policy_value_v2(
        candidate(generation, input, schema, &mut setup)?,
        schema,
        &mut setup,
    )?)
}

pub(super) fn minimal(schema: PolicySchemaLimits) -> TestResult<CompletePolicyValue> {
    value(1, data(b"r".to_vec(), None, BTreeMap::new()), schema)
}

pub(super) fn enum_variant(
    schema: PolicySchemaLimits,
    ledger: bool,
) -> TestResult<(CompletePolicyValue, Vec<u8>)> {
    let (mode, mode_name, denials, denial_name) = if ledger {
        (
            GovernanceMode::Ledger,
            "ledger",
            DenialAuditMode::Sampled,
            "sampled",
        )
    } else {
        (
            GovernanceMode::Minimal,
            "minimal",
            DenialAuditMode::Off,
            "off",
        )
    };
    let mut input = data(
        b"r".to_vec(),
        Some(PolicyVersion::Number(0)),
        BTreeMap::new(),
    );
    input.governance = Some(GovernanceData {
        mode: Some(mode),
        effects: Some(EffectsData {
            allowed: None,
            denied: None,
            require_idempotency_key: Some(true),
        }),
        audit: Some(AuditData {
            effect_events: Some(true),
            denials: Some(denials),
        }),
        limits: Some(BTreeMap::new()),
        budgets: Some(BTreeMap::new()),
        circuits: Some(BTreeMap::new()),
        approvals: Some(ApprovalsData {
            effects: None,
            states: None,
            assignees: None,
            timeout_ms: None,
        }),
    });
    let expected = envelope(
        1,
        &fields(&[
            (
                "governance",
                fields(&[
                    ("approvals", map(&[])),
                    (
                        "audit",
                        fields(&[
                            ("denials", atom(denial_name)),
                            ("effect_events", atom("true")),
                        ]),
                    ),
                    ("budgets", map(&[])),
                    ("circuits", map(&[])),
                    (
                        "effects",
                        fields(&[("require_idempotency_key", atom("true"))]),
                    ),
                    ("limits", map(&[])),
                    ("mode", atom(mode_name)),
                ]),
            ),
            ("states", map(&[])),
            ("type", binary(b"r")),
            ("version", integer(0)),
        ]),
    );
    Ok((value(1, input, schema)?, expected))
}

pub(super) fn states_wire(generation: u64, names: &[&[u8]]) -> Vec<u8> {
    let rows = names
        .iter()
        .map(|name| (binary(name), fields(&[("mode", atom("fifo"))])))
        .collect::<Vec<_>>();
    envelope(
        generation,
        &fields(&[("states", map(&rows)), ("type", binary(b"r"))]),
    )
}

pub(super) fn empty_governance() -> GovernanceData {
    GovernanceData {
        mode: None,
        effects: None,
        audit: None,
        limits: None,
        budgets: None,
        circuits: None,
        approvals: None,
    }
}

pub(super) fn rich_data() -> CompletePolicyData {
    let mut out = data(
        b"r".to_vec(),
        Some(PolicyVersion::Number(u64::MAX)),
        BTreeMap::new(),
    );
    out.max_active_ms = Some(1_000);
    out.retry = Some(RetryOverride {
        max_retries: Some(7),
        backoff: Some(Backoff {
            kind: BackoffKind::Linear,
            base_ms: 17,
            max_ms: 33,
            jitter_pct: 4,
        }),
        exhausted_to: Some(b"dead".to_vec()),
    });
    out.retention = Some(RetentionOverride {
        ttl_ms: Some(123),
        history_max_events: Some(64),
    });
    out.states.insert(
        b"a".to_vec(),
        CompleteStatePolicyData {
            mode: Some(StateMode::Fifo),
            retry: Some(RetryOverride::default()),
            retention: Some(RetentionOverride::default()),
            governance: Some(empty_governance()),
        },
    );
    out.states.insert(
        b"z".to_vec(),
        CompleteStatePolicyData {
            mode: Some(StateMode::Parallel),
            retry: Some(RetryOverride {
                max_retries: Some(9),
                backoff: Some(Backoff {
                    kind: BackoffKind::Exponential,
                    base_ms: 1,
                    max_ms: 2,
                    jitter_pct: 3,
                }),
                exhausted_to: Some(b"dead".to_vec()),
            }),
            retention: Some(RetentionOverride {
                ttl_ms: Some(456),
                history_max_events: Some(17),
            }),
            governance: Some(rich_governance()),
        },
    );
    out.indexed_attributes = Some(vec![b"a".to_vec(), b"z".to_vec()]);
    out.indexed_state_meta = Some(b"m".to_vec());
    out.governance = Some(rich_governance());
    out
}

fn rich_governance() -> GovernanceData {
    GovernanceData {
        mode: Some(GovernanceMode::Full),
        effects: Some(EffectsData {
            allowed: Some(vec![b"z".to_vec(), b"a".to_vec()]),
            denied: Some(Vec::new()),
            require_idempotency_key: Some(false),
        }),
        audit: Some(AuditData {
            effect_events: Some(false),
            denials: Some(DenialAuditMode::All),
        }),
        limits: Some(BTreeMap::from([(
            b"l".to_vec(),
            LimitData {
                limit: 0,
                enforcement: Some(LimitEnforcement::StrictGlobal),
                lease_size: Some(2),
            },
        )])),
        budgets: Some(BTreeMap::from([(
            b"b".to_vec(),
            BudgetData {
                limit: 3,
                window_ms: 4,
                enforcement: Some(LimitEnforcement::ApproximateGlobal),
                lease_size: Some(5),
                unit: Some(b"u".to_vec()),
            },
        )])),
        circuits: Some(BTreeMap::from([(
            b"c".to_vec(),
            CircuitData {
                failure_threshold: 2,
                open_ms: 3,
                window_ms: Some(4),
                min_calls: Some(5),
                failure_rate_pct: Some(6),
                latency_threshold_ms: Some(7),
                half_open_max_probes: Some(8),
                half_open_success_threshold: Some(1),
                error_classes: Some(vec![b"z".to_vec(), b"a".to_vec()]),
            },
        )])),
        approvals: Some(ApprovalsData {
            effects: Some(vec![b"z".to_vec(), b"a".to_vec()]),
            states: Some(vec![b"a".to_vec(), b"z".to_vec()]),
            assignees: Some(vec![b"y".to_vec(), b"x".to_vec()]),
            timeout_ms: Some(9),
        }),
    }
}

pub(super) fn atom(name: &str) -> Vec<u8> {
    let len = u8::try_from(name.len()).unwrap_or_default();
    assert_eq!(usize::from(len), name.len(), "literal atom length");
    let mut out = vec![119, len];
    out.extend_from_slice(name.as_bytes());
    out
}
pub(super) fn binary(bytes: &[u8]) -> Vec<u8> {
    let len = u32::try_from(bytes.len()).unwrap_or_default();
    assert_eq!(u64::from(len), bytes.len() as u64, "bounded fixture");
    let mut out = vec![109];
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    out
}
pub(super) fn integer(n: u64) -> Vec<u8> {
    if let Ok(n) = u8::try_from(n) {
        return vec![97, n];
    }
    if let Ok(n) = i32::try_from(n) {
        let mut out = vec![98];
        out.extend_from_slice(&n.to_be_bytes());
        return out;
    }
    let raw = n.to_le_bytes();
    let len = raw.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    let len = u8::try_from(len).unwrap_or_default();
    assert!((1..=8).contains(&len), "positive eight-limb integer");
    let mut out = vec![110, len, 0];
    out.extend_from_slice(&raw[..usize::from(len)]);
    out
}
pub(super) fn map(rows: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    let len = u32::try_from(rows.len()).unwrap_or_default();
    assert_eq!(u64::from(len), rows.len() as u64, "bounded rows");
    let mut out = vec![116];
    out.extend_from_slice(&len.to_be_bytes());
    for (key, value) in rows {
        out.extend_from_slice(key);
        out.extend_from_slice(value);
    }
    out
}
pub(super) fn fields(rows: &[(&str, Vec<u8>)]) -> Vec<u8> {
    map(&rows
        .iter()
        .map(|(name, value)| (atom(name), value.clone()))
        .collect::<Vec<_>>())
}
pub(super) fn names(names: &[&[u8]]) -> Vec<u8> {
    if names.is_empty() {
        return vec![106];
    }
    let len = u32::try_from(names.len()).unwrap_or_default();
    assert_eq!(u64::from(len), names.len() as u64, "bounded list");
    let mut out = vec![108];
    out.extend_from_slice(&len.to_be_bytes());
    for name in names {
        out.extend_from_slice(&binary(name));
    }
    out.push(106);
    out
}
pub(super) fn envelope(generation: u64, policy: &[u8]) -> Vec<u8> {
    let mut out = vec![131, 104, 3];
    out.extend_from_slice(&atom("flow_policy_v1"));
    out.extend_from_slice(&integer(generation));
    out.extend_from_slice(policy);
    out
}
pub(super) fn minimal_wire() -> Vec<u8> {
    envelope(1, &fields(&[("states", map(&[])), ("type", binary(b"r"))]))
}

pub(super) fn rich_wire(generation: u64) -> Vec<u8> {
    let a = fields(&[
        ("governance", map(&[])),
        ("mode", atom("fifo")),
        ("retention", map(&[])),
        ("retry", map(&[])),
    ]);
    let z = fields(&[
        ("governance", wire_governance()),
        ("mode", atom("parallel")),
        (
            "retention",
            fields(&[
                ("history_max_events", integer(17)),
                ("ttl_ms", integer(456)),
            ]),
        ),
        (
            "retry",
            fields(&[
                (
                    "backoff",
                    fields(&[
                        ("base_ms", integer(1)),
                        ("jitter_pct", integer(3)),
                        ("kind", atom("exponential")),
                        ("max_ms", integer(2)),
                    ]),
                ),
                ("exhausted_to", binary(b"dead")),
                ("max_retries", integer(9)),
            ]),
        ),
    ]);
    envelope(
        generation,
        &fields(&[
            ("governance", wire_governance()),
            ("indexed_attributes", names(&[b"a", b"z"])),
            ("indexed_state_meta", binary(b"m")),
            ("max_active_ms", integer(1_000)),
            (
                "retention",
                fields(&[
                    ("history_max_events", integer(64)),
                    ("ttl_ms", integer(123)),
                ]),
            ),
            (
                "retry",
                fields(&[
                    (
                        "backoff",
                        fields(&[
                            ("base_ms", integer(17)),
                            ("jitter_pct", integer(4)),
                            ("kind", atom("linear")),
                            ("max_ms", integer(33)),
                        ]),
                    ),
                    ("exhausted_to", binary(b"dead")),
                    ("max_retries", integer(7)),
                ]),
            ),
            ("states", map(&[(binary(b"a"), a), (binary(b"z"), z)])),
            ("type", binary(b"r")),
            ("version", integer(u64::MAX)),
        ]),
    )
}
fn wire_governance() -> Vec<u8> {
    fields(&[
        (
            "approvals",
            fields(&[
                ("assignees", names(&[b"y", b"x"])),
                ("effects", names(&[b"z", b"a"])),
                ("states", names(&[b"a", b"z"])),
                ("timeout_ms", integer(9)),
            ]),
        ),
        (
            "audit",
            fields(&[("denials", atom("all")), ("effect_events", atom("false"))]),
        ),
        (
            "budgets",
            map(&[(
                binary(b"b"),
                fields(&[
                    ("enforcement", atom("approximate_global")),
                    ("lease_size", integer(5)),
                    ("limit", integer(3)),
                    ("unit", binary(b"u")),
                    ("window_ms", integer(4)),
                ]),
            )]),
        ),
        ("circuits", wire_circuits()),
        (
            "effects",
            fields(&[
                ("allowed", names(&[b"z", b"a"])),
                ("denied", names(&[])),
                ("require_idempotency_key", atom("false")),
            ]),
        ),
        (
            "limits",
            map(&[(
                binary(b"l"),
                fields(&[
                    ("enforcement", atom("strict_global")),
                    ("lease_size", integer(2)),
                    ("limit", integer(0)),
                ]),
            )]),
        ),
        ("mode", atom("full")),
    ])
}
fn wire_circuits() -> Vec<u8> {
    map(&[(
        binary(b"c"),
        fields(&[
            ("error_classes", names(&[b"z", b"a"])),
            ("failure_rate_pct", integer(6)),
            ("failure_threshold", integer(2)),
            ("half_open_max_probes", integer(8)),
            ("half_open_success_threshold", integer(1)),
            ("latency_threshold_ms", integer(7)),
            ("min_calls", integer(5)),
            ("open_ms", integer(3)),
            ("window_ms", integer(4)),
        ]),
    )])
}

pub(super) fn original_wire(generation: u64, version_magnitude: &[u8], opaque: &[u8]) -> Vec<u8> {
    let len = u8::try_from(version_magnitude.len()).unwrap_or_default();
    assert_eq!(usize::from(len), version_magnitude.len(), "small literal");
    let mut big = vec![110, len, 0];
    big.extend_from_slice(version_magnitude);
    let wrapped_empty = vec![108, 0, 0, 0, 0, 106];
    let config = map(&[
        (atom("mode"), atom("parallel")),
        (binary(b"opaque"), wrapped_empty.clone()),
        (atom("governance"), map(&[])),
    ]);
    envelope(
        generation,
        &map(&[
            (binary(b"unknown"), binary(opaque)),
            (integer(7), wrapped_empty),
            (atom("version"), big),
            (atom("states"), map(&[(binary(b"queued"), config)])),
            (atom("type"), binary(b"r")),
        ]),
    )
}
pub(super) fn fifo_wire(generation: u64, version_magnitude: &[u8], opaque: &[u8]) -> Vec<u8> {
    let mut out = original_wire(generation, version_magnitude, opaque);
    let needle = atom("parallel");
    let start = out.windows(needle.len()).position(|w| w == needle);
    assert!(start.is_some(), "one mode");
    let start = start.unwrap_or_default();
    out.splice(start..start + needle.len(), atom("fifo"));
    out
}

pub(super) fn partial_wire(generation: u64, after: bool) -> Vec<u8> {
    let retry = fields(&[
        (
            "backoff",
            fields(&[
                ("base_ms", integer(if after { 9 } else { 17 })),
                ("jitter_pct", integer(4)),
                ("kind", atom("fixed")),
                ("max_ms", integer(33)),
            ]),
        ),
        ("exhausted_to", binary(b"dead")),
        ("max_retries", integer(7)),
    ]);
    let mut config = vec![(atom("mode"), atom("fifo"))];
    if !after {
        config.push((
            atom("retention"),
            fields(&[("history_max_events", integer(17)), ("ttl_ms", integer(50))]),
        ));
    }
    config.push((
        atom("retry"),
        fields(&[("max_retries", integer(if after { 2 } else { 3 }))]),
    ));
    config.push((binary(b"opaque"), vec![108, 0, 0, 0, 0, 106]));
    let mut rows = vec![
        (atom("retry"), retry),
        (
            atom("retention"),
            fields(&[
                ("history_max_events", integer(64)),
                ("ttl_ms", integer(if after { 200 } else { 100 })),
            ]),
        ),
        (atom("states"), map(&[(binary(b"queued"), map(&config))])),
        (atom("type"), binary(b"r")),
    ];
    if after {
        rows.push((atom("max_active_ms"), integer(3)));
    } else {
        rows.extend([
            (atom("version"), integer(7)),
            (binary(b"version"), integer(8)),
        ]);
    }
    envelope(generation, &map(&rows))
}

pub(super) fn group(keywords: bool, rows: &[(&str, Vec<u8>)]) -> Vec<u8> {
    if keywords {
        super::keyword(rows)
    } else {
        fields(rows)
    }
}

pub(super) fn keyword_wire(generation: u64, phase: u8, keywords: bool) -> Vec<u8> {
    let changed = phase == 2;
    let mut retry = vec![("max_retries", integer(if changed { 2 } else { 3 }))];
    if keywords {
        retry.insert(0, ("max_retries", integer(1)));
    }
    let retained = group(
        keywords,
        &[("history_max_events", integer(17)), ("ttl_ms", integer(50))],
    );
    let mut config = vec![("mode", atom("fifo"))];
    if !changed {
        config.push(("retention", retained.clone()));
    }
    config.push(("retry", group(keywords, &retry)));
    if !changed && keywords {
        config.push(("retention", retained));
    }
    config.push(("indexed_state_meta", vec![108, 0, 0, 0, 0, 106]));
    let mut rows = vec![
        (atom("retry"), super::keyword_retry(changed, keywords)),
        (binary(b"retry"), fields(&[("max_retries", integer(999))])),
        (
            atom("retention"),
            group(
                keywords,
                &[
                    ("history_max_events", integer(64)),
                    ("ttl_ms", integer(if changed { 200 } else { 100 })),
                ],
            ),
        ),
        (
            atom("states"),
            map(&[(binary(b"queued"), group(keywords, &config))]),
        ),
        (atom("type"), binary(b"r")),
    ];
    if !changed {
        rows.extend([
            (atom("version"), integer(7)),
            (binary(b"version"), integer(8)),
        ]);
    }
    if phase > 0 {
        rows.push((atom("max_active_ms"), integer(3)));
    }
    envelope(generation, &map(&rows))
}
