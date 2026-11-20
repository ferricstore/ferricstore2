//! Flow type policies on the shard (ADR-014 step 9).
//!
//! Shard 0 decides every `FLOW.POLICY.SET` as Elixir's shard 0 does: the
//! expected generation, the merge into the stored form, normalization and
//! the next generation, logged as one `Policy` record. The other shards hold
//! copies the node installs once shard 0's change is durable; replay and
//! snapshots restore shard 0's table and the node copies it out at open. A
//! shard keeps the newest generation it has seen of each type.

use std::sync::Arc;

use wf_application::commands::flow::policy::{self, Installed, Lifecycle, PolicyGet, PolicySet};
use wf_application::commands::flow::term::Term;
use wf_application::commands::queue::ServiceError;

use super::super::command::Logged;
use super::ShardState;

/// A policy command's reply, and the entry a set installed for the other
/// shards to copy.
pub(crate) type PolicyReply = (Term, Option<Arc<Installed>>);

/// What a policy request asks of a shard.
pub(crate) enum PolicyCommand {
    /// Decide and log a set; shard 0 only.
    Set(Box<PolicySet>),
    /// Read a type's policy.
    Get(PolicyGet),
    /// Hold a copy of shard 0's durable change.
    Install(Arc<Installed>),
}

impl ShardState {
    /// The policy of `flow_type`, if one was set.
    pub(crate) fn policy(&self, flow_type: &[u8]) -> Option<&Installed> {
        self.policies.get(flow_type).map(Arc::as_ref)
    }

    /// The node's retention defaults that a policy layers over.
    pub(super) fn lifecycle_defaults(&self) -> Lifecycle {
        let defaults = self.defaults();
        Lifecycle {
            ttl_ms: defaults.retention_ttl_ms,
            history_hot_max_events: defaults.history_hot_max_events,
            history_max_events: defaults.history_max_events,
        }
    }

    /// Holds `installed` unless this shard has a newer generation of its
    /// type.
    pub(crate) fn install_policy(&mut self, installed: Arc<Installed>) {
        let newer = self
            .policies
            .get(&installed.flow_type)
            .is_none_or(|held| held.generation <= installed.generation);
        if newer {
            self.policies.insert(installed.flow_type.clone(), installed);
        }
    }

    /// The policies this shard holds, for its image and for the others.
    pub(crate) fn policies(&self) -> impl Iterator<Item = &Arc<Installed>> {
        self.policies.values()
    }

    /// Runs one policy command.
    pub(crate) fn policy_command(
        &mut self,
        command: PolicyCommand,
    ) -> (Result<PolicyReply, ServiceError>, Option<Logged>) {
        match command {
            PolicyCommand::Get(get) => (Ok((self.policy_term(&get), None)), None),
            PolicyCommand::Install(installed) => {
                self.install_policy(installed);
                (Ok((Term::text("OK"), None)), None)
            }
            PolicyCommand::Set(set) => match self.decide_policy(&set) {
                Ok((installed, logged)) => {
                    self.install_policy(Arc::clone(&installed));
                    let term = policy::reply(
                        &set.flow_type,
                        Some(&installed),
                        set.state.as_deref(),
                        self.lifecycle_defaults(),
                    );
                    (Ok((term, Some(installed))), Some(logged))
                }
                Err(error) => (Err(error), None),
            },
        }
    }

    fn policy_term(&self, get: &PolicyGet) -> Term {
        policy::reply(
            &get.flow_type,
            self.policy(&get.flow_type),
            get.state.as_deref(),
            self.lifecycle_defaults(),
        )
    }

    fn decide_policy(&self, set: &PolicySet) -> Result<(Arc<Installed>, Logged), ServiceError> {
        let stored = self.policy(&set.flow_type);
        let generation = policy::next_generation(
            stored.map_or(0, |stored| stored.generation),
            set.expected_generation,
        )?;
        let attrs = policy::merge(stored.map(|stored| &stored.policy), &set.patch, set.replace);
        let normalized = policy::normalize(&attrs)?;
        let mut encoded = Vec::new();
        normalized
            .term()
            .encode(&mut encoded)
            .ok_or(ServiceError::Unavailable)?;
        let installed = Installed::new(set.flow_type.clone(), generation, normalized)?;
        let logged = Logged::Policy {
            flow_type: set.flow_type.clone(),
            generation,
            policy: encoded,
        };
        Ok((Arc::new(installed), logged))
    }

    /// Restores a logged or imaged policy.
    pub(crate) fn restore_policy(
        &mut self,
        flow_type: &[u8],
        generation: u64,
        bytes: &[u8],
    ) -> Result<(), ServiceError> {
        let Some((Term::Map(entries), [])) = Term::decode(bytes) else {
            return Err(ServiceError::Unavailable);
        };
        let normalized = policy::normalize(&entries)?;
        let installed = Installed::new(flow_type.to_vec(), generation, normalized)?;
        self.install_policy(Arc::new(installed));
        Ok(())
    }
}
