//! State boundary for `wf-storage`.
//!
//! Pure persisted formats and an explicitly scoped exclusive physical owner.

pub mod key;
pub(crate) mod redb;
pub(crate) use redb::snapshot;
pub mod value;
pub use redb::{
    RedbStateDueCursor, RedbStateStore, RedbStateTransaction, StateStoreGenesis, StateStoreLimits,
};
