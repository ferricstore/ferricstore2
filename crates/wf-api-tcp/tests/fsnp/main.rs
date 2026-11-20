//! FSNP v1 codec conformance against frames captured from the Elixir server and
//! the Python SDK (ADR-011). Fixtures are request and response bodies; the
//! 24-byte header is covered separately.
#![forbid(unsafe_code)]

mod frame;
mod requests;
mod responses;
mod robustness;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const CREATE_MANY: &[u8] = include_bytes!("fixtures/sdk_create_many_request.bin");
const CREATE: &[u8] = include_bytes!("fixtures/sdk_create_request.bin");
const CLAIM_DUE: &[u8] = include_bytes!("fixtures/sdk_claim_due_request.bin");
const COMPLETE_MANY: &[u8] = include_bytes!("fixtures/sdk_complete_many_request.bin");
const COMPLETE: &[u8] = include_bytes!("fixtures/sdk_complete_request.bin");
