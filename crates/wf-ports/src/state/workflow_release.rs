//! Bounded queue observations; no release, acknowledgement or freshness proof.
use super::{GovernanceCatalogReadLimits, StateTransactionError};
use crate::log::{CodecError, inspect_release_intent};
use wf_domain::governance::release::{ReleaseIntentRef, ReleaseOutboxState};
use wf_domain::workflow::request::RequestScope;

/// Bounded original physical state/binding facts, without payload/history clones.
#[derive(Debug, Eq, PartialEq)]
pub enum WorkflowReleaseRecord {
    /// Actual missing, distinct from unavailability.
    Missing,
    /// Decoded map fields, preserving absent state/reservation facts.
    Record {
        /// Physical source state, preserving absence.
        state: Option<Vec<u8>>,
        /// Actual original governance reservation, preserving absence.
        reservation_id: Option<Vec<u8>>,
    },
    /// The configured read lifecycle could not resolve this record.
    Unavailable,
    /// The underlying record or binding was malformed.
    Corrupt,
}
impl WorkflowReleaseRecord {
    /// Borrows exact loaded facts without implying current read authority.
    #[must_use]
    pub fn observation(&self) -> wf_domain::governance::release::ReleaseRecordObservation<'_> {
        use wf_domain::governance::release::ReleaseRecordObservation as O;
        match self {
            Self::Missing => O::Missing,
            Self::Record {
                state,
                reservation_id,
            } => O::Record {
                state: state.as_deref(),
                reservation_id: reservation_id.as_deref(),
            },
            Self::Unavailable => O::Unavailable,
            Self::Corrupt => O::Corrupt,
        }
    }
}
/// Configured namespace and release-authorizing current record read lifecycle.
pub trait WorkflowReleaseReadPort {
    /// Reads an inclusive cursor page; stale/beyond-tail positions restart at head.
    /// # Errors
    /// Refuses namespace/capability, corrupt data or cumulative read admission.
    fn workflow_release_page(
        &mut self,
        scope: &RequestScope,
        start: Option<u64>,
        limit: u16,
        limits: GovernanceCatalogReadLimits,
    ) -> Result<WorkflowReleasePage, StateTransactionError>;
    /// Reads exact original optional partition and ID order under accepted current reads.
    /// Production requires leader/read-barrier and applied-position lifecycle.
    /// Nil and present-empty partition are distinct routing inputs. Malformed
    /// cardinality is never evidence of Missing. Every allocation must be admitted.
    /// # Errors
    /// Defaults closed until the configured owner can establish current authority.
    fn workflow_release_records_for_release(
        &mut self,
        scope: &RequestScope,
        partition: Option<&[u8]>,
        ids: &[&[u8]],
        limits: GovernanceCatalogReadLimits,
    ) -> Result<Vec<WorkflowReleaseRecord>, StateTransactionError> {
        let _ = (scope, partition, ids, limits);
        Err(StateTransactionError::UnsupportedPolicyProfile)
    }
}

/// One original native intent owner and its same-revision completion marker.
#[derive(Debug, Eq, PartialEq)]
pub struct WorkflowReleaseEntry {
    /// Positive original sequence within the observed pending range.
    pub sequence: u64,
    /// Sparse completion observation; no acknowledgement authority.
    pub completed: bool,
    encoded: Vec<u8>,
}
impl WorkflowReleaseEntry {
    /// Checks complete original bytes before taking their existing owner.
    /// # Errors
    /// Refuses malformed fields/EOF or an invalid sequence.
    pub fn new(sequence: u64, completed: bool, encoded: Vec<u8>) -> Result<Self, CodecError> {
        if !(1..=wf_domain::workflow::MAX_EXACT_MILLIS).contains(&sequence) {
            return Err(CodecError::Malformed);
        }
        inspect_release_intent(&encoded, encoded.len())?;
        Ok(Self {
            sequence,
            completed,
            encoded,
        })
    }
    /// Borrows the original complete bytes without copying any payload.
    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }
    /// Inspects a borrowed original intent; no identity is regenerated.
    /// # Errors
    /// Refuses malformed original bytes, with the same complete codec checks.
    pub fn intent(&self) -> Result<ReleaseIntentRef<'_>, CodecError> {
        inspect_release_intent(&self.encoded, self.encoded.len())
    }
}

/// At most256 consecutive entries from one observed queue revision.
pub struct WorkflowReleasePage {
    /// Same-revision finite scalar queue state.
    pub meta: ReleaseOutboxState,
    /// Original ascending sequence and byte owners, including completed entries.
    pub entries: Vec<WorkflowReleaseEntry>,
    /// More pending sequences follow this page in that same revision.
    pub more: bool,
}
