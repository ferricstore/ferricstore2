//! Source return flags against an explicitly selected public/internal maximum.
use super::{OptionsError, Raw};
use std::fmt;

/// Normalized semantic selection; it confers no internal or read authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OriginalPayloadReturn {
    enabled: bool,
    maximum: u64,
}
impl OriginalPayloadReturn {
    /// Explicit payload override after legacy full and caller defaults.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }
    /// Inclusive complete stored-byte maximum, after source validation.
    #[must_use]
    pub const fn maximum_bytes(self) -> u64 {
        self.maximum
    }
    /// Existing regular-value projection semantics, without decoding another shape.
    #[must_use]
    pub fn value_return(self) -> crate::history::values::HistoryValueReturn {
        if self.enabled {
            crate::history::values::HistoryValueReturn::Enabled {
                max_bytes: i128::from(self.maximum),
            }
        } else {
            crate::history::values::HistoryValueReturn::Disabled
        }
    }
}
/// Source option diagnosis or an unsupported selected configuration capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalPayloadReturnError {
    /// Existing exact option key/class/ceiling diagnosis.
    Option(OptionsError<'static>),
    /// Bounded caller value exceeds the actual selected configured maximum.
    Maximum {
        /// Exact selected maximum.
        maximum: u64,
    },
    /// Caller did not supply the resolved nonnegative integer configuration.
    Configuration,
}
impl fmt::Display for OriginalPayloadReturnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Option(e) => fmt::Display::fmt(e, f),
            Self::Maximum { maximum } => {
                write!(f, "ERR flow payload_max_bytes exceeds maximum {maximum}")
            }
            Self::Configuration => f.write_str("unsupported payload return configuration capture"),
        }
    }
}
impl std::error::Error for OriginalPayloadReturnError {}

/// Normalize first-occurrence full, payload and max in original error order.
/// `configured` is the actual selected public/internal maximum; environment
/// fallback and internal capability acquisition stay with the trusted caller.
/// # Errors
/// Preserve option classes/ceiling, selected maximum and unsupported captures.
pub fn normalize_original_payload_return(
    options: &[(&str, Raw<'_>)],
    default_enabled: bool,
    configured: Raw<'_>,
) -> Result<OriginalPayloadReturn, OriginalPayloadReturnError> {
    let full = super::optional_boolean(options, "full", boolean(default_enabled))
        .map_err(OriginalPayloadReturnError::Option)?;
    let enabled = super::optional_boolean(options, "payload", boolean(full))
        .map_err(OriginalPayloadReturnError::Option)?;
    Ok(OriginalPayloadReturn {
        enabled,
        maximum: maximum(options, configured)?,
    })
}
/// Normalize original history values/max flags, independent of full/payload.
/// # Errors
/// Preserve the same selected-configuration and source option diagnoses.
pub fn normalize_original_history_payload_return(
    options: &[(&str, Raw<'_>)],
    configured: Raw<'_>,
) -> Result<OriginalPayloadReturn, OriginalPayloadReturnError> {
    let enabled = super::optional_boolean(options, "values", Raw::Atom("false"))
        .map_err(OriginalPayloadReturnError::Option)?;
    Ok(OriginalPayloadReturn {
        enabled,
        maximum: maximum(options, configured)?,
    })
}
fn boolean(value: bool) -> Raw<'static> {
    if value {
        Raw::Atom("true")
    } else {
        Raw::Atom("false")
    }
}
fn maximum(
    options: &[(&str, Raw<'_>)],
    configured: Raw<'_>,
) -> Result<u64, OriginalPayloadReturnError> {
    use OriginalPayloadReturnError as E;
    let requested = super::optional_non_neg_integer(options, "payload_max_bytes", configured)
        .map_err(E::Option)?
        .ok_or(E::Configuration)?;
    match super::integer_value(configured) {
        Ok(maximum) if requested > maximum => Err(E::Maximum { maximum }),
        Ok(_) | Err(super::OptionsErrorKind::Ceiling) => Ok(requested),
        Err(_) => Err(E::Configuration),
    }
}
