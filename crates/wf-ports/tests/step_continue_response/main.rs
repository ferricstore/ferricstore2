//! Actual indexed Step replies; no persistence, Fresh or source-profile credit.

#![forbid(unsafe_code)]

mod fixtures;
mod limits;
mod matching;
mod refusals;
mod wire;

use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::indexed::{IndexRejection, IndexedDecision};
use wf_domain::workflow::request::{ClientIntent, RetainedStatus};
use wf_ports::log::{
    CodecError, atomic_start_retained_summary, encode_step_continue_response,
    record_batch_retained_summary, step_continue_response_matches, step_continue_retained_summary,
};
use wf_types::version::{AppliedPosition, LogIdentity, TimestampMillis};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
