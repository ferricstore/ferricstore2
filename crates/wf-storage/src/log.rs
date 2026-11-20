//! Log boundary for `wf-storage`.
//!
//! Scoped synchronous durability, without consensus or state-machine apply.

mod format;
mod group;
mod install;
mod purge;
mod read;
mod recovery;
mod store;
mod write;

pub use group::{
    DirectoryStorage, GroupLog, GroupLogError, GroupLogLimits, Recovered, SegmentStorage,
    SegmentWriter,
};
pub use store::RedbLogStore;
