//! What a Flow read asks to include besides the record: the payload,
//! result and error bytes up to a size, and which named values.

/// Elixir's default `payload_max_bytes`: 64 KiB.
pub const DEFAULT_PAYLOAD_MAX_BYTES: u64 = 65_536;

/// Which named values a read includes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Values {
    /// None.
    #[default]
    None,
    /// These names, where the Flow has them.
    Names(Vec<Vec<u8>>),
    /// Every named value.
    All,
}

/// What a read asks to include besides the record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Include {
    /// The payload, result and error bytes with their sizes; a value above
    /// `max_bytes` is reported as omitted with its size.
    pub payload: bool,
    /// Largest value returned inline.
    pub max_bytes: u64,
    /// Named values, as `values`.
    pub values: Values,
}

impl Default for Include {
    fn default() -> Self {
        Self {
            payload: false,
            max_bytes: DEFAULT_PAYLOAD_MAX_BYTES,
            values: Values::None,
        }
    }
}
