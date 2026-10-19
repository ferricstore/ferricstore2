//! State boundary for `wf-storage`.
//!
//! Pure persisted formats and an explicitly scoped exclusive physical owner.

pub mod key;
mod redb;
pub mod value;
pub use redb::{
    RedbStateDueCursor, RedbStateStore, RedbStateTransaction, StateStoreGenesis, StateStoreLimits,
};
