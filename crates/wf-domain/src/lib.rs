//! Pure, deterministic single-record workflow transitions.
//!
//! [`workflow::reduce`] accepts an authoritative record and explicit committed
//! inputs. Its write description requires atomic persistence by a future adapter.
//! [`workflow::reduce_with_policy`] also resolves a borrowed installed retry-only
//! policy. These plans provide no durability or request-ID outcome replay.

#![forbid(unsafe_code)]

pub mod claim;
pub mod history;
pub mod lease;
pub mod lineage;
pub mod policy;
pub mod retention;
pub mod retry;
pub mod schedule;
pub mod signal;
pub mod value;
pub mod workflow;
