//! Current policy backing and represented record acquired on one committed revision.
use super::{
    StateTransactionError, StoredPolicyValue, WorkflowRecordReadLimits, WorkflowRecordReadQuery,
    WorkflowRecordReadSnapshot,
};
use std::fmt;
use wf_domain::policy::complete::PolicySchemaLimits;
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::{PartitionId, RecordId};

/// Current policy observation, distinct from a record with no known type.
pub enum WorkflowRecordPolicyObservation {
    /// No record existed at the selected point, so no policy was read.
    NotRead,
    /// The actual record type has no current policy on this revision.
    Absent,
    /// Complete verified backing for the actual record type on this revision.
    Present(StoredPolicyValue),
}

/// One coherent physical record/control observation and its current policy.
/// For historical records this is the policy now, not the historical write policy.
pub struct WorkflowRecordPolicyReadSnapshot {
    /// Same-revision core, metadata, value binding, control and aggregate accounting.
    pub record: WorkflowRecordReadSnapshot,
    /// Current policy on that same revision, without birth-witness substitution.
    pub current_policy: WorkflowRecordPolicyObservation,
}

/// Independent whole-read limits plus the reached policy's schema profile.
#[derive(Clone, Copy)]
pub struct WorkflowRecordPolicyReadLimits {
    /// Aggregate physical, returned, work and temporary ceilings for both observations.
    pub record: WorkflowRecordReadLimits,
    /// Policy grammar/represented-work ceiling, further capped by remaining read work.
    pub schema: PolicySchemaLimits,
}
impl fmt::Debug for WorkflowRecordPolicyReadSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkflowRecordPolicyReadSnapshot(<redacted>)")
    }
}

/// Coherent physical policy/record reads; service admission and barriers are external.
pub trait WorkflowRecordPolicyReadPort {
    /// Reads the selected core and its current type policy on one committed snapshot.
    /// Aggregate read limits cover both owners; schema work shares remaining read work.
    /// # Errors
    /// Preserves corruption, profile, control, schema, resource and I/O refusals.
    fn read_workflow_record_with_policy(
        &self,
        scope: &RequestScope,
        partition: &PartitionId,
        id: &RecordId,
        query: WorkflowRecordReadQuery<'_>,
        limits: WorkflowRecordPolicyReadLimits,
    ) -> Result<WorkflowRecordPolicyReadSnapshot, StateTransactionError>;
}
