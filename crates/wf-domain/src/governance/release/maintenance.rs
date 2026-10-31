//! Bounded original maintenance commands; plans grant no remote-release authority.
use super::{
    ReleaseAckPlan, ReleaseCompletionPlan, ReleaseOutboxError as E, ReleaseOutboxState,
    acknowledge_release_outbox, prepare_release_completion, validate_release_state,
};
use crate::workflow::MAX_EXACT_MILLIS as MAX;
use std::fmt;

/// One original trusted maintenance command and its bounded sequence owners.
#[derive(Debug, Eq, PartialEq)]
pub enum ReleaseMaintenanceRequest {
    /// Remove one acknowledged prefix; completion markers are not a prerequisite.
    Acknowledge {
        /// Original observed first sequence.
        expected_head: u64,
        /// Last sequence of the caller's acknowledged contiguous prefix.
        up_to: u64,
    },
    /// Remember sparse successful releases without advancing the queue head.
    MarkCompleted {
        /// Complete unique original sequence order, at most256 elements.
        sequences: Vec<u64>,
    },
}
impl ReleaseMaintenanceRequest {
    /// Validates private scalar/cohort admission, without current queue authority.
    /// # Errors
    /// Refuses zero, nonexact, reversed/oversized or duplicate original controls.
    pub fn validate(&self) -> Result<(), ReleaseMaintenanceRejection> {
        match self {
            Self::Acknowledge {
                expected_head,
                up_to,
            } => {
                if *expected_head == 0
                    || *up_to > MAX
                    || up_to < expected_head
                    || up_to - expected_head >= 256
                {
                    Err(ReleaseMaintenanceRejection::InvalidAcknowledgment)
                } else {
                    Ok(())
                }
            }
            Self::MarkCompleted { sequences } => validate_release_completion_sequences(sequences),
        }
    }
}
/// Business-only original maintenance refusal; corruption is unrepresentable here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseMaintenanceRejection {
    /// Still-pending prefix has a different current head.
    StaleCursor,
    /// Original ACK range or current tail cannot admit the requested prefix.
    InvalidAcknowledgment,
    /// Original completion cohort is invalid or exceeds the observed tail.
    InvalidCompletion,
}
impl fmt::Display for ReleaseMaintenanceRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(
            &match self {
                Self::StaleCursor => E::StaleCursor,
                Self::InvalidAcknowledgment => E::InvalidAcknowledgment,
                Self::InvalidCompletion => E::InvalidCompletion,
            },
            f,
        )
    }
}
impl std::error::Error for ReleaseMaintenanceRejection {}
/// Original observed mathematical response; no durable or remote-release proof.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum ReleaseMaintenanceReply {
    /// Applied mathematical ACK or completion, including idempotent no-op.
    Accepted {
        /// Original resulting scalar state.
        meta: ReleaseOutboxState,
        /// Selected pending sequences, including already-existing completion marks.
        affected: u16,
    },
    /// Original deterministic refusal with unchanged observed meta.
    Rejected {
        /// Original unchanged state.
        meta: ReleaseOutboxState,
        /// Business-only original diagnosis.
        reason: ReleaseMaintenanceRejection,
    },
}
impl fmt::Debug for ReleaseMaintenanceReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Accepted { meta, affected } => f
                .debug_struct("Accepted")
                .field("meta", &(meta.head, meta.tail))
                .field("affected", affected)
                .finish(),
            Self::Rejected { meta, reason } => f
                .debug_struct("Rejected")
                .field("meta", &(meta.head, meta.tail))
                .field("reason", reason)
                .finish(),
        }
    }
}
enum ReleaseMaintenanceEffect<'a> {
    Acknowledge(ReleaseAckPlan),
    MarkCompleted(ReleaseCompletionPlan<'a>),
}
/// Immutable bounded effects; the adapter must repeat actual guarded validation.
pub struct ReleaseMaintenancePlan<'a> {
    original: ReleaseOutboxState,
    request: &'a ReleaseMaintenanceRequest,
    decision: Result<ReleaseMaintenanceEffect<'a>, ReleaseMaintenanceRejection>,
    affected: u16,
}
impl<'a> ReleaseMaintenancePlan<'a> {
    /// Original observed meta, without stored provenance.
    #[must_use]
    pub const fn original(&self) -> ReleaseOutboxState {
        self.original
    }
    /// Complete original immutable command.
    #[must_use]
    pub const fn request(&self) -> &'a ReleaseMaintenanceRequest {
        self.request
    }
    /// Exact bounded ACK range, absent for completion or rejection.
    #[must_use]
    pub const fn acknowledgement(&self) -> Option<&ReleaseAckPlan> {
        match &self.decision {
            Ok(ReleaseMaintenanceEffect::Acknowledge(plan)) => Some(plan),
            _ => None,
        }
    }
    /// Exact bounded pending completions in original order.
    #[must_use]
    pub const fn completion(&self) -> Option<&ReleaseCompletionPlan<'a>> {
        match &self.decision {
            Ok(ReleaseMaintenanceEffect::MarkCompleted(plan)) => Some(plan),
            _ => None,
        }
    }
    /// Complete original neutral reply; not evidence of an actual commit.
    #[must_use]
    pub fn reply(&self) -> ReleaseMaintenanceReply {
        match &self.decision {
            Ok(effect) => ReleaseMaintenanceReply::Accepted {
                affected: self.affected,
                meta: match effect {
                    ReleaseMaintenanceEffect::Acknowledge(plan) => plan.next(),
                    ReleaseMaintenanceEffect::MarkCompleted(_) => self.original,
                },
            },
            Err(reason) => ReleaseMaintenanceReply::Rejected {
                meta: self.original,
                reason: *reason,
            },
        }
    }
}
/// Validates every original completion ID before old IDs may be filtered out.
/// # Errors
/// Refuses empty/oversized, nonexact or duplicate sequence cohorts.
pub fn validate_release_completion_sequences(
    sequences: &[u64],
) -> Result<(), ReleaseMaintenanceRejection> {
    if sequences.is_empty()
        || sequences.len() > 256
        || sequences.iter().any(|n| !(1..=MAX).contains(n))
    {
        return Err(ReleaseMaintenanceRejection::InvalidCompletion);
    }
    let mut ordered = [0_u64; 256];
    ordered[..sequences.len()].copy_from_slice(sequences);
    let ordered = &mut ordered[..sequences.len()];
    ordered.sort_unstable();
    if ordered.windows(2).any(|p| p[0] == p[1]) {
        return Err(ReleaseMaintenanceRejection::InvalidCompletion);
    }
    Ok(())
}
/// Plans source prefix/completion behavior from explicit mathematical observations.
/// # Errors
/// Returns actual corrupt meta as an operational failure, never as a business reply.
pub fn prepare_release_maintenance(
    meta: ReleaseOutboxState,
    request: &ReleaseMaintenanceRequest,
) -> Result<ReleaseMaintenancePlan<'_>, E> {
    validate_release_state(meta)?;
    let decision = if let Err(reason) = request.validate() {
        Err(reason)
    } else {
        let effect = match request {
            ReleaseMaintenanceRequest::Acknowledge {
                expected_head,
                up_to,
            } => acknowledge_release_outbox(meta, *expected_head, *up_to)
                .map(ReleaseMaintenanceEffect::Acknowledge),
            ReleaseMaintenanceRequest::MarkCompleted { sequences } => {
                prepare_release_completion(meta, sequences)
                    .map(ReleaseMaintenanceEffect::MarkCompleted)
            }
        };
        match effect {
            Ok(effect) => Ok(effect),
            Err(E::StaleCursor) => Err(ReleaseMaintenanceRejection::StaleCursor),
            Err(E::InvalidAcknowledgment) => {
                Err(ReleaseMaintenanceRejection::InvalidAcknowledgment)
            }
            Err(E::InvalidCompletion) => Err(ReleaseMaintenanceRejection::InvalidCompletion),
            Err(error) => return Err(error),
        }
    };
    let count = match &decision {
        Ok(ReleaseMaintenanceEffect::Acknowledge(plan)) => plan.sequences().count(),
        Ok(ReleaseMaintenanceEffect::MarkCompleted(plan)) => plan.sequences().count(),
        Err(_) => 0,
    };
    let affected = u16::try_from(count).map_err(|_| E::Corrupt)?;
    Ok(ReleaseMaintenancePlan {
        original: meta,
        request,
        decision,
        affected,
    })
}
