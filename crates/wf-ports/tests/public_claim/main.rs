//! Public canonical comparison is separate from private prepared frame identity.
#![forbid(unsafe_code)]
mod frame;
mod intent;
mod response;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
