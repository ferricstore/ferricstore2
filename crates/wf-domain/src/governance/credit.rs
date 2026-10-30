//! Consuming credit-owner transitions without clocks, storage or spend authority.

use crate::workflow::control::PreparedDigest;

mod model;
pub use model::{
    CreditDecision, CreditError, CreditLeaseData, CreditLimits, CreditOperation, CreditOwner,
    CreditOwnerData, CreditRejection, CreditReservationValue, CreditTransition,
};

/// Constructs one bounded mathematical owner, without installed-policy authority.
/// # Errors
/// Refuses invalid input, ownership/work limits or fallible reservation failure.
pub fn new_credit_owner(
    _scope: Vec<u8>,
    _limit: u64,
    _config_version: u64,
    _policy_fingerprint: Option<PreparedDigest>,
    _limits: CreditLimits,
) -> Result<CreditOwner, CreditError> {
    Err(CreditError::InvalidState)
}

/// Validates complete canonical data before sealing the moved owner.
/// # Errors
/// Refuses invalid scalars/order/capacity, resource limits or allocation failure.
pub fn restore_credit_owner(
    _data: CreditOwnerData,
    _limits: CreditLimits,
) -> Result<CreditOwner, CreditError> {
    Err(CreditError::InvalidState)
}

/// Consumes one owner into a complete mathematical business classification.
/// Known denial may include source expiry or pending-reclaim changes.
/// # Errors
/// Operational authority/input/allocation/work failure produces no transition.
pub fn decide_credit(
    _owner: CreditOwner,
    _operation: CreditOperation<'_>,
    _limits: CreditLimits,
) -> Result<CreditTransition, CreditError> {
    Err(CreditError::InvalidState)
}
