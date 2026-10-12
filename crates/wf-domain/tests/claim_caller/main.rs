//! Public caller comparison controls, without claim admission or authority.
#![forbid(unsafe_code)]

mod admission;
mod normalization;

type TestError = Box<dyn std::error::Error>;
type TestResult<T = ()> = Result<T, TestError>;
