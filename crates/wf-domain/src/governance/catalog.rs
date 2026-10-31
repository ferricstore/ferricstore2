//! Mathematical publication queue values; none establish commit or authority.

use crate::workflow::MAX_EXACT_MILLIS as MAX;

mod ack;
pub use ack::{
    CreditCatalogAckPlan, CreditCatalogAckRejection, CreditCatalogAckReply,
    CreditCatalogAckRequest, prepare_catalog_ack,
};
use std::fmt;

/// Canonical monotone publication queue header, independent of stored origin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditCatalogMeta {
    head: u64,
    tail: u64,
}

impl CreditCatalogMeta {
    /// Mathematical empty queue, without any allocated sequence.
    #[must_use]
    pub const fn empty() -> Self {
        Self { head: 1, tail: 0 }
    }

    /// Restores a canonical header; does not verify any stored queue entries.
    /// # Errors
    /// Refuses noncanonical or unsupported exact-integer ranges.
    pub fn restore(head: u64, tail: u64) -> Result<Self, CreditCatalogError> {
        if head == 0 || tail > MAX || head > MAX + 1 || head > tail + 1 {
            return Err(CreditCatalogError::CorruptMetadata);
        }
        Ok(Self { head, tail })
    }

    /// First pending sequence, possibly `MAX_EXACT+1` after the last ack.
    #[must_use]
    pub const fn head(self) -> u64 {
        self.head
    }

    /// Greatest allocated sequence; never reset after acknowledgement.
    #[must_use]
    pub const fn tail(self) -> u64 {
        self.tail
    }

    /// Proposes one append without copying the original owner identity.
    /// # Errors
    /// Refuses invalid owner bytes, full backlog or exhausted sequence space.
    pub fn append(
        self,
        owner: &[u8],
        maximum_key_bytes: usize,
    ) -> Result<(Self, u64), CreditCatalogError> {
        valid_owner(owner, maximum_key_bytes)?;
        if self.tail + 1 - self.head >= 65_536 {
            return Err(CreditCatalogError::BacklogFull);
        }
        if self.tail == MAX {
            return Err(CreditCatalogError::SequenceExhausted);
        }
        let sequence = self.tail + 1;
        Ok((
            Self {
                head: self.head,
                tail: sequence,
            },
            sequence,
        ))
    }

    /// Proposes a bounded acknowledgement, preserving already-acked retries.
    /// # Errors
    /// Refuses invalid scalar controls, stale cursors or oversized ranges.
    pub fn acknowledge(
        self,
        expected_head: u64,
        up_to: u64,
    ) -> Result<CreditCatalogAcknowledgement, CreditCatalogError> {
        if !(1..=MAX + 1).contains(&expected_head) || !(1..=MAX).contains(&up_to) {
            return Err(CreditCatalogError::CorruptMetadata);
        }
        if up_to < self.head {
            return Ok(CreditCatalogAcknowledgement {
                next: self,
                first: self.head,
                count: 0,
            });
        }
        if expected_head != self.head {
            return Err(CreditCatalogError::StaleCursor);
        }
        if up_to > self.tail || up_to + 1 - self.head > 256 {
            return Err(CreditCatalogError::InvalidAcknowledgement);
        }
        Ok(CreditCatalogAcknowledgement {
            next: Self {
                head: up_to + 1,
                tail: self.tail,
            },
            first: self.head,
            count: u16::try_from(up_to + 1 - self.head)
                .map_err(|_| CreditCatalogError::InvalidAcknowledgement)?,
        })
    }
}

/// Bounded, allocation-free acknowledgement effects, without storage authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditCatalogAcknowledgement {
    next: CreditCatalogMeta,
    first: u64,
    count: u16,
}

impl CreditCatalogAcknowledgement {
    /// Proposed replacement metadata.
    #[must_use]
    pub const fn next(self) -> CreditCatalogMeta {
        self.next
    }
    /// Number of actual proposed deletions, at most256.
    #[must_use]
    pub const fn len(self) -> u16 {
        self.count
    }
    /// Whether this is an already-acknowledged no-op.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.count == 0
    }
    /// Visits exact deletion sequences, without an allocated vector.
    pub fn sequences(self) -> impl ExactSizeIterator<Item = u64> {
        (0..self.count).map(move |offset| self.first + u64::from(offset))
    }
}

/// Exact mathematical header and tail point, with no provenance/payment claim.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditCatalogObservation<'a> {
    /// Complete original header.
    pub meta: CreditCatalogMeta,
    /// Original pending tail owner, required only when the queue is nonempty.
    pub tail_scope: Option<&'a [u8]>,
}

impl CreditCatalogObservation<'_> {
    /// Proposes an append, or reuse of an identical pending tail owner.
    /// # Errors
    /// Refuses missing/invalid tail observations or append business ceilings.
    pub fn publication(
        self,
        owner: &[u8],
        maximum_key_bytes: usize,
    ) -> Result<Option<CreditCatalogMeta>, CreditCatalogError> {
        valid_owner(owner, maximum_key_bytes)?;
        if self.meta.head > self.meta.tail {
            if self.tail_scope.is_some() {
                return Err(CreditCatalogError::CorruptEntry);
            }
        } else {
            let tail = self.tail_scope.ok_or(CreditCatalogError::CorruptEntry)?;
            valid_owner(tail, maximum_key_bytes).map_err(|_| CreditCatalogError::CorruptEntry)?;
            if tail == owner {
                return Ok(None);
            }
        }
        self.meta
            .append(owner, maximum_key_bytes)
            .map(|(meta, _)| Some(meta))
    }
}

fn valid_owner(owner: &[u8], maximum: usize) -> Result<(), CreditCatalogError> {
    if owner.is_empty() || owner.len() > maximum {
        Err(CreditCatalogError::CorruptMetadata)
    } else {
        Ok(())
    }
}

/// Source publication queue diagnosis; operational adapter failures are separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCatalogError {
    /// Header or owner identity is outside its canonical scalar/byte range.
    CorruptMetadata,
    /// A required active entry is missing or malformed.
    CorruptEntry,
    /// The65536 pending-entry ceiling was reached.
    BacklogFull,
    /// No further source-exact positive sequence can be allocated.
    SequenceExhausted,
    /// An unacknowledged request starts from an obsolete cursor.
    StaleCursor,
    /// Acknowledgement exceeds the tail or256-entry page ceiling.
    InvalidAcknowledgement,
}

impl fmt::Display for CreditCatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CorruptMetadata => "ERR flow limit catalog publication outbox is corrupt",
            Self::CorruptEntry => "ERR flow limit catalog publication entry is missing or corrupt",
            Self::BacklogFull => "ERR flow limit catalog publication backlog is full",
            Self::SequenceExhausted => "ERR flow limit catalog publication sequence is exhausted",
            Self::StaleCursor => "ERR stale flow limit catalog publication cursor",
            Self::InvalidAcknowledgement => {
                "ERR invalid flow limit catalog publication acknowledgement"
            }
        })
    }
}
impl std::error::Error for CreditCatalogError {}
