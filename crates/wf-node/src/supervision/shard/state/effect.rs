//! `FLOW.EFFECT.*` and `FLOW.GOVERNANCE.LEDGER` on the shard that owns the
//! Flow (ADR-014 step 14).
//!
//! A Flow's effects and ledger are among its stored fields. A command that
//! changes them (a refusal too, which records a ledger event) stores the
//! Flow again unchanged otherwise and logs its after-image without history
//! events, so replay and snapshots restore them and retention removes them
//! with the Flow.

use wf_application::commands::flow::effect::decide::{self, Flow};
use wf_application::commands::flow::effect::denial::default_policy_hash;
use wf_application::commands::flow::effect::{Command, Governance};
use wf_application::commands::queue::Partition;

use super::flow::Executed;
use super::{Key, ShardState};

impl ShardState {
    /// Decides one effect or ledger command on the Flow in `partition`.
    pub(crate) fn effect(
        &mut self,
        wall: u64,
        command: &Command,
        partition: Partition,
    ) -> Executed {
        let key: Key = (partition, command.target().0.to_vec());
        let name = key.0.name();
        let Some(entry) = self.records.get(&key) else {
            if command.reads() {
                return (Ok(decide::read(command, None, None)), None);
            }
            let (reply, _) = decide::write(command, None, &mut Governance::default(), wall);
            return (reply, None);
        };
        let record = &entry.record;
        let flow = Flow {
            id: &key.1,
            partition_key: &name,
            flow_type: record.workflow_type(),
            state: record.state(),
            lease_token: record.lease().map(|lease| lease.token.as_bytes()),
            fence: record.fence().get(),
            policy_hash: self
                .policy(record.workflow_type())
                .map_or(default_policy_hash(), |installed| &installed.policy_hash),
        };
        let stored = entry
            .meta
            .as_deref()
            .and_then(|meta| meta.governance.as_deref());
        if command.reads() {
            return (Ok(decide::read(command, Some(&flow), stored)), None);
        }
        let mut governance = stored.cloned().unwrap_or_default();
        let (reply, changed) = decide::write(command, Some(&flow), &mut governance, wall);
        if !changed {
            return (reply, None);
        }
        let mut meta = entry
            .meta
            .as_deref()
            .cloned()
            .unwrap_or_else(|| self.defaults());
        meta.governance = (!governance.is_empty()).then(|| Box::new(governance));
        let flow = (entry.record.clone(), entry.priority, Some(Box::new(meta)));
        self.put(key.clone(), flow, None);
        match self.after_image_events(&key, 0) {
            Ok(logged) => (reply, Some(logged)),
            Err(error) => (Err(error), None),
        }
    }
}
