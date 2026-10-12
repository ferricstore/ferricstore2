//! Immutable apply budgets and admission over actual borrowed bytes.

use std::fmt;

use wf_types::bounds::MAX_API_KEY_BYTES;

use super::{CommittedCommand, Operation, WorkflowRecord};

/// Hard source-compatible configured inline-value ceiling.
pub const MAX_INLINE_BYTES: usize = 512 * 1_024 * 1_024;
/// Hard source-compatible configured atomic batch ceiling.
pub const MAX_BATCH_ITEMS: usize = 100_000;
/// Source maximum for the complete actual encoded isolation scope.
pub const MAX_ENCODED_ISOLATION_SCOPE_BYTES: usize = 8_192;
/// Current typed single-record schema has at most payload plus result/error.
pub const MAX_ENCODED_VALUES_PER_ITEM: usize = 2;
/// Stable per-item plan/history/control work charge, independent of native layout.
pub const PLAN_FIXED_CHARGE: usize = 1_024;
/// Conservative copied variable-byte multiplier across records/history/retry.
pub const PLAN_VARIABLE_MULTIPLIER: usize = 4;

/// Replicated immutable budgets; every replica must use identical values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdmissionLimits {
    inline_bytes: usize,
    decision_bytes: usize,
    batch_items: usize,
}

impl Default for AdmissionLimits {
    fn default() -> Self {
        Self {
            inline_bytes: 1_048_576,
            decision_bytes: 16 * 1_024 * 1_024,
            batch_items: 1_000,
        }
    }
}

impl AdmissionLimits {
    /// Validates source-compatible scalar caps and a separate aggregate budget.
    ///
    /// # Errors
    /// Rejects zero or a scalar value above its source immutable hard ceiling.
    /// Larger aggregate profiles require outer workload/resource approval.
    pub const fn new(
        inline_bytes: usize,
        decision_bytes: usize,
        batch_items: usize,
    ) -> Result<Self, AdmissionError> {
        if inline_bytes == 0
            || inline_bytes > MAX_INLINE_BYTES
            || decision_bytes == 0
            || batch_items == 0
            || batch_items > MAX_BATCH_ITEMS
        {
            Err(AdmissionError::InvalidLimits)
        } else {
            Ok(Self {
                inline_bytes,
                decision_bytes,
                batch_items,
            })
        }
    }

    /// Reads the maximum bytes of each present inline/encoded value.
    #[must_use]
    pub const fn inline_bytes(self) -> usize {
        self.inline_bytes
    }
    /// Reads the maximum actual encoded entry and aggregate decision-input bytes.
    #[must_use]
    pub const fn decision_bytes(self) -> usize {
        self.decision_bytes
    }
    /// Reads the combined maximum touched-record count.
    #[must_use]
    pub const fn batch_items(self) -> usize {
        self.batch_items
    }

    /// Computes stable input/output/work charge before producing any owned plan.
    ///
    /// Four copies conservatively cover represented final/intermediate record,
    /// history and retry strings; 1024 per item covers bounded scalar/ID event
    /// fields and container work. This is a deterministic resource estimate,
    /// not native allocation/RSS or a durable response encoding certificate.
    /// Actual response bytes still require outer encoded-output admission.
    ///
    /// # Errors
    /// Rejects checked multiplication/addition overflow or aggregate excess.
    pub fn charge(self, variable_bytes: usize, items: usize) -> Result<usize, AdmissionError> {
        let bytes = variable_bytes
            .checked_mul(PLAN_VARIABLE_MULTIPLIER)
            .and_then(|variable| {
                items
                    .checked_mul(PLAN_FIXED_CHARGE)
                    .and_then(|fixed| variable.checked_add(fixed))
            })
            .ok_or(AdmissionError::LengthOverflow)?;
        self.check_total(bytes)?;
        Ok(bytes)
    }

    pub(super) fn check_total(self, bytes: usize) -> Result<(), AdmissionError> {
        if bytes > self.decision_bytes {
            Err(AdmissionError::DecisionBytes)
        } else {
            Ok(())
        }
    }

    pub(super) fn check_value(self, bytes: &[u8]) -> Result<(), AdmissionError> {
        if bytes.len() > self.inline_bytes {
            Err(AdmissionError::ValueTooLarge {
                bytes: bytes.len(),
                maximum: self.inline_bytes,
            })
        } else {
            Ok(())
        }
    }
}

/// Deterministic malformed/oversize command rejection, independent of allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionError {
    /// The replicated budget itself is malformed.
    InvalidLimits,
    /// Empty or excessive atomic item count.
    BatchItems,
    /// An actual encoded key exceeds the source composed-key maximum.
    KeyTooLarge,
    /// One actual present value exceeds the configured source limit.
    ValueTooLarge {
        /// Actual byte length.
        bytes: usize,
        /// Replicated configured maximum.
        maximum: usize,
    },
    /// The encoded entry or summed decision inputs exceed the aggregate budget.
    DecisionBytes,
    /// Adding actual byte lengths cannot fit in the host representation.
    LengthOverflow,
    /// Encoded values exceed the represented finite single-record field schema.
    EncodedValueCount,
}

impl fmt::Display for AdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ValueTooLarge { bytes, maximum } => formatter.write_str(&format!(
                "ERR value too large ({bytes} bytes, max {maximum} bytes)"
            )),
            Self::KeyTooLarge => formatter.write_str(&format!(
                "ERR key too large (max {MAX_API_KEY_BYTES} bytes)"
            )),
            Self::InvalidLimits => {
                formatter.write_str("invalid immutable workflow admission limits")
            }
            Self::BatchItems => {
                formatter.write_str("ERR flow batch item count outside configured bounds")
            }
            Self::DecisionBytes => formatter.write_str("workflow decision byte budget exceeded"),
            Self::LengthOverflow => formatter.write_str("workflow byte length overflow"),
            Self::EncodedValueCount => {
                formatter.write_str("encoded workflow value count exceeds supported schema")
            }
        }
    }
}

impl std::error::Error for AdmissionError {}

/// Admits actual immutable command/key/value encodings without cloning them.
///
/// The outer committed decoder must verify these encodings correspond to this
/// command and its authoritative partition; a client-supplied size/digest/flag
/// is never an encoding certificate. This function reads actual slice lengths.
/// Raw typed values are checked as well, so a shorter supplied encoding cannot
/// hide a large command field. Public legacy `reduce` remains a separate API.
///
/// # Errors
/// Rejects an oversized composed key, inline/encoded value or summed byte count.
pub fn admit_command(
    command: &CommittedCommand,
    state_key: &[u8],
    encoded_values: &[&[u8]],
    limits: AdmissionLimits,
) -> Result<usize, AdmissionError> {
    if encoded_values.len() > MAX_ENCODED_VALUES_PER_ITEM {
        return Err(AdmissionError::EncodedValueCount);
    }
    if state_key.len() > MAX_API_KEY_BYTES {
        return Err(AdmissionError::KeyTooLarge);
    }
    let mut bytes = command_bytes(command, limits)?;
    add(&mut bytes, state_key.len())?;
    for value in encoded_values {
        limits.check_value(value)?;
        add(&mut bytes, value.len())?;
    }
    limits.check_total(bytes)?;
    Ok(bytes)
}

fn command_bytes(
    command: &CommittedCommand,
    limits: AdmissionLimits,
) -> Result<usize, AdmissionError> {
    let mut total = command.id.as_bytes().len();
    let (names, values): (&[&[u8]], &[Option<&[u8]>]) = match &command.operation {
        Operation::Create(create) => (
            &[&create.workflow_type, &create.state],
            &[create.payload.as_deref()],
        ),
        Operation::Claim(claim) | Operation::Reclaim(claim) => (
            &[&claim.expected_state, &claim.worker, claim.token.as_bytes()],
            &[],
        ),
        Operation::Transition(transition) => (
            &[
                &transition.from,
                &transition.to,
                transition
                    .lease_token
                    .as_ref()
                    .map_or(&[], crate::lease::LeaseToken::as_bytes),
            ],
            &[transition.payload.as_deref()],
        ),
        Operation::Complete(terminal) | Operation::Fail(terminal) => (
            &[terminal.guard.token.as_bytes()],
            &[terminal.payload.as_deref(), terminal.value.as_deref()],
        ),
        Operation::Retry(retry) => (
            &[retry.guard.token.as_bytes()],
            &[retry.payload.as_deref(), retry.error.as_deref()],
        ),
        Operation::RetryWithPolicy(retry) => (
            &[
                retry.guard.token.as_bytes(),
                retry.policy_override.exhausted_to.as_deref().unwrap_or(&[]),
            ],
            &[retry.payload.as_deref(), retry.error.as_deref()],
        ),
        Operation::ExtendLease(extend) => (&[extend.guard.token.as_bytes()], &[]),
        Operation::Cancel(cancel) => (
            &[
                cancel
                    .lease_token
                    .as_ref()
                    .map_or(&[], crate::lease::LeaseToken::as_bytes),
                cancel.expected_state.as_deref().unwrap_or(&[]),
            ],
            &[cancel.error.as_deref()],
        ),
    };
    for name in names {
        limits.check_value(name)?;
        add(&mut total, name.len())?;
    }
    for value in values.iter().flatten() {
        limits.check_value(value)?;
        add(&mut total, value.len())?;
    }
    Ok(total)
}

pub(crate) fn record_bytes(
    record: &WorkflowRecord,
    limits: AdmissionLimits,
) -> Result<usize, AdmissionError> {
    let mut total = record.id().as_bytes().len();
    let values = [
        Some(record.workflow_type()),
        Some(record.state()),
        record.run_state(),
        record.payload(),
        record.result(),
        record.error(),
        record.lease().map(|lease| lease.worker.as_slice()),
        record.lease().map(|lease| lease.token.as_bytes()),
    ];
    for value in values.iter().flatten() {
        limits.check_value(value)?;
        add(&mut total, value.len())?;
    }
    limits.check_total(total)?;
    Ok(total)
}

pub(super) fn add(total: &mut usize, bytes: usize) -> Result<(), AdmissionError> {
    *total = total
        .checked_add(bytes)
        .ok_or(AdmissionError::LengthOverflow)?;
    Ok(())
}
