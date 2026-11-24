//! What a Flow carries beyond its lifecycle record, and the record map the
//! Elixir server replies with (`Ferricstore.Flow.RecordProjection.public/1`).

use std::collections::BTreeMap;

use super::children::ChildGroup;
use super::create::MaxActive;
use super::meta::{Attributes, StateMeta};

mod include;
pub mod retained;
mod view;

pub use include::{DEFAULT_PAYLOAD_MAX_BYTES, Include, Values};
pub use retained::{
    RETAINED_ERROR, RETAINED_NAMED, RETAINED_PAYLOAD, RETAINED_RESULT, Retained, RetainedKey,
    RetainedMap, StoredValue, ValueFiles,
};
pub use view::{View, attached, hydrated, payload_fields};

/// Elixir's default terminal retention: 7 days.
pub const DEFAULT_RETENTION_TTL_MS: u64 = 604_800_000;
/// Elixir's default history cap.
pub const DEFAULT_HISTORY_MAX_EVENTS: u64 = 100_000;

/// One named value of a Flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedValue {
    /// Version, from 1, raised each time the value is replaced.
    pub version: u64,
    /// The stored bytes.
    pub bytes: Vec<u8>,
}

/// A Flow's fields beyond the lifecycle record.
#[derive(Clone, Debug, PartialEq)]
pub struct FlowMeta {
    /// Parent Flow id.
    pub parent_flow_id: Option<Vec<u8>>,
    /// Parent partition.
    pub parent_partition_key: Option<Vec<u8>>,
    /// Root Flow id; the Flow itself when absent.
    pub root_flow_id: Option<Vec<u8>>,
    /// Correlation id.
    pub correlation_id: Option<Vec<u8>>,
    /// Terminal retention.
    pub retention_ttl_ms: u64,
    /// Active-time limit, if set.
    pub max_active_ms: Option<MaxActive>,
    /// Hot history cap.
    pub history_hot_max_events: u64,
    /// History cap.
    pub history_max_events: u64,
    /// Attributes.
    pub attributes: Attributes,
    /// Per-state metadata.
    pub state_meta: StateMeta,
    /// Named values, by name.
    pub values: BTreeMap<Vec<u8>, NamedValue>,
    /// Version of the payload, result and error references.
    pub payload_version: u64,
    /// Version of the result reference.
    pub result_version: u64,
    /// Version of the error reference.
    pub error_version: u64,
    /// Retention set by the terminal command, if any.
    pub ttl_ms: Option<u64>,
    /// When a terminal Flow is removed, set when it became terminal.
    pub terminal_until: Option<u64>,
    /// Signal idempotency keys and the digest of the content each applied.
    pub signal_keys: BTreeMap<Vec<u8>, Vec<u8>>,
    /// Child groups by id.
    pub child_groups: BTreeMap<Vec<u8>, ChildGroup>,
    /// The history event the Flow was last rewound to (ADR-014 step 10).
    pub rewound_to: Option<Vec<u8>>,
    /// The attribute names its type's policy indexed when the Flow was last
    /// written: the attributes broad reads see (ADR-014 step 11).
    pub indexed_attributes: Vec<String>,
    /// Kept in the stored format only: reads take the key from the type's
    /// current policy (audit gap G31), and writes no longer stamp it.
    pub indexed_state_meta: Option<String>,
    /// Effects and governance ledger (ADR-014 step 14).
    pub governance: Option<Box<super::effect::Governance>>,
    /// Replaced value versions (ADR-016 phase C; see [`retained`]).
    pub retained: RetainedMap,
    /// Kept versions no logged after-image carries yet: an after-image logs
    /// only these and replay adds them to the ones the Flow had, so a change
    /// logs the versions it kept, not every one (perf note P2).
    pub retained_unlogged: Vec<RetainedKey>,
    /// References attached by name (`value_refs`, audit gap G9): values
    /// this Flow names but does not own.
    pub value_refs: BTreeMap<Vec<u8>, Vec<u8>>,
    /// A stored value given as the payload (`payload_ref`, audit gap G16);
    /// it replaces the record's own payload until a command sets one.
    pub payload_ref: Option<Vec<u8>>,
}

impl Default for FlowMeta {
    fn default() -> Self {
        Self {
            parent_flow_id: None,
            parent_partition_key: None,
            root_flow_id: None,
            correlation_id: None,
            retention_ttl_ms: DEFAULT_RETENTION_TTL_MS,
            max_active_ms: None,
            history_hot_max_events: 0,
            history_max_events: DEFAULT_HISTORY_MAX_EVENTS,
            attributes: Attributes::new(),
            state_meta: StateMeta::new(),
            values: BTreeMap::new(),
            payload_version: 1,
            result_version: 1,
            error_version: 1,
            ttl_ms: None,
            terminal_until: None,
            signal_keys: BTreeMap::new(),
            child_groups: BTreeMap::new(),
            rewound_to: None,
            indexed_attributes: Vec::new(),
            indexed_state_meta: None,
            governance: None,
            retained: BTreeMap::new(),
            retained_unlogged: Vec::new(),
            value_refs: BTreeMap::new(),
            payload_ref: None,
        }
    }
}
