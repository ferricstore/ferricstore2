//! Bounded release-outbox decisions, independent from actual persisted authority.
mod model;
mod queue;
pub use model::{
    GovernanceReservation, LimitEnforcement, ReleaseAckPlan, ReleaseAppend, ReleaseCompletionPlan,
    ReleaseDisposition, ReleaseIntent, ReleaseIntentRef, ReleaseOutboxError, ReleaseOutboxState,
    ReleaseRecordObservation,
};
pub use queue::{
    acknowledge_release_outbox, append_release_outbox, classify_release,
    prepare_release_completion, validate_release_intent, validate_release_state,
};
