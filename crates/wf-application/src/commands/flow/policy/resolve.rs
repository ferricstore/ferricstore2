//! What a policy resolves to: retry and retention per state, a new Flow's
//! lifecycle limits, and the `FLOW.POLICY.SET` / `FLOW.POLICY.GET` reply as
//! `Ferricstore.Flow.Policy.response/4` builds it.

use wf_domain::policy::{RetryOverride, RetryPolicy, resolve as resolve_retry};

use super::super::term::{Term, map};
use super::{FlowPolicy, Installed, Mode, Retention, backoff_term};

/// Retention limits: the node's defaults, or what a Flow stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lifecycle {
    /// How long a terminal Flow is kept.
    pub ttl_ms: u64,
    /// History events kept hot.
    pub history_hot_max_events: u64,
    /// History events kept.
    pub history_max_events: u64,
}

/// `resolve_retention/4` for a Flow created in `state`: the create options
/// over the state's retention over the node defaults; hot events never
/// exceed the total.
#[must_use]
pub fn lifecycle(
    policy: Option<&FlowPolicy>,
    state: Option<&[u8]>,
    defaults: Lifecycle,
    overrides: [Option<u64>; 3],
) -> Lifecycle {
    let [ttl_ms, hot, total] = overrides;
    let typed = policy.and_then(|policy| policy.retention.as_ref());
    let stated = policy
        .zip(state)
        .and_then(|(policy, state)| policy.states.get(state))
        .and_then(|policy| policy.retention.as_ref());
    // The state's field over the type's.
    let pick = |field: fn(&Retention) -> Option<u64>| {
        stated.and_then(field).or_else(|| typed.and_then(field))
    };
    let history_max_events = total
        .or_else(|| pick(|retention| retention.history_max_events))
        .unwrap_or(defaults.history_max_events);
    Lifecycle {
        ttl_ms: ttl_ms
            .or_else(|| pick(|retention| retention.ttl_ms))
            .unwrap_or(defaults.ttl_ms),
        history_hot_max_events: hot
            .unwrap_or(defaults.history_hot_max_events)
            .min(history_max_events),
        history_max_events,
    }
}

fn retry_term(installed: Option<&Installed>, flow_type: &[u8], state: Option<&[u8]>) -> Term {
    let retry = resolve_retry(
        installed.map(|installed| &installed.retry),
        flow_type,
        state.unwrap_or_default(),
        &RetryOverride::default(),
    )
    .unwrap_or_else(|_| RetryPolicy::default());
    map(vec![
        ("max_retries", Term::int(u64::from(retry.max_retries))),
        ("backoff", backoff_term(&retry.backoff)),
        ("exhausted_to", Term::Bin(retry.exhausted_to)),
    ])
}

fn retention_term(policy: Option<&FlowPolicy>, state: Option<&[u8]>, defaults: Lifecycle) -> Term {
    let resolved = lifecycle(policy, state, defaults, [None; 3]);
    map(vec![
        ("ttl_ms", Term::int(resolved.ttl_ms)),
        ("history_max_events", Term::int(resolved.history_max_events)),
    ])
}

/// The reply for `flow_type`'s policy, for one state or with every state.
#[must_use]
pub fn reply(
    flow_type: &[u8],
    installed: Option<&Installed>,
    state: Option<&[u8]>,
    defaults: Lifecycle,
) -> Term {
    let policy = installed.map(|installed| &installed.policy);
    let max_active_ms = Term::optional_int(policy.and_then(|policy| policy.max_active_ms));
    let indexed_attributes = Term::List(
        policy
            .and_then(|policy| policy.indexed_attributes.as_ref())
            .map(|names| names.iter().map(|name| Term::text(name)).collect())
            .unwrap_or_default(),
    );
    let indexed_state_meta = policy
        .and_then(|policy| policy.indexed_state_meta.as_deref())
        .map_or(Term::Nil, Term::text);
    let generation = Term::int(installed.map_or(0, |installed| installed.generation));
    let mode = |state: &[u8]| {
        Term::text(
            policy
                .map_or(Mode::Parallel, |policy| policy.mode(state))
                .as_str(),
        )
    };
    let Some(state) = state else {
        let states = policy
            .map(|policy| {
                policy
                    .states
                    .keys()
                    .map(|name| {
                        let fields = map(vec![
                            ("mode", mode(name)),
                            ("max_active_ms", max_active_ms.clone()),
                            ("retry", retry_term(installed, flow_type, Some(name))),
                            (
                                "retention",
                                retention_term(policy.into(), Some(name), defaults),
                            ),
                            ("governance", Term::Nil),
                        ]);
                        (name.clone(), fields)
                    })
                    .collect()
            })
            .unwrap_or_default();
        return map(vec![
            ("type", Term::Bin(flow_type.to_vec())),
            ("generation", generation),
            ("version", Term::Nil),
            ("max_active_ms", max_active_ms),
            ("retry", retry_term(installed, flow_type, None)),
            ("retention", retention_term(policy, None, defaults)),
            ("indexed_attributes", indexed_attributes),
            ("indexed_state_meta", indexed_state_meta),
            ("governance", Term::Nil),
            ("states", Term::Map(states)),
        ]);
    };
    map(vec![
        ("type", Term::Bin(flow_type.to_vec())),
        ("state", Term::Bin(state.to_vec())),
        ("generation", generation),
        ("version", Term::Nil),
        ("mode", mode(state)),
        ("max_active_ms", max_active_ms),
        ("retry", retry_term(installed, flow_type, Some(state))),
        ("retention", retention_term(policy, Some(state), defaults)),
        ("indexed_attributes", indexed_attributes),
        ("indexed_state_meta", indexed_state_meta),
        ("governance", Term::Nil),
    ])
}
