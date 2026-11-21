//! Limit credits of governed claims on a shard (ADR-014 step 15): a claim
//! draws one credit per Flow it takes from the pool its composition spent,
//! stamping it on the Flow; a credited Flow that leaves `running` queues
//! its credit for release, delivered once the round is durable. Replay
//! queues the releases of the log again; a release of a credit already
//! released changes nothing.

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use wf_application::commands::flow::effect::Credit;
use wf_application::commands::flow::record::FlowMeta;
use wf_domain::workflow::WorkflowRecord;

use super::{Key, ShardState};

const RUNNING: &[u8] = b"running";

/// The credits a governed claim spent, for its shards to stamp.
pub(crate) struct CreditPool {
    scope: Vec<u8>,
    shard_id: u64,
    ids: Mutex<VecDeque<Vec<u8>>>,
}

impl CreditPool {
    pub(crate) fn new(scope: Vec<u8>, shard_id: u64, ids: Vec<Vec<u8>>) -> Self {
        Self {
            scope,
            shard_id,
            ids: Mutex::new(ids.into()),
        }
    }

    /// The next credit, if any is left.
    pub(crate) fn take(&self) -> Option<Credit> {
        let id = self
            .ids
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()?;
        Some(Credit {
            scope: self.scope.clone(),
            shard_id: self.shard_id,
            reservation_id: id,
        })
    }

    /// The credits no Flow took.
    pub(crate) fn left(&self) -> Vec<Vec<u8>> {
        self.ids
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
            .collect()
    }
}

impl ShardState {
    /// Queues the credit of a Flow leaving `running`.
    pub(super) fn note_credit(
        &mut self,
        was_running: bool,
        record: &WorkflowRecord,
        meta: Option<&FlowMeta>,
    ) {
        if !was_running || record.state() == RUNNING {
            return;
        }
        if let Some(credit) = meta
            .and_then(|meta| meta.governance.as_deref())
            .and_then(|governance| governance.credit.clone())
        {
            self.credit_releases.push(credit);
        }
    }

    /// Whether a release waits for delivery.
    pub(crate) fn has_credit_releases(&self) -> bool {
        !self.credit_releases.is_empty()
    }

    /// The releases to deliver once this round is durable.
    pub(crate) fn take_credit_releases(&mut self) -> Vec<Credit> {
        std::mem::take(&mut self.credit_releases)
    }

    /// Stamps a claimed Flow with its credit; the claim is then logged as
    /// the Flow's after-image.
    pub(super) fn stamp_credit(&mut self, key: &Key, credit: Credit) -> bool {
        let defaults = self.defaults();
        let Some(entry) = self.records.get(key) else {
            return false;
        };
        let mut meta = entry.meta.as_deref().cloned().unwrap_or(defaults);
        let mut governance = meta.governance.take().map(|held| *held).unwrap_or_default();
        governance.credit = Some(credit);
        meta.governance = Some(Box::new(governance));
        let flow = (entry.record.clone(), entry.priority, Some(Box::new(meta)));
        self.put(key.clone(), flow, None);
        true
    }
}
