//! Actual retained/Fresh proposals; storage and complete source parity remain open.

#![forbid(unsafe_code)]

mod fixtures;
mod fresh;
mod limits;
mod refusals;
mod replay;
mod views;

use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::indexed::{IndexRejection, IndexedDecision};
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{
    ClientIntent, RequestScope, RequestValueError, RetainedMetadata, RetainedOutcome,
    RetainedStatus,
};
use wf_domain::workflow::{CommittedCommand, Operation, StepContinue, WorkflowRecord};
use wf_ports::blob::{
    RecordBatchValueEffect, RecordBatchValueEffects, RecordBatchValueEffectsError,
    inspect_record_batch_value_effects,
};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{
    CodecError, CodecLimits, DecodedRequest, FreshRecordBatch, PreparedBytes,
    RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchApplyPlan, RecordBatchInspection,
    decide_fresh_atomic_start_touched, decide_fresh_record_batch_touched,
    decide_fresh_step_continue_touched, encode_step_continue, inspect_step_continue_point,
    prepare_step_continue, step_continue_response_matches, step_continue_retained_summary,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, Fence, HlcTimestamp, LogIdentity, TimestampMillis,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const WORKSPACE: usize = 1_048_576;
