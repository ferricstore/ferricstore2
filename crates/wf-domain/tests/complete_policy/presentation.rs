//! Pure Policy.response projection; source hot-key omission is type-shape evidence.

use wf_domain::policy::complete::{
    CompletePolicyEncodingError, CompletePolicyPresentation, EffectsData, GovernanceData,
    GovernanceMode, PolicySchemaError, PresentedPolicyState, PresentedRetry,
    prepare_complete_policy_value_v2, present_complete_policy,
};
use wf_domain::policy::definition::StateMode;
use wf_domain::policy::{Backoff, BackoffKind, RetryOverride};
use wf_domain::retention::{
    MAX_HISTORY_EVENTS, MAX_RETENTION_MILLIS, ResolvedRetention, RetentionOverride,
};

use CompletePolicyEncodingError::{EncodingMismatch, Schema};
use CompletePolicyPresentation::{All, State};
use PolicySchemaError::{InvalidAuthority, ProfileMismatch, WorkBytes};

use std::collections::BTreeMap;
use wf_domain::policy::complete::{
    CompletePolicyData, CompletePolicyValue, CompleteStatePolicyData, PolicySchemaCharge,
    PolicySchemaLimits,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::policy::installation::PolicyLimits;

use super::{TestResult, bind, candidate, data, limits};

mod refusals;

#[test]
fn presentation_preserves_pinned_type_and_state_response_defaults() -> TestResult {
    let p = limits(10_000_000)?;
    let mut charge = PolicySchemaCharge::new(p);
    let value = presentation_value(1, presentation_source(), p, &mut charge)?;
    let ty = b"flow-policy".to_vec();
    let query = b"charge_card".to_vec();
    let defaults = ResolvedRetention::default();
    let fixed = Backoff {
        kind: BackoffKind::Fixed,
        base_ms: 10_000,
        max_ms: 30_000,
        jitter_pct: 0,
    };
    let expected_row = PresentedPolicyState {
        state: b"charge_card",
        mode: StateMode::Parallel,
        max_active_ms: Some(120_000),
        retry: PresentedRetry {
            max_retries: 2,
            backoff: fixed,
            exhausted_to: b"payment_failed",
        },
        retention: ResolvedRetention {
            ttl_ms: 30_000,
            history_max_events: 256,
        },
        governance: None,
    };
    // Independent source34–44 whole oracle; public retention has no hot field.
    let expected_all = All {
        workflow_type: b"flow-policy",
        generation: 1,
        version: None,
        max_active_ms: Some(120_000),
        retry: PresentedRetry {
            max_retries: 5,
            backoff: fixed,
            exhausted_to: b"failed",
        },
        retention: ResolvedRetention {
            ttl_ms: 60_000,
            history_max_events: 512,
        },
        indexed_attributes: &[],
        indexed_state_meta: None,
        governance: None,
        states: vec![expected_row],
    };
    // Source46–53 inherits fixed10000/30000/0 and preserves30000/256.
    let expected_selected = State {
        workflow_type: b"flow-policy",
        generation: 1,
        version: None,
        max_active_ms: Some(120_000),
        state: b"charge_card",
        mode: StateMode::Parallel,
        retry: expected_row.retry,
        retention: expected_row.retention,
        indexed_attributes: &[],
        indexed_state_meta: None,
        governance: None,
    };
    let all = present_complete_policy(&ty, Some(&value), None, defaults, p, &mut charge)?;
    assert!(all == expected_all);
    let selected =
        present_complete_policy(&ty, Some(&value), Some(&query), defaults, p, &mut charge)?;
    assert!(selected == expected_selected);
    assert_eq!(charge.p(), 0);
    let All { states, .. } = all else {
        return Err("expected all-state response".into());
    };
    let [row] = states.as_slice() else {
        return Err("expected charge_card only".into());
    };
    let State {
        workflow_type,
        state,
        ..
    } = selected
    else {
        return Err("expected queried response".into());
    };
    let raw = &value.data().states;
    let (name, stored) = raw.first_key_value().ok_or("missing state")?;
    let stored_retry = stored.retry.as_ref().ok_or("missing retry")?;
    let target = stored_retry
        .exhausted_to
        .as_deref()
        .ok_or("missing target")?;
    assert_eq!(row.state.as_ptr(), name.as_ptr());
    assert_eq!(row.retry.exhausted_to.as_ptr(), target.as_ptr());
    assert_eq!(state.as_ptr(), query.as_ptr());
    assert_eq!(workflow_type.as_ptr(), ty.as_ptr());
    Ok(())
}

#[test]
fn presentation_distinguishes_absent_all_and_requested_state_shapes() -> TestResult {
    let p = limits(10_000_000)?;
    let mut charge = PolicySchemaCharge::new(p);
    let defaults = ResolvedRetention::default();
    let retry = PresentedRetry {
        max_retries: 3,
        backoff: Backoff::default(),
        exhausted_to: b"failed",
    };
    for query in [None, Some(&b""[..]), Some(&b"unknown"[..])] {
        let expected = match query {
            None => All {
                workflow_type: b"r",
                generation: 0,
                version: None,
                max_active_ms: None,
                retry,
                retention: defaults,
                indexed_attributes: &[],
                indexed_state_meta: None,
                governance: None,
                states: Vec::new(),
            },
            Some(state) => State {
                workflow_type: b"r",
                generation: 0,
                version: None,
                max_active_ms: None,
                retry,
                retention: defaults,
                indexed_attributes: &[],
                indexed_state_meta: None,
                governance: None,
                state,
                mode: StateMode::Parallel,
            },
        };
        let output = present_complete_policy(b"r", None, query, defaults, p, &mut charge)?;
        assert!(output == expected);
    }
    assert_eq!((charge.g(), charge.p()), (1_162, 0));
    let invalid = Schema(InvalidAuthority);
    for (ty, ttl_ms, history_max_events) in [
        (Vec::new(), 53, 17),
        (vec![b'r'; 262_145], 53, 17),
        (b"r".to_vec(), 0, 17),
        (b"r".to_vec(), MAX_RETENTION_MILLIS + 1, 17),
        (b"r".to_vec(), 53, 0),
        (b"r".to_vec(), 53, MAX_HISTORY_EVENTS + 1),
    ] {
        let supplied = ResolvedRetention {
            ttl_ms,
            history_max_events,
        };
        let result = present_complete_policy(&ty, None, None, supplied, p, &mut charge);
        assert_eq!(result.err(), Some(invalid));
    }
    assert_eq!((charge.g(), charge.p()), (1_930, 0));
    let query = vec![0; 262_145];
    let result = present_complete_policy(b"r", None, Some(&query), defaults, p, &mut charge);
    assert_eq!(result.err(), Some(invalid));
    assert_eq!((charge.g(), charge.p()), (2_059, 0));
    Ok(())
}

#[test]
fn presentation_preserves_whole_raw_governance_presence_and_fallback() -> TestResult {
    let p = limits(10_000_000)?;
    let mut charge = PolicySchemaCharge::new(p);
    let defaults = ResolvedRetention::default();
    let mut type_governance = presentation_governance();
    type_governance.mode = Some(GovernanceMode::Full);
    type_governance.effects = Some(EffectsData {
        allowed: Some(vec![b"charge".to_vec()]),
        denied: Some(vec![b"refund".to_vec()]),
        require_idempotency_key: Some(true),
    });
    let mut own = presentation_governance();
    own.effects = Some(EffectsData {
        allowed: Some(Vec::new()),
        denied: None,
        require_idempotency_key: Some(false),
    });
    let mut input = data(b"r".to_vec(), None, BTreeMap::new());
    input.governance = Some(type_governance);
    input.states.insert(
        b"empty".to_vec(),
        presentation_state(None, None, None, Some(presentation_governance())),
    );
    input.states.insert(
        b"own".to_vec(),
        presentation_state(None, None, None, Some(own)),
    );
    input.states.insert(
        b"plain".to_vec(),
        presentation_state(None, None, None, None),
    );
    let value = presentation_value(2, input, p, &mut charge)?;
    let raw = value.data();
    let type_group = raw.governance.as_ref();
    let own_group = raw.states[b"own".as_slice()].governance.as_ref();
    let empty_group = raw.states[b"empty".as_slice()].governance.as_ref();
    let all = present_complete_policy(b"r", Some(&value), None, defaults, p, &mut charge)?;
    let All {
        governance, states, ..
    } = all
    else {
        return Err("expected all-state response".into());
    };
    assert!(governance == type_group);
    assert_eq!(states.len(), 3);
    assert_eq!(
        states.iter().map(|row| row.state).collect::<Vec<_>>(),
        [b"empty".as_slice(), b"own".as_slice(), b"plain".as_slice()]
    );
    assert!(states[0].governance == Some(&presentation_governance()));
    assert!(states[1].governance == own_group);
    assert!(states[1].governance.is_some_and(|g| g.mode.is_none()));
    assert!(states[2].governance.is_none());
    for (query, expected) in [
        (b"plain".as_slice(), type_group),
        (b"unknown".as_slice(), type_group),
        (b"".as_slice(), type_group),
        (b"empty".as_slice(), empty_group),
        (b"own".as_slice(), own_group),
    ] {
        let selected =
            present_complete_policy(b"r", Some(&value), Some(query), defaults, p, &mut charge)?;
        let State {
            state, governance, ..
        } = selected
        else {
            return Err("expected queried response".into());
        };
        assert_eq!(state, query);
        assert!(governance == expected);
        if query == b"empty" {
            assert!(governance == Some(&presentation_governance()));
        }
    }
    let p = PolicySchemaLimits::new(PolicyLimits::new(2)?, 10_000_000)?;
    let mut charge = PolicySchemaCharge::new(p);
    let query = vec![0; 262_145];
    let result =
        present_complete_policy(b"r", Some(&value), Some(&query), defaults, p, &mut charge);
    assert_eq!(result.err(), Some(Schema(InvalidAuthority)));
    assert_eq!((charge.g(), charge.p()), (257, 0));
    Ok(())
}

#[test]
fn presentation_borrows_indexed_version_and_each_backoff_kind() -> TestResult {
    let p = limits(10_000_000)?;
    let mut charge = PolicySchemaCharge::new(p);
    let defaults = ResolvedRetention::default();
    for (kind, number) in [
        (BackoffKind::None, 0),
        (BackoffKind::None, u64::MAX),
        (BackoffKind::Fixed, u64::MAX),
        (BackoffKind::Linear, u64::MAX),
        (BackoffKind::Exponential, u64::MAX),
    ] {
        let expected_version = PolicyVersion::Number(number);
        let backoff = Backoff {
            kind,
            base_ms: 0,
            max_ms: 0,
            jitter_pct: 0,
        };
        let mut input = data(
            b"r".to_vec(),
            Some(PolicyVersion::Number(number)),
            BTreeMap::new(),
        );
        input.max_active_ms = Some(1);
        input.retry = Some(RetryOverride {
            max_retries: Some(0),
            backoff: Some(backoff),
            exhausted_to: Some(vec![0, 0xff]),
        });
        input.indexed_attributes = Some(vec![b"second".to_vec(), b"first".to_vec()]);
        input.indexed_state_meta = Some(b"meta".to_vec());
        let mode = Some(StateMode::Fifo);
        let retry = Some(RetryOverride::default());
        input
            .states
            .insert(b"s".to_vec(), presentation_state(mode, retry, None, None));
        let value = presentation_value(7, input, p, &mut charge)?;
        let raw = value.data();
        let names = raw.indexed_attributes.as_deref().ok_or("missing names")?;
        let meta = raw.indexed_state_meta.as_deref().ok_or("missing meta")?;
        let stored_retry = raw.retry.as_ref().ok_or("missing retry")?;
        let target = stored_retry
            .exhausted_to
            .as_deref()
            .ok_or("missing target")?;
        for query in [b"s".as_slice(), b"unknown".as_slice(), b"".as_slice()] {
            let mode = if query == b"s" {
                StateMode::Fifo
            } else {
                StateMode::Parallel
            };
            let expected = State {
                workflow_type: b"r",
                state: query,
                mode,
                generation: 7,
                version: Some(&expected_version),
                max_active_ms: Some(1),
                retry: PresentedRetry {
                    max_retries: 0,
                    backoff,
                    exhausted_to: &[0, 0xff],
                },
                retention: defaults,
                indexed_attributes: names,
                indexed_state_meta: Some(b"meta"),
                governance: None,
            };
            assert_eq!(names, [b"second".to_vec(), b"first".to_vec()]);
            let output =
                present_complete_policy(b"r", Some(&value), Some(query), defaults, p, &mut charge)?;
            assert!(output == expected);
            let State {
                version,
                retry,
                indexed_attributes,
                indexed_state_meta,
                ..
            } = output
            else {
                return Err("expected queried response".into());
            };
            assert_presented_retry(&retry, 0, backoff, &[0, 0xff]);
            assert_eq!(charge.p(), 128);
            assert_eq!(indexed_attributes.as_ptr(), names.as_ptr());
            assert_eq!(
                indexed_state_meta.ok_or("missing projected meta")?.as_ptr(),
                meta.as_ptr()
            );
            assert_eq!(retry.exhausted_to.as_ptr(), target.as_ptr());
            assert!(
                version
                    .zip(raw.version.as_ref())
                    .is_some_and(|(a, b)| std::ptr::eq(a, b))
            );
        }
    }
    Ok(())
}

#[test]
fn presentation_uses_captured_public_defaults_without_history_clamp() -> TestResult {
    let p = limits(10_000_000)?;
    let mut charge = PolicySchemaCharge::new(p);
    let defaults = ResolvedRetention {
        ttl_ms: 53,
        history_max_events: 17,
    };
    for (present, ttl, history, state_ttl, expected) in [
        (false, None, None, None, (53, 17)),
        (true, None, None, None, (53, 17)),
        (true, Some(101), Some(88), None, (101, 88)),
        (true, Some(101), Some(88), Some(23), (23, 88)),
    ] {
        let mut input = data(
            b"r".to_vec(),
            Some(PolicyVersion::Bytes(vec![0, 0xff])),
            BTreeMap::new(),
        );
        input.retention = present.then_some(RetentionOverride {
            ttl_ms: ttl,
            history_max_events: history,
        });
        let retention = present.then_some(RetentionOverride {
            ttl_ms: state_ttl,
            history_max_events: None,
        });
        input.states.insert(
            b"s".to_vec(),
            presentation_state(None, None, retention, None),
        );
        let value = presentation_value(1, input, p, &mut charge)?;
        let output =
            present_complete_policy(b"r", Some(&value), Some(b"s"), defaults, p, &mut charge)?;
        let State {
            retention, version, ..
        } = output
        else {
            return Err("expected queried-state response".into());
        };
        assert_eq!((retention.ttl_ms, retention.history_max_events), expected);
        assert_eq!(version, Some(&PolicyVersion::Bytes(vec![0, 0xff])));
        let all = present_complete_policy(b"r", Some(&value), None, defaults, p, &mut charge)?;
        assert!(
            matches!(all, All { version: Some(PolicyVersion::Bytes(bytes)), .. } if bytes == &[0, 0xff])
        );
    }
    let p = limits(10_000)?;
    let mut charge = PolicySchemaCharge::new(p);
    let input = data(b"r".to_vec(), None, BTreeMap::new());
    let value = presentation_value(1, input, p, &mut charge)?;
    let query = vec![0; 262_145];
    let result =
        present_complete_policy(b"x", Some(&value), Some(&query), defaults, p, &mut charge);
    assert_eq!(result.err(), Some(Schema(InvalidAuthority)));
    assert_eq!((charge.g(), charge.p()), (4_600, 0));
    for (length, expected) in [(40, 2_743), (39, 2_607)] {
        let mut charge = PolicySchemaCharge::new(p);
        let input = data(b"r".to_vec(), None, BTreeMap::new());
        let input = candidate(1, input, p, &mut charge)?;
        let bad = bind(input, vec![0; length], 0..length, p, &mut charge)?;
        let result =
            present_complete_policy(b"x", Some(&bad), Some(&query), defaults, p, &mut charge);
        assert!(matches!(result, Err(EncodingMismatch)));
        assert_eq!((charge.g(), charge.p()), (expected, 0));
    }
    Ok(())
}

fn presentation_value(
    generation: u64,
    value: CompletePolicyData,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompletePolicyValue> {
    Ok(prepare_complete_policy_value_v2(
        candidate(generation, value, limits, charge)?,
        limits,
        charge,
    )?)
}

fn presentation_state(
    mode: Option<StateMode>,
    retry: Option<RetryOverride>,
    retention: Option<RetentionOverride>,
    governance: Option<GovernanceData>,
) -> CompleteStatePolicyData {
    CompleteStatePolicyData {
        mode,
        retry,
        retention,
        governance,
    }
}

fn presentation_governance() -> GovernanceData {
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

fn presentation_source() -> CompletePolicyData {
    let mut value = data(b"flow-policy".to_vec(), None, BTreeMap::new());
    value.max_active_ms = Some(120_000);
    value.retry = Some(RetryOverride {
        max_retries: Some(5),
        backoff: Some(Backoff {
            kind: BackoffKind::Fixed,
            base_ms: 10_000,
            max_ms: 30_000,
            jitter_pct: 0,
        }),
        exhausted_to: Some(b"failed".to_vec()),
    });
    value.retention = Some(RetentionOverride {
        ttl_ms: Some(60_000),
        history_max_events: Some(512),
    });
    value.states.insert(
        b"charge_card".to_vec(),
        presentation_state(
            None,
            Some(RetryOverride {
                max_retries: Some(2),
                backoff: None,
                exhausted_to: Some(b"payment_failed".to_vec()),
            }),
            Some(RetentionOverride {
                ttl_ms: Some(30_000),
                history_max_events: Some(256),
            }),
            None,
        ),
    );
    value
}

fn assert_presented_retry(
    retry: &PresentedRetry<'_>,
    maximum: u16,
    backoff: Backoff,
    target: &[u8],
) {
    assert_eq!(retry.max_retries, maximum);
    assert_eq!(retry.backoff, backoff);
    assert_eq!(retry.exhausted_to, target);
}
