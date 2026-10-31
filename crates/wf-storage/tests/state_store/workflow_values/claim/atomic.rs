//! Actual atomic parent: every phase shares one writer and one original receipt.
use super::*;
pub(super) mod fixtures;
mod payment;
mod phases;
mod policy;
mod rollback;
