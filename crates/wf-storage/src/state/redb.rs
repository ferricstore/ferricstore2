//! Exclusive scoped redb authority with explicit bootstrap and strict recovery.

use wf_domain::policy::complete::{CompletePolicyValue, PolicySchemaLimits};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_ports::consensus::RaftMembershipLimits;
use wf_ports::snapshot::SnapshotLimits;
use wf_ports::state::StateTransactionError;
use wf_types::identity::{GroupId, PartitionId, TenantId};
use wf_types::version::CommittedTime;

mod admission;
mod bootstrap;
mod commit;
mod cursor;
mod membership;
mod publication;
mod read;
mod recovery;
pub(crate) mod snapshot;
mod store;

pub use cursor::RedbStateDueCursor;
pub use store::{RedbStateStore, RedbStateTransaction};

const META: redb::TableDefinition<'static, &[u8], &[u8]> =
    redb::TableDefinition::new("wf_state_meta_v1");
const ROWS: redb::TableDefinition<'static, &[u8], &[u8]> =
    redb::TableDefinition::new("wf_state_rows_v1");

/// Borrowed immutable genesis. No applied records or retained replies can be seeded.
#[derive(Clone, Copy)]
pub struct StateStoreGenesis<'g> {
    /// Original unapplied group clock.
    pub initial_time: CommittedTime,
    /// Strictly partition-sorted recovery-wide routing, including foreign groups.
    pub placements: &'g [(PartitionId, GroupId)],
    /// Strictly (tenant,type)-sorted complete normalized policy donors.
    pub policies: &'g [(TenantId, CompletePolicyValue)],
}

/// Immutable supported data, bootstrap, recovery and temporary-owner ceilings.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct StateStoreLimits {
    admission: AdmissionLimits,
    schema: PolicySchemaLimits,
    genesis: (usize, usize),
    recovery: (usize, usize, usize),
    temporary: usize,
    workspace: usize,
    membership: RaftMembershipLimits,
    snapshot: SnapshotLimits,
}

impl StateStoreLimits {
    /// Sets positive row/byte/work ceilings; a small positive W may refuse work.
    /// # Errors
    /// Refuses zero caps or a schema work allowance larger than temporary space.
    pub fn new(
        admission: AdmissionLimits,
        schema: PolicySchemaLimits,
        genesis: (usize, usize),
        recovery: (usize, usize, usize),
        temporary_bytes: usize,
        workspace_bytes: usize,
    ) -> Result<Self, StateTransactionError> {
        let work = usize::try_from(schema.max_work_bytes())
            .map_err(|_| StateTransactionError::InvalidLimits)?;
        if [
            genesis.0,
            genesis.1,
            recovery.0,
            recovery.1,
            recovery.2,
            temporary_bytes,
            workspace_bytes,
        ]
        .contains(&0)
            || work > temporary_bytes
        {
            return Err(StateTransactionError::InvalidLimits);
        }
        Ok(Self {
            admission,
            schema,
            genesis,
            recovery,
            temporary: temporary_bytes,
            workspace: workspace_bytes,
            membership: RaftMembershipLimits::default(),
            snapshot: SnapshotLimits::default(),
        })
    }
    /// Supported persisted field and decision profile.
    #[must_use]
    pub const fn admission(self) -> AdmissionLimits {
        self.admission
    }
    /// Supported complete policy profile.
    #[must_use]
    pub const fn schema(self) -> PolicySchemaLimits {
        self.schema
    }
    /// Maximum immutable input rows, excluding fixed manifest/control.
    #[must_use]
    pub const fn maximum_genesis_rows(self) -> usize {
        self.genesis.0
    }
    /// Maximum simultaneous borrowed donor-owner bytes.
    #[must_use]
    pub const fn maximum_genesis_bytes(self) -> usize {
        self.genesis.1
    }
    /// Maximum reached recovery rows, including revisits.
    #[must_use]
    pub const fn maximum_recovery_rows(self) -> usize {
        self.recovery.0
    }
    /// Maximum reached full key/value recovery bytes, including revisits.
    #[must_use]
    pub const fn maximum_recovery_bytes(self) -> usize {
        self.recovery.1
    }
    /// Maximum recovery open/probe/advance work.
    #[must_use]
    pub const fn maximum_recovery_steps(self) -> usize {
        self.recovery.2
    }
    /// Cumulative temporary-owner ceiling.
    #[must_use]
    pub const fn maximum_temporary_bytes(self) -> usize {
        self.temporary
    }
    /// Complete constructor workspace ceiling.
    #[must_use]
    pub const fn workspace_bytes(self) -> usize {
        self.workspace
    }
    /// Supported complete applied-membership data and ownership profile.
    #[must_use]
    pub const fn membership_limits(self) -> RaftMembershipLimits {
        self.membership
    }
    /// Selects the actual persisted membership profile without changing other caps.
    #[must_use]
    pub const fn with_membership_limits(mut self, limits: RaftMembershipLimits) -> Self {
        self.membership = limits;
        self
    }
    /// Captured immutable snapshot data profile and default operation budgets.
    #[must_use]
    pub const fn snapshot_limits(self) -> SnapshotLimits {
        self.snapshot
    }
    /// Selects snapshot ceilings without changing workflow or membership data.
    #[must_use]
    pub const fn with_snapshot_limits(mut self, limits: SnapshotLimits) -> Self {
        self.snapshot = limits;
        self
    }
}

impl Default for StateStoreLimits {
    fn default() -> Self {
        Self {
            admission: AdmissionLimits::default(),
            schema: PolicySchemaLimits::default(),
            genesis: (1_000, 128 * 1_048_576),
            recovery: (1_000_000, 1_024 * 1_048_576, 10_000_000),
            temporary: 128 * 1_048_576,
            workspace: 256 * 1_048_576,
            membership: RaftMembershipLimits::default(),
            snapshot: SnapshotLimits::default(),
        }
    }
}
