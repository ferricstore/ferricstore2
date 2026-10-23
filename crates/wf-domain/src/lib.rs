//! Pure, deterministic single-record workflow transitions.
//!
//! [`workflow::reduce`] accepts an authoritative record and explicit committed
//! inputs. Its write description requires atomic persistence by a future adapter.
//! [`workflow::reduce_with_policy`] also resolves a borrowed installed retry-only
//! policy. Bounded batch, committed-control and scoped retained-request plans
//! live under [`workflow`]; an outer adapter must persist their effects atomically.

#![forbid(unsafe_code)]

pub mod claim;
pub mod history;
pub mod lease;
pub mod lineage;
pub mod metadata;
pub mod options;
pub mod policy;
pub mod retention;
pub mod retry;
pub mod schedule;
pub mod signal;
pub mod value;
pub mod workflow;
