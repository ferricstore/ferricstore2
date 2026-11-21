//! The fields an FQL1 query names, as `Ferricstore.Flow.Query.Field` and the
//! native parser know them: seventeen built-in record fields, attributes and
//! state metadata, and the selectors a `RETURN` projection may list.

/// A query field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// `partition_key`.
    PartitionKey,
    /// `run_id`, the Flow id.
    RunId,
    /// `event_id` of a history event.
    EventId,
    /// `type`.
    Type,
    /// `state`.
    State,
    /// `version`.
    Version,
    /// `priority`.
    Priority,
    /// `created_at_ms`.
    CreatedAtMs,
    /// `updated_at_ms`.
    UpdatedAtMs,
    /// `next_run_at_ms`.
    NextRunAtMs,
    /// `lease_deadline_ms`.
    LeaseDeadlineMs,
    /// `attempts`.
    Attempts,
    /// `run_state`.
    RunState,
    /// `max_active_ms`.
    MaxActiveMs,
    /// `parent_flow_id`.
    ParentFlowId,
    /// `root_flow_id`.
    RootFlowId,
    /// `correlation_id`.
    CorrelationId,
    /// `attribute.<name>`.
    Attribute(String),
    /// `state_meta.<state>.<name>`.
    StateMeta(String, String),
}

/// The type a field's values have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueType {
    /// Text.
    Keyword,
    /// A 64-bit integer.
    Integer,
    /// Any scalar: attributes and state metadata.
    Dynamic,
}

/// What a `RETURN` projection may select.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selector {
    /// A record field.
    Field(Field),
    /// Every attribute.
    Attributes,
    /// All state metadata.
    StateMeta,
    /// An event's id.
    EventId,
    /// An event's fields.
    Fields,
    /// One event field.
    EventField(String),
}

const BUILTINS: [(&str, Field); 17] = [
    ("partition_key", Field::PartitionKey),
    ("run_id", Field::RunId),
    ("event_id", Field::EventId),
    ("type", Field::Type),
    ("state", Field::State),
    ("version", Field::Version),
    ("priority", Field::Priority),
    ("created_at_ms", Field::CreatedAtMs),
    ("updated_at_ms", Field::UpdatedAtMs),
    ("next_run_at_ms", Field::NextRunAtMs),
    ("lease_deadline_ms", Field::LeaseDeadlineMs),
    ("attempts", Field::Attempts),
    ("run_state", Field::RunState),
    ("max_active_ms", Field::MaxActiveMs),
    ("parent_flow_id", Field::ParentFlowId),
    ("root_flow_id", Field::RootFlowId),
    ("correlation_id", Field::CorrelationId),
];

/// Every predicate field name, as the unsupported-field hint lists them.
pub const SUPPORTED_FIELDS: [&str; 19] = [
    "attempts",
    "attribute.<name>",
    "correlation_id",
    "created_at_ms",
    "event_id",
    "lease_deadline_ms",
    "max_active_ms",
    "next_run_at_ms",
    "parent_flow_id",
    "partition_key",
    "priority",
    "root_flow_id",
    "run_id",
    "run_state",
    "state",
    "state_meta.<state>.<name>",
    "type",
    "updated_at_ms",
    "version",
];

/// The fields a runs projection may name.
pub const RUN_PROJECTION_FIELDS: [&str; 20] = [
    "attempts",
    "attribute.<name>",
    "attributes",
    "correlation_id",
    "created_at_ms",
    "lease_deadline_ms",
    "max_active_ms",
    "next_run_at_ms",
    "parent_flow_id",
    "partition_key",
    "priority",
    "root_flow_id",
    "run_id",
    "run_state",
    "state",
    "state_meta",
    "state_meta.<state>.<name>",
    "type",
    "updated_at_ms",
    "version",
];

/// The fields an events projection may name.
pub const EVENT_PROJECTION_FIELDS: [&str; 3] = ["event_id", "fields", "fields['<name>']"];

/// ASCII case-insensitive equality with an uppercase keyword.
pub(super) fn keyword(actual: &[u8], expected: &[u8]) -> bool {
    actual.eq_ignore_ascii_case(expected)
}

/// A dotted metadata name: ASCII letters, digits, `_` and `-`.
fn valid_name(name: &[u8]) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(b"__")
        && name
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// A quoted state name: non-empty UTF-8 of at most 64 bytes.
pub(super) fn valid_state_name(name: &[u8]) -> bool {
    !name.is_empty() && name.len() <= 64 && std::str::from_utf8(name).is_ok()
}

/// A quoted metadata key: a state name that is not reserved.
pub(super) fn valid_key(name: &[u8]) -> bool {
    valid_state_name(name) && !name.starts_with(b"__")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl Field {
    /// The field a bare word names, or `None` when FQL1 has no such field.
    #[must_use]
    pub fn parse(input: &[u8]) -> Option<Self> {
        if let Some((_, field)) = BUILTINS
            .iter()
            .find(|(name, _)| keyword(input, name.as_bytes()))
        {
            return Some(field.clone());
        }
        let mut parts = input.split(|byte| *byte == b'.');
        let prefix = parts.next().unwrap_or_default();
        let first = parts.next()?;
        let second = parts.next();
        if parts.next().is_some() {
            return None;
        }
        match second {
            None if keyword(prefix, b"ATTRIBUTE") && valid_name(first) => {
                Some(Self::Attribute(text(first)))
            }
            Some(name)
                if keyword(prefix, b"STATE_META") && valid_name(first) && valid_name(name) =>
            {
                Some(Self::StateMeta(text(first), text(name)))
            }
            _ => None,
        }
    }

    /// The bracketed attribute field `attribute['<name>']`.
    #[must_use]
    pub fn attribute(name: &[u8]) -> Self {
        Self::Attribute(text(name))
    }

    /// The bracketed field `state_meta['<state>']['<name>']`.
    #[must_use]
    pub fn state_meta(state: &[u8], name: &[u8]) -> Self {
        Self::StateMeta(text(state), text(name))
    }

    /// The type of the field's values.
    #[must_use]
    pub const fn value_type(&self) -> ValueType {
        match self {
            Self::Version
            | Self::Priority
            | Self::CreatedAtMs
            | Self::UpdatedAtMs
            | Self::NextRunAtMs
            | Self::LeaseDeadlineMs
            | Self::Attempts
            | Self::MaxActiveMs => ValueType::Integer,
            Self::Attribute(_) | Self::StateMeta(..) => ValueType::Dynamic,
            _ => ValueType::Keyword,
        }
    }

    /// Whether the field is an attribute or state metadata.
    #[must_use]
    pub const fn is_metadata(&self) -> bool {
        matches!(self, Self::Attribute(_) | Self::StateMeta(..))
    }

    /// Whether the field names a valid metadata key, as `Field.valid?/1`.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Attribute(name) => valid_key(name.as_bytes()),
            Self::StateMeta(state, name) => {
                valid_state_name(state.as_bytes()) && valid_key(name.as_bytes())
            }
            _ => true,
        }
    }

    /// The key of a built-in field in a run record; `run_id` is `id`.
    #[must_use]
    pub fn record_key(&self) -> Option<&'static str> {
        match self {
            Self::RunId => Some("id"),
            Self::Attribute(_) | Self::StateMeta(..) => None,
            builtin => BUILTINS
                .iter()
                .find(|(_, field)| field == builtin)
                .map(|(name, _)| *name),
        }
    }

    /// The field's canonical external name, as EXPLAIN shows it.
    #[must_use]
    pub fn external(&self) -> String {
        match self {
            Self::RunId => "run_id".to_owned(),
            Self::Attribute(name) => format!("attribute.{name}"),
            Self::StateMeta(state, name) => format!("state_meta.{state}.{name}"),
            builtin => builtin.record_key().unwrap_or_default().to_owned(),
        }
    }
}
