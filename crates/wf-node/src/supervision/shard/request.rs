//! What every shard request shares: its weight in the backlog (ADR-017),
//! whether the node sends it itself, and its refusal.

use wf_application::commands::queue::ServiceError;

use super::Request;
use super::state::Peeked;

impl Request {
    /// The items it carries, at least one: its weight in the shard's
    /// backlog (ADR-017).
    pub(crate) fn weight(&self) -> usize {
        let items = match self {
            Self::Create { items, .. } => items.len(),
            Self::CreateBatch { items, .. } => items.len(),
            Self::Complete { items, .. } => items.len(),
            Self::FlowBatch { items, .. } => items.len(),
            Self::RunSteps { items, .. } => items.len(),
            Self::ValueRead { refs, .. } => refs.len(),
            Self::Replay { changes, .. } => changes.len(),
            _ => 1,
        };
        items.max(1)
    }

    /// Whether the node sent it itself: a follower still handles it
    /// (ADR-018).
    pub(crate) const fn internal(&self) -> bool {
        matches!(
            self,
            Self::Replay { .. }
                | Self::Live { .. }
                | Self::Quiet { .. }
                | Self::Policies { .. }
                | Self::Capture { .. }
        )
    }

    /// Answers it with `error` without running it.
    pub(crate) fn fail(self, error: ServiceError) {
        match self {
            Self::Create { done, .. } | Self::Complete { done, .. } => done(Err(error)),
            Self::Claim { done, .. } => done(Err(error)),
            Self::Capture { done, .. } => done(Err(error)),
            Self::Flow { done, .. } => done(Err(error)),
            Self::FlowBatch { done, .. }
            | Self::CreateBatch { done, .. }
            | Self::RunSteps { done, .. } => done(Err(error)),
            Self::ValueRead { done, .. } => done(Err(error)),
            Self::Policy { done, .. } => done(Err(error)),
            Self::Cleanup { done, .. } => done(Err(error)),
            Self::Read { done, .. } => done(Err(error)),
            Self::GovernanceList { done, .. } => done(Err(error)),
            Self::Schedule { done, .. } => done(Err(error)),
            Self::Peek { done, .. } => done(Peeked::default()),
            Self::Replay { done, .. } | Self::Live { done, .. } | Self::Quiet { done } => {
                done(Err(error));
            }
            Self::Policies { done } => done(Err(error)),
        }
    }
}
