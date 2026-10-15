//! Retry-only policy values borrowed from coherent authoritative state.

use std::collections::BTreeMap;

use wf_types::version::TimestampMillis;

use crate::retry::JitterSample;
use crate::value::{DomainError, InputField, RetryPolicyField};

pub mod complete;
pub mod definition;
mod install_validation;
pub mod installation;
mod patch;
mod preview;
pub mod restoration;

const MAX_DELAY_MILLIS: u64 = 2_592_000_000;

/// Supported source backoff kinds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackoffKind {
    /// Retry without delay.
    None,
    /// Constant base delay before jitter and the final cap.
    Fixed,
    /// Base delay multiplied by the new attempt count.
    Linear,
    /// Base delay doubled for each attempt after the first.
    Exponential,
}

impl BackoffKind {
    /// Returns the source history label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Fixed => "fixed",
            Self::Linear => "linear",
            Self::Exponential => "exponential",
        }
    }
}

/// Fully normalized backoff; omitted input fields use source defaults.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Backoff {
    /// Delay growth rule.
    pub kind: BackoffKind,
    /// Base delay in milliseconds, including zero.
    pub base_ms: u64,
    /// Final delay cap in milliseconds, including zero.
    pub max_ms: u64,
    /// Symmetric jitter percentage, including zero.
    pub jitter_pct: u8,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            kind: BackoffKind::Exponential,
            base_ms: 1_000,
            max_ms: 30_000,
            jitter_pct: 20,
        }
    }
}

impl Backoff {
    fn validate(self) -> Result<(), DomainError> {
        if self.base_ms > MAX_DELAY_MILLIS {
            return Err(DomainError::InvalidRetryPolicy(
                RetryPolicyField::BaseMillis,
            ));
        }
        if self.max_ms > MAX_DELAY_MILLIS {
            return Err(DomainError::InvalidRetryPolicy(RetryPolicyField::MaxMillis));
        }
        if self.jitter_pct > 100 {
            return Err(DomainError::InvalidRetryPolicy(
                RetryPolicyField::JitterPercent,
            ));
        }
        Ok(())
    }

    fn delay(self, attempt: u64, jitter: JitterSample) -> u64 {
        let delay = match self.kind {
            BackoffKind::None => 0,
            BackoffKind::Fixed => self.base_ms,
            BackoffKind::Linear => self.base_ms.saturating_mul(attempt).min(self.max_ms),
            BackoffKind::Exponential => {
                let steps = u32::try_from(attempt.saturating_sub(1)).unwrap_or(u32::MAX);
                self.base_ms
                    .saturating_mul(2_u64.saturating_pow(steps))
                    .min(self.max_ms)
            }
        };
        let spread = delay * u64::from(self.jitter_pct) / 100;
        let jittered = if spread == 0 {
            delay
        } else {
            let slot = u64::from(jitter.get()) % (spread * 2 + 1);
            if slot >= spread {
                delay + (slot - spread)
            } else {
                delay.saturating_sub(spread - slot)
            }
        };
        jittered.min(self.max_ms)
    }
}

/// Normalized retry override; absent fields inherit from lower policy layers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RetryOverride {
    /// Optional inclusive number of permitted retries.
    pub max_retries: Option<u16>,
    /// A supplied backoff is fully normalized before layer resolution.
    pub backoff: Option<Backoff>,
    /// Optional terminal or active target after retries are exhausted.
    pub exhausted_to: Option<Vec<u8>>,
}

impl RetryOverride {
    /// Revalidates normalized input with the source field-specific errors.
    ///
    /// # Errors
    /// Rejects an out-of-range value, empty target or `running` target.
    pub fn validate(&self) -> Result<(), DomainError> {
        validate_retry(self.max_retries, self.backoff, self.exhausted_to.as_deref())
    }
}

/// Validates borrowed retry fields without constructing or copying an override.
///
/// # Errors
/// Rejects the same out-of-range fields and invalid exhaustion targets as input.
pub fn validate_retry(
    max_retries: Option<u16>,
    backoff: Option<Backoff>,
    exhausted_to: Option<&[u8]>,
) -> Result<(), DomainError> {
    if max_retries.is_some_and(|limit| limit > 1_000) {
        return Err(DomainError::InvalidRetryPolicy(
            RetryPolicyField::MaxRetries,
        ));
    }
    if let Some(backoff) = backoff {
        backoff.validate()?;
    }
    match exhausted_to {
        Some([]) => Err(DomainError::InvalidRetryPolicy(
            RetryPolicyField::ExhaustedTo,
        )),
        Some(b"running") => Err(DomainError::InvalidRetryPolicy(
            RetryPolicyField::RunningExhaustion,
        )),
        _ => Ok(()),
    }
}

/// Resolved source retry policy; public values are validated before use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryPolicy {
    /// Inclusive retry count; claim itself does not increment attempts.
    pub max_retries: u16,
    /// Resolved timing rule.
    pub backoff: Backoff,
    /// State entered once the new attempts count exceeds the retry limit.
    pub exhausted_to: Vec<u8>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            backoff: Backoff::default(),
            exhausted_to: b"failed".to_vec(),
        }
    }
}

impl RetryPolicy {
    /// Checks the source's inclusive retry count.
    #[must_use]
    pub fn attempt_allowed(&self, attempts: u64) -> bool {
        attempts <= u64::from(self.max_retries)
    }

    /// Computes a deadline from explicit committed time and a jitter sample.
    ///
    /// # Errors
    /// Returns a policy rejection or checked timestamp overflow.
    pub fn next_run_at(
        &self,
        now: TimestampMillis,
        attempt: u64,
        jitter: JitterSample,
    ) -> Result<TimestampMillis, DomainError> {
        validate_retry(
            Some(self.max_retries),
            Some(self.backoff),
            Some(&self.exhausted_to),
        )?;
        now.checked_add(self.backoff.delay(attempt, jitter))
            .ok_or(DomainError::RetryDeadlineOverflow)
    }
}

/// Validated retry-only view of an installed type/state policy.
///
/// This value performs no installation or persistence and omits other policy
/// concerns. The future apply adapter supplies the coherent authoritative view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstalledRetryPolicy {
    workflow_type: Vec<u8>,
    retry: RetryOverride,
    states: BTreeMap<Vec<u8>, RetryOverride>,
}

impl InstalledRetryPolicy {
    /// Validates a retry-only authoritative view.
    ///
    /// # Errors
    /// Returns source type/state-name or retry-policy input rejection.
    pub fn new(
        workflow_type: Vec<u8>,
        retry: RetryOverride,
        states: BTreeMap<Vec<u8>, RetryOverride>,
    ) -> Result<Self, DomainError> {
        if workflow_type.is_empty() {
            return Err(DomainError::EmptyValue(InputField::Type));
        }
        retry.validate()?;
        for (state, state_retry) in &states {
            if state.is_empty() {
                return Err(DomainError::InvalidRetryPolicy(RetryPolicyField::State));
            }
            if state == b"running" {
                return Err(DomainError::InvalidRetryPolicy(
                    RetryPolicyField::RunningState,
                ));
            }
            state_retry.validate()?;
        }
        Ok(Self {
            workflow_type,
            retry,
            states,
        })
    }

    /// Borrows the type to which this installed view belongs.
    #[must_use]
    pub fn workflow_type(&self) -> &[u8] {
        &self.workflow_type
    }

    /// Borrows the installed type override without cloning unrelated state policies.
    #[must_use]
    pub const fn type_retry(&self) -> &RetryOverride {
        &self.retry
    }

    /// Borrows only the relevant logical-state override for bounded admission.
    #[must_use]
    pub fn state_retry(&self, state: &[u8]) -> Option<&RetryOverride> {
        self.states.get(state)
    }
}

/// Resolves defaults, type, logical-state and command override in that order.
///
/// # Errors
/// Rejects malformed typed override values or an unrelated authoritative policy.
pub fn resolve(
    installed: Option<&InstalledRetryPolicy>,
    workflow_type: &[u8],
    run_state: &[u8],
    command_override: &RetryOverride,
) -> Result<RetryPolicy, DomainError> {
    command_override.validate()?;
    let mut policy = RetryPolicy::default();
    if let Some(installed) = installed {
        if installed.workflow_type != workflow_type {
            return Err(DomainError::PolicyTypeMismatch);
        }
        merge(&mut policy, &installed.retry);
        if let Some(state_retry) = installed.states.get(run_state) {
            merge(&mut policy, state_retry);
        }
    }
    merge(&mut policy, command_override);
    Ok(policy)
}

fn merge(policy: &mut RetryPolicy, override_: &RetryOverride) {
    if let Some(limit) = override_.max_retries {
        policy.max_retries = limit;
    }
    if let Some(backoff) = override_.backoff {
        policy.backoff = backoff;
    }
    if let Some(target) = &override_.exhausted_to {
        policy.exhausted_to.clone_from(target);
    }
}
