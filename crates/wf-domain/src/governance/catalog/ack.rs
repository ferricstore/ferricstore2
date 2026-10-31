//! Bounded acknowledgement decisions; mathematical plans establish no authority.

use super::{CreditCatalogAcknowledgement, CreditCatalogError, CreditCatalogMeta};
use std::fmt;

/// Original immutable acknowledgement controls within one routed namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditCatalogAckRequest {
    /// Original first sequence observed by the publisher.
    pub expected_head: u64,
    /// Last sequence of the successfully published contiguous prefix.
    pub up_to: u64,
}

impl CreditCatalogAckRequest {
    /// Checks source command scalar/order/page controls, without admission.
    /// # Errors
    /// Refuses nonpositive/nonexact, reversed or oversized ranges.
    pub const fn validate(self) -> Result<(), CreditCatalogAckRejection> {
        if self.expected_head == 0
            || self.up_to > crate::workflow::MAX_EXACT_MILLIS
            || self.up_to < self.expected_head
            || self.up_to - self.expected_head >= 256
        {
            Err(CreditCatalogAckRejection::InvalidRange)
        } else {
            Ok(())
        }
    }
}

/// Business-only ACK diagnosis; operational/corrupt failures cannot appear here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCatalogAckRejection {
    /// A still-unacknowledged request uses an obsolete first sequence.
    StaleCursor,
    /// Scalar/range controls exceed the command or current tail.
    InvalidRange,
}

impl fmt::Display for CreditCatalogAckRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(
            &match self {
                Self::StaleCursor => CreditCatalogError::StaleCursor,
                Self::InvalidRange => CreditCatalogError::InvalidAcknowledgement,
            },
            f,
        )
    }
}
impl std::error::Error for CreditCatalogAckRejection {}

/// Original private ACK response; not proof that any catalog write committed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCatalogAckReply {
    /// Original successful acknowledgement, including already-acked no-op.
    Accepted {
        /// Original resulting queue header.
        meta: CreditCatalogMeta,
        /// Original actual deletion count, at most256.
        deleted: u16,
    },
    /// Original deterministic refusal with the unchanged observed header.
    Rejected {
        /// Original unchanged queue header.
        meta: CreditCatalogMeta,
        /// Original business diagnosis.
        reason: CreditCatalogAckRejection,
    },
}

/// Sealed bounded mathematical effects; only an actual writer can publish them.
#[derive(Debug)]
pub struct CreditCatalogAckPlan {
    original: CreditCatalogMeta,
    request: CreditCatalogAckRequest,
    decision: Result<CreditCatalogAcknowledgement, CreditCatalogAckRejection>,
}

impl CreditCatalogAckPlan {
    /// Original mathematical header, without stored provenance.
    #[must_use]
    pub const fn original(&self) -> CreditCatalogMeta {
        self.original
    }
    /// Original immutable controls.
    #[must_use]
    pub const fn request(&self) -> CreditCatalogAckRequest {
        self.request
    }
    /// Bounded proposed effects, absent on a deterministic rejection.
    #[must_use]
    pub const fn acknowledgement(&self) -> Option<CreditCatalogAcknowledgement> {
        match self.decision {
            Ok(ack) => Some(ack),
            Err(_) => None,
        }
    }
    /// Original represented private reply, without durable success authority.
    #[must_use]
    pub const fn reply(&self) -> CreditCatalogAckReply {
        match self.decision {
            Ok(ack) => CreditCatalogAckReply::Accepted {
                meta: ack.next(),
                deleted: ack.len(),
            },
            Err(reason) => CreditCatalogAckReply::Rejected {
                meta: self.original,
                reason,
            },
        }
    }
}

/// Prepares one bounded ACK from explicit observed mathematical data, without I/O.
#[must_use]
pub fn prepare_catalog_ack(
    meta: CreditCatalogMeta,
    request: CreditCatalogAckRequest,
) -> CreditCatalogAckPlan {
    let decision = request.validate().and_then(|()| {
        meta.acknowledge(request.expected_head, request.up_to)
            .map_err(|error| match error {
                CreditCatalogError::StaleCursor => CreditCatalogAckRejection::StaleCursor,
                _ => CreditCatalogAckRejection::InvalidRange,
            })
    });
    CreditCatalogAckPlan {
        original: meta,
        request,
        decision,
    }
}
