//! ADR-004 pure clock/checkpoint plans with complete scoped entry identity.

use wf_types::version::{AppliedPosition, CommittedTime, LogIdentity, TimestampMillis};

use super::batch::ApplyError;

/// Explicit digest of private versioned committed bytes, verified by the decoder.
///
/// Digest production stays in outer preparation/storage. This is deliberately
/// distinct from client intent; no hash algorithm or memory-layout codec occurs
/// in the domain. Unverified client bytes cannot construct apply authority.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PreparedDigest([u8; 32]);

impl PreparedDigest {
    /// Preserves an explicit verified digest value.
    #[must_use]
    pub const fn new(value: [u8; 32]) -> Self {
        Self(value)
    }
    /// Borrows the exact immutable digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Full last-entry identity within the authoritative group's recovery scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EntryIdentity {
    log: LogIdentity,
    prepared: PreparedDigest,
}

impl EntryIdentity {
    /// Binds lossless log identity to independently verified committed bytes.
    #[must_use]
    pub const fn new(log: LogIdentity, prepared: PreparedDigest) -> Self {
        Self { log, prepared }
    }
    /// Reads the complete term/leader-node/index identity.
    #[must_use]
    pub const fn log(self) -> LogIdentity {
        self.log
    }
    /// Reads prepared identity independently from client intent.
    #[must_use]
    pub const fn prepared(self) -> PreparedDigest {
        self.prepared
    }
}

/// Explicit clock treatment for an admissible committed entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryStamp {
    /// Prepared validated metadata, used for every stamped entry class.
    Stamped(CommittedTime),
    /// Consensus blank/membership entry; no local stamp is manufactured.
    Unstamped,
}

/// Coherent persisted control values; only plans advance this immutable value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlState {
    position: AppliedPosition,
    time: CommittedTime,
    last_entry: Option<(EntryIdentity, EntryStamp)>,
}

impl ControlState {
    /// Builds genuinely unapplied control; the first log entry must have index zero.
    #[must_use]
    pub const fn initial(time: CommittedTime) -> Self {
        Self {
            position: AppliedPosition::new(0),
            time,
            last_entry: None,
        }
    }
    /// Restores a checkpoint derived from complete last-entry identity and clocks.
    ///
    /// The outer snapshot adapter proves coherent scope/state/index/outcome bytes.
    /// This constructor cannot establish snapshot atomicity or durable application.
    ///
    /// # Errors
    /// Rejects restored clocks below the validated stamped last entry.
    pub fn restored(
        time: CommittedTime,
        identity: EntryIdentity,
        stamp: EntryStamp,
    ) -> Result<Self, ApplyError> {
        if let EntryStamp::Stamped(last) = stamp
            && (time.hlc() < last.hlc() || time.wall() < last.wall())
        {
            return Err(ApplyError::InvalidAuthority);
        }
        Ok(Self {
            position: identity.log.index(),
            time,
            last_entry: Some((identity, stamp)),
        })
    }
    /// Reads the scoped index; zero alone does not prove an entry was applied.
    #[must_use]
    pub const fn position(self) -> AppliedPosition {
        self.position
    }
    /// Reads persisted nondecreasing HLC and wall control time.
    #[must_use]
    pub const fn time(self) -> CommittedTime {
        self.time
    }
    /// Reads full last-entry identity, not only an index.
    #[must_use]
    pub const fn last_entry(self) -> Option<EntryIdentity> {
        match self.last_entry {
            Some((identity, _)) => Some(identity),
            None => None,
        }
    }
}

/// A proposed control update; it provides no persistence guarantee.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlPlan {
    next: ControlState,
    log_replay: bool,
}

impl ControlPlan {
    /// Reads the proposed checkpoint/time state.
    #[must_use]
    pub const fn next(self) -> ControlState {
        self.next
    }
    /// Reads effective execution time from authoritative HLC physical control.
    #[must_use]
    pub const fn execution_time(self) -> TimestampMillis {
        self.next.time.hlc().physical()
    }
    /// Identifies a repeated exact last log entry, with no checkpoint/time change.
    #[must_use]
    pub const fn is_log_replay(self) -> bool {
        self.log_replay
    }
}

/// Plans one consecutive committed entry without clocks, effects or mutation.
///
/// Accepted/rejected/request-replay/tick/stamped-membership all pass their
/// validated stamp here. Equal full last identity/digest/stamp is a log no-op;
/// older replay requires the outer adapter's complete durable identity proof.
/// Group/recovery scope is revalidated by the enclosing command service/domain.
///
/// # Errors
/// Conflicting identities/digests/stamps, gaps and counter overflow are apply
/// failures, never durable workflow business rejections.
pub fn plan_control(
    previous: ControlState,
    identity: EntryIdentity,
    stamp: EntryStamp,
) -> Result<ControlPlan, ApplyError> {
    if previous.last_entry.is_none() {
        if identity.log.index().get() != 0 {
            return Err(ApplyError::PositionConflict);
        }
    } else if identity.log.index() == previous.position {
        if previous.last_entry == Some((identity, stamp)) {
            return Ok(ControlPlan {
                next: previous,
                log_replay: true,
            });
        }
        return Err(ApplyError::PositionConflict);
    } else if previous.position.checked_next() != Some(identity.log.index())
        || previous
            .last_entry
            .is_some_and(|(last, _)| identity.log.term() < last.log.term())
    {
        return Err(ApplyError::PositionConflict);
    }
    let time = match stamp {
        EntryStamp::Unstamped => previous.time,
        EntryStamp::Stamped(time) => CommittedTime::new(
            previous.time.hlc().max(time.hlc()),
            previous.time.wall().max(time.wall()).get(),
        )
        .map_err(|_| ApplyError::InvalidAuthority)?,
    };
    Ok(ControlPlan {
        next: ControlState {
            position: identity.log.index(),
            time,
            last_entry: Some((identity, stamp)),
        },
        log_replay: false,
    })
}
