//! Complete capture data, not evidence of a durable writer or source parity.
#![forbid(unsafe_code)]
mod cases;
mod fixtures;
mod resources;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
