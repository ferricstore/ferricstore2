//! Scoped workflow queries through explicit authorized consistency capabilities.
mod record;
pub use record::{
    StrongWorkflowRecordError, StrongWorkflowRecordLimits, StrongWorkflowRecordRequest,
    StrongWorkflowRecordResult, query_strong_workflow_record,
};
