//! Independent healthy vectors for the private identified owner protocol.

#![forbid(unsafe_code)]

mod catalog_ack;
mod catalog_member;
mod catalog_membership;
mod catalog_progress;
mod catalog_prune;
mod cleanup;
mod framing;
mod view;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
