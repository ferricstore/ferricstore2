//! Running-Flow limits from Flow policies (ADR-014 step 15), as
//! `policy_governance_limit/7` reads them: a claim of a type whose policy
//! gives a claimed state `governance.limits.running` spends from the
//! type's `flow-running:` limit, leasing it on demand with the strictest
//! of the rules. The rules of every type are kept with the router, from
//! the policies at open and each policy set.

use std::collections::HashMap;
use std::sync::{PoisonError, RwLock};

use wf_application::commands::flow::governance::policy::{
    RunningLimit, running_limit, running_scope,
};
use wf_application::commands::flow::policy::Installed;
use wf_application::commands::queue::{ClaimFlows, Partition};

use super::{AutoLease, Governed};

/// One type's running-limit rules by state, with its policy generation.
pub(crate) struct RunningPolicy {
    generation: u64,
    rules: Vec<(Vec<u8>, RunningLimit)>,
}

/// Every type's running-limit rules.
pub(crate) type Running = RwLock<HashMap<Vec<u8>, RunningPolicy>>;

/// A policy's running-limit rules, if any state has one.
fn rules(installed: &Installed) -> Option<RunningPolicy> {
    let rules: Vec<(Vec<u8>, RunningLimit)> = installed
        .policy
        .states
        .iter()
        .filter_map(|(state, policy)| {
            let rule = running_limit(&policy.governance_term()?)?;
            Some((state.clone(), rule))
        })
        .collect();
    (!rules.is_empty()).then_some(RunningPolicy {
        generation: installed.generation,
        rules,
    })
}

/// The rules of the policies shard 0 holds at open.
pub(crate) fn opened(owner: Option<&crate::supervision::shard::state::ShardState>) -> Running {
    let running = Running::default();
    if let Some(owner) = owner {
        owner
            .policies()
            .for_each(|installed| install(&running, installed));
    }
    running
}

/// Keeps `installed`'s rules, replacing the type's earlier ones.
pub(crate) fn install(running: &Running, installed: &Installed) {
    let mut held = running.write().unwrap_or_else(PoisonError::into_inner);
    match rules(installed) {
        Some(policy) => {
            held.insert(installed.flow_type.clone(), policy);
        }
        None => {
            held.remove(&installed.flow_type);
        }
    }
}

/// How the type's policy governs `request`: the claimed states' rules
/// joined (the lowest limit and lease size, strict if any rule is), or
/// `None` when no claimed state has one.
pub(crate) fn governed(running: &Running, request: &ClaimFlows, shards: usize) -> Option<Governed> {
    let held = running.read().unwrap_or_else(PoisonError::into_inner);
    let policy = held.get(&request.flow_type)?;
    let amount = u64::try_from(request.limit).unwrap_or(u64::MAX);
    let rules: Vec<&RunningLimit> = policy
        .rules
        .iter()
        .filter(|(state, _)| request.states.is_empty() || request.states.contains(state))
        .map(|(_, rule)| rule)
        .collect();
    let limit = rules.iter().map(|rule| rule.limit).min()?;
    let lease_size = rules
        .iter()
        .map(|rule| rule.lease_size.unwrap_or(amount))
        .min()
        .unwrap_or(amount);
    let scope = running_scope(&request.flow_type);
    // Elixir hashes the worker with `phash2`; this node with its routing
    // slot, so the lease shard id may differ.
    let slot = usize::from(Partition::parse(&request.worker).slot());
    Some(Governed {
        scope,
        shard_id: u64::try_from(slot.checked_rem(shards.max(1)).unwrap_or(0)).unwrap_or(0),
        strict: rules.iter().any(|rule| rule.strict),
        auto: Some(AutoLease {
            lease_size,
            limit,
            config_version: (policy.generation > 0).then_some(policy.generation),
        }),
    })
}
