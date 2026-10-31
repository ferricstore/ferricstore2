//! Public borrowed caller operands, with private validated capture ownership.
use std::fmt;

/// Public operation selector, separate from private prepared mutations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCallKind {
    /// Grant partial or full capacity from the owner.
    Lease,
    /// Generate and spend one identified reservation per credit.
    Spend,
    /// Release the exact original identified reservations.
    Release,
    /// Extend the selected current lease.
    Renew,
    /// Observe the owner after committed expiry processing.
    Get,
    /// Process one bounded detached reservation page.
    Cleanup,
}
/// One borrowed ID cohort with no payload clone or execution authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditIdList<'a> {
    /// Original caller-owned payload buffers.
    Owned(&'a [Vec<u8>]),
    /// Original wire fields after bounded borrowed inspection.
    Borrowed(&'a [&'a [u8]]),
}
impl<'a> CreditIdList<'a> {
    /// Number of original ordered IDs.
    #[must_use]
    pub const fn len(self) -> usize {
        match self {
            Self::Owned(v) => v.len(),
            Self::Borrowed(v) => v.len(),
        }
    }
    /// Whether the original cohort has no IDs.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }
    /// Visits only the original bounded ID descriptors, preserving order.
    pub fn iter(self) -> impl ExactSizeIterator<Item = &'a [u8]> {
        (0..self.len()).map(move |i| match self {
            Self::Owned(v) => v[i].as_slice(),
            Self::Borrowed(v) => v[i],
        })
    }
}
/// Raw supported keyword value; Nil and absence remain distinct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditOptionValue<'a> {
    /// Explicit source nil, distinct from an absent keyword.
    Nil,
    /// Exact captured signed integer before finite-range validation.
    Integer(i128),
    /// Original opaque bytes, without UTF-8 conversion.
    Bytes(&'a [u8]),
    /// Ordered original reservation identifiers.
    Ids(CreditIdList<'a>),
    /// Captured value outside the selected scalar grammar.
    Unsupported,
}
/// Raw policy version, before the source-compatible fingerprint is computed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditPolicyInput<'a> {
    /// Nonnegative finite numeric source policy version.
    Number(u64),
    /// Original opaque bytes, without UTF-8 conversion.
    Bytes(&'a [u8]),
}
/// Complete optional public reconfiguration; no current-policy certificate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditCallConfiguration<'a> {
    /// Nonnegative requested capacity ceiling.
    pub limit: u64,
    /// Explicit generation or the numeric-policy fallback.
    pub config_version: Option<u64>,
    /// Original raw policy version before fingerprinting.
    pub policy_version: Option<CreditPolicyInput<'a>>,
}
/// Public normalized operands; generated spend IDs are deliberately absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCallAction<'a> {
    /// Capacity grant with optional complete configuration.
    Lease {
        /// Original selected owner shard.
        shard_id: u64,
        /// Positive credit count, at most1000.
        amount: u64,
        /// Positive requested lease duration.
        ttl_ms: u64,
        /// Optional complete capacity configuration.
        configuration: Option<CreditCallConfiguration<'a>>,
    },
    /// Public spend; preparation supplies epoch and fresh IDs.
    Spend {
        /// Original selected owner shard.
        shard_id: u64,
        /// Positive credit count, at most1000.
        amount: u64,
        /// Optional positive lease extension.
        ttl_ms: Option<u64>,
        /// Optional complete capacity configuration.
        configuration: Option<CreditCallConfiguration<'a>>,
    },
    /// Release only the supplied original ordered identifiers.
    Release {
        /// Original selected owner shard.
        shard_id: u64,
        /// Positive credit count, at most1000.
        amount: u64,
        /// Exact original release IDs, with no generated replacements.
        reservation_ids: CreditIdList<'a>,
    },
    /// Extend a current selected shard lease.
    Renew {
        /// Original selected owner shard.
        shard_id: u64,
        /// Positive requested lease duration.
        ttl_ms: u64,
    },
    /// Observe the owner after committed expiry processing.
    Get,
    /// Process one bounded detached reservation page.
    Cleanup,
}
impl CreditCallAction<'_> {
    /// Original public operation without trusting a private command tag.
    #[must_use]
    pub const fn kind(self) -> CreditCallKind {
        match self {
            Self::Lease { .. } => CreditCallKind::Lease,
            Self::Spend { .. } => CreditCallKind::Spend,
            Self::Release { .. } => CreditCallKind::Release,
            Self::Renew { .. } => CreditCallKind::Renew,
            Self::Get => CreditCallKind::Get,
            Self::Cleanup => CreditCallKind::Cleanup,
        }
    }
}
/// Sealed borrowed caller fields, without clock, placement or generated data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalizedCreditCall<'a> {
    pub(super) scope: &'a [u8],
    pub(super) shard_count: u64,
    pub(super) now_ms: Option<u64>,
    pub(super) action: CreditCallAction<'a>,
}
impl<'a> NormalizedCreditCall<'a> {
    /// Restores typed caller fields only after complete validation.
    /// # Errors
    /// Refuses invalid fields, IDs, configuration or deadlines.
    pub fn from_parts(
        scope: &'a [u8],
        count: u64,
        now: Option<u64>,
        action: CreditCallAction<'a>,
    ) -> Result<Self, CreditCallerError> {
        super::validate::parts(scope, count, now, action)?;
        Ok(Self {
            scope,
            shard_count: count,
            now_ms: now,
            action,
        })
    }
    /// Original opaque scope bytes.
    #[must_use]
    pub const fn scope(self) -> &'a [u8] {
        self.scope
    }
    /// Configured count, rechecked by the execution owner.
    #[must_use]
    pub const fn shard_count(self) -> u64 {
        self.shard_count
    }
    /// Explicit caller time; None requires a trusted prepared clock.
    #[must_use]
    pub const fn now_ms(self) -> Option<u64> {
        self.now_ms
    }
    /// Original normalized operation operands.
    #[must_use]
    pub const fn action(self) -> CreditCallAction<'a> {
        self.action
    }
}
/// Public input/resource refusal, separate from committed business outcomes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CreditCallerError {
    /// Unsupported option shape, scope or configured count.
    InvalidOptions,
    /// Named finite integer or optionality validation failure.
    InvalidField(&'static str),
    /// Selected shard is outside the configured finite count.
    Shard {
        /// Greatest allowed shard identity.
        maximum: u64,
    },
    /// Mutation amount is outside1..1000.
    Amount,
    /// Caller time plus duration exceeds the exact integer range.
    Deadline,
    /// Public spends cannot supply their own IDs.
    ReservationIdsNotAllowed,
    /// Release IDs have invalid shape, uniqueness or cardinality.
    ReservationIds,
    /// Configuration generations require a capacity ceiling.
    LimitRequired,
    /// Policy version has invalid type or size.
    PolicyVersion,
    /// Binary policy versions require an explicit generation.
    PolicyVersionRequiresConfig,
    /// Caller raw admission ceiling is zero.
    InvalidLimits,
    /// Fallible bounded reference-owner reservation failed.
    Allocation,
    /// Bounded raw input or metadata admission refused.
    Limit,
}
impl fmt::Display for CreditCallerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions => f.write_str("ERR flow limit opts must be a keyword list"),
            Self::InvalidField("ttl_ms") => {
                f.write_str("ERR flow limit ttl_ms must be a positive integer")
            }
            Self::InvalidField(name) => {
                write!(f, "ERR flow limit {name} must be a non-negative integer")
            }
            Self::Shard { maximum } => {
                write!(f, "ERR flow limit shard_id must be between 0 and {maximum}")
            }
            Self::Amount => {
                f.write_str("ERR flow limit amount must be a positive integer at most 1000")
            }
            Self::Deadline => {
                f.write_str("ERR flow limit lease deadline exceeds the supported integer range")
            }
            Self::ReservationIdsNotAllowed => {
                f.write_str("ERR flow limit reservation_ids cannot be supplied for spend")
            }
            Self::ReservationIds => {
                f.write_str("ERR flow limit reservation_ids must contain one unique id per credit")
            }
            Self::LimitRequired => {
                f.write_str("ERR flow limit limit is required with configuration version")
            }
            Self::PolicyVersion => f.write_str("ERR flow limit policy_version is invalid"),
            Self::PolicyVersionRequiresConfig => {
                f.write_str("ERR flow limit config_version is required with binary policy_version")
            }
            Self::InvalidLimits => f.write_str("invalid credit caller limits"),
            Self::Allocation => f.write_str("credit caller allocation failed"),
            Self::Limit => f.write_str("credit caller resource limit"),
        }
    }
}
impl std::error::Error for CreditCallerError {}
