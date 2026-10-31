//! Independent healthy vectors for the private identified owner protocol.

#![forbid(unsafe_code)]

mod catalog_ack;
mod framing;
mod view;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
