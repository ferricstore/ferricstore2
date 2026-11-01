//! Scoped workflow queries through explicit authorized consistency capabilities.
mod record;
mod record_values;
pub use record::{
    StrongWorkflowRecordError, StrongWorkflowRecordLimits, StrongWorkflowRecordRequest,
    StrongWorkflowRecordResult, query_strong_workflow_record,
};
pub use record_values::{
    StrongWorkflowRecordValueError, StrongWorkflowRecordValueLimits,
    StrongWorkflowRecordValueRequest, StrongWorkflowRecordValueResult,
    query_strong_workflow_record_with_values,
};
