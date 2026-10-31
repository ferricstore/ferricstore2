//! Definite pre-submission failures preserve original contexts on retry.
use super::PreparedPublicClaimCall;
use std::fmt;
use wf_ports::{consensus::MutationNotSubmitted, log::CodecError};
/// No new proposal was made by this application preparation/admission failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicClaimCallError {
    /// Invalid server profiles or original scope/options.
    InvalidOptions,
    /// Prospective checked ownership/work admission refused.
    Limit,
    /// Fallible bounded metadata allocation failed.
    Allocation,
    /// Current original inquiry or trusted preparation refused.
    Preparation(MutationNotSubmitted),
    /// Complete canonical caller/frame/binding preparation failed.
    Codec(CodecError),
}
impl fmt::Display for PublicClaimCallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions => f.write_str("invalid public Claim call options"),
            Self::Limit => f.write_str("public Claim call resource limit"),
            Self::Allocation => f.write_str("public Claim call allocation failed"),
            Self::Preparation(e) => write!(f, "public Claim preparation refused: {e:?}"),
            Self::Codec(e) => fmt::Display::fmt(e, f),
        }
    }
}
impl std::error::Error for PublicClaimCallError {}
/// Retry admission failed without dropping the original immutable submission.
pub struct PublicClaimRetryError {
    pub(in crate::commands::claim) error: PublicClaimCallError,
    pub(in crate::commands::claim) original: Box<PreparedPublicClaimCall>,
}
impl PublicClaimRetryError {
    /// Current pre-submission refusal.
    #[must_use]
    pub const fn error(&self) -> PublicClaimCallError {
        self.error
    }
    /// Transfers the unchanged original context for later inquiry/retry.
    #[must_use]
    pub fn into_original(self) -> Box<PreparedPublicClaimCall> {
        self.original
    }
}
impl fmt::Debug for PublicClaimRetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl fmt::Display for PublicClaimRetryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}
impl std::error::Error for PublicClaimRetryError {}
