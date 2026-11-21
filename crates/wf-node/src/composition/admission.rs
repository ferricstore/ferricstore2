//! Admission (ADR-017): each shard counts the items waiting in its request
//! queue. A client request is refused as busy, before any part of it is
//! sent, when a shard it needs holds as many as the request's class allows.
//! Requests that start Flows are refused first, so the work that finishes
//! Flows keeps draining the backlog. The node's own requests (snapshots,
//! policy copies, schedules, sweeps) are never refused.

use std::sync::atomic::Ordering;

use wf_application::commands::flow::FlowCommand;
use wf_application::commands::queue::ServiceError;

use super::router::Router;

/// Per-shard backlog limits, in items: a batch counts each of its items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Admission {
    /// Backlog at which requests that start Flows are refused; 0 refuses
    /// every one.
    pub new_work: usize,
    /// Backlog at which other client requests are refused.
    pub other: usize,
}

impl Default for Admission {
    fn default() -> Self {
        Self {
            new_work: 16_384,
            other: 65_536,
        }
    }
}

/// Which limit a request is held to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    /// Starts Flows.
    New,
    /// Any other client request.
    Other,
}

impl Class {
    pub(crate) const fn of(command: &FlowCommand) -> Self {
        match command {
            FlowCommand::Create(_)
            | FlowCommand::CreateMany(_)
            | FlowCommand::StartAndClaim(_)
            | FlowCommand::RunSteps(_)
            | FlowCommand::SpawnChildren(_) => Self::New,
            _ => Self::Other,
        }
    }
}

impl Router {
    /// Refuses the request when one of `shards` is at its class's limit.
    ///
    /// # Errors
    /// [`ServiceError::Busy`].
    pub(crate) fn admit(
        &self,
        shards: impl IntoIterator<Item = usize>,
        class: Class,
    ) -> Result<(), ServiceError> {
        let limit = match class {
            Class::New => self.admission.new_work,
            Class::Other => self.admission.other,
        };
        let full = shards.into_iter().any(|index| {
            self.shards
                .get(index)
                .is_some_and(|shard| shard.backlog.load(Ordering::Relaxed) >= limit)
        });
        if full {
            Err(ServiceError::Busy)
        } else {
            Ok(())
        }
    }
}

/// The shards a per-shard grouping sends to.
pub(crate) fn occupied<T>(per_shard: &[Vec<T>]) -> impl Iterator<Item = usize> + '_ {
    per_shard
        .iter()
        .enumerate()
        .filter(|(_, items)| !items.is_empty())
        .map(|(index, _)| index)
}
