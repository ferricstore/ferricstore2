//! Complete normalized policy ownership, admission, and governance controls.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::ops::Range;

use wf_domain::policy::complete::{
    CompletePolicyCandidate, CompletePolicyData, CompletePolicyValue, CompleteStatePolicyData,
    PolicySchemaCharge, PolicySchemaLimits, restore_complete_policy,
};
use wf_domain::policy::definition::PolicyVersion;
use wf_domain::policy::installation::PolicyLimits;

mod admission;
mod encoding_accounting;
mod encoding_malformed;
mod encoding_vectors;
mod install;
mod patch;
mod presentation;
mod resolution;
mod values;

type TestError = Box<dyn std::error::Error>;
type TestResult<T = ()> = Result<T, TestError>;

fn data(
    workflow_type: Vec<u8>,
    version: Option<PolicyVersion>,
    states: BTreeMap<Vec<u8>, CompleteStatePolicyData>,
) -> CompletePolicyData {
    CompletePolicyData {
        workflow_type,
        version,
        max_active_ms: None,
        retry: None,
        retention: None,
        states,
        indexed_attributes: None,
        indexed_state_meta: None,
        governance: None,
    }
}

fn limits(work: u64) -> TestResult<PolicySchemaLimits> {
    Ok(PolicySchemaLimits::new(PolicyLimits::default(), work)?)
}

fn candidate(
    generation: u64,
    value: CompletePolicyData,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompletePolicyCandidate> {
    Ok(restore_complete_policy(generation, value, limits, charge)?)
}

// Length-only ownership fixture, with no encoded content or authority claim.
fn bind(
    candidate: CompletePolicyCandidate,
    bytes: Vec<u8>,
    range: Range<usize>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> TestResult<CompletePolicyValue> {
    Ok(candidate.bind_encoded_snapshot(range, bytes, limits, charge)?)
}

mod replace_content;

mod indexed_input;

mod original_retry_keys;

mod original_entry;

mod retry_input;

mod retention_input;
