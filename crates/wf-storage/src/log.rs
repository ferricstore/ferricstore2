//! Log boundary for `wf-storage`.
//!
//! Scoped synchronous durability, without consensus or state-machine apply.

mod format;
mod purge;
mod read;
mod recovery;
mod store;
mod write;

pub use store::RedbLogStore;
