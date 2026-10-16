//! Engine-independent persisted values; these tests establish no durability.

#![forbid(unsafe_code)]

mod control;
mod history;
mod independent;
mod outcome;
mod ownership;
mod public_mutation;
mod record;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
