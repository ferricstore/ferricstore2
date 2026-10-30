//! Independent healthy vectors for the private identified owner protocol.

#![forbid(unsafe_code)]

mod framing;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
