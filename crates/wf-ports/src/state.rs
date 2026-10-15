//! State boundary for `wf-ports`.
//!
//! Source compatibility policy queries use an explicit policy read capability.

mod policy;

pub use policy::{OriginalPolicyGetReader, read_original_policy_get_reply};
