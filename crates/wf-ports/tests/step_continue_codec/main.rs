//! Explicit intent10 data preparation; durable Step remains a successor gate.

#![forbid(unsafe_code)]

mod core;
mod fixtures;
mod identity;
mod limits;
mod refusals;
mod wire;

use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_domain::workflow::{CommittedCommand, Operation, StepContinue};
use wf_ports::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use wf_ports::log::{
    CodecError, CodecLimits, DecodedRequest, PreparedBytes, RecordBatchApplyError,
    encode_step_continue, prepare_step_continue,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, Fence, HlcTimestamp, TimestampMillis};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const WORKSPACE: usize = 1_048_576;
