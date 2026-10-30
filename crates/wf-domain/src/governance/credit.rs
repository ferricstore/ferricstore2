//! Consuming credit-owner transitions without clocks, storage or spend authority.

use crate::workflow::control::PreparedDigest;

pub(in crate::governance) mod admission;
pub(in crate::governance) mod capacity;
pub(in crate::governance) mod decision;
mod model;
mod reservation;
pub use model::{
    CreditDecision, CreditError, CreditLeaseData, CreditLimits, CreditOperation, CreditOwner,
    CreditOwnerData, CreditRejection, CreditReservationValue, CreditTransition,
};

/// Constructs one bounded mathematical owner, without installed-policy authority.
/// # Errors
/// Refuses invalid input, ownership/work limits or fallible reservation failure.
pub fn new_credit_owner(
    scope: Vec<u8>,
    limit: u64,
    config_version: u64,
    policy_fingerprint: Option<PreparedDigest>,
    limits: CreditLimits,
) -> Result<CreditOwner, CreditError> {
    restore_credit_owner(
        CreditOwnerData {
            scope,
            limit,
            free: limit,
            epoch: 0,
            config_version,
            policy_fingerprint,
            cleanup_head: 1,
            cleanup_tail: 0,
            leases: Vec::new(),
        },
        limits,
    )
}

/// Validates complete canonical data before sealing the moved owner.
/// # Errors
/// Refuses invalid scalars/order/capacity, resource limits or allocation failure.
pub fn restore_credit_owner(
    data: CreditOwnerData,
    limits: CreditLimits,
) -> Result<CreditOwner, CreditError> {
    admission::CreditCharge::inspect(&data, limits)?;
    Ok(CreditOwner { data })
}

/// Consumes one owner into a complete mathematical business classification.
/// Known denial may include source expiry or pending-reclaim changes.
/// # Errors
/// Operational authority/input/allocation/work failure produces no transition.
pub fn decide_credit(
    mut owner: CreditOwner,
    operation: CreditOperation<'_>,
    limits: CreditLimits,
) -> Result<CreditTransition, CreditError> {
    let mut charge = admission::CreditCharge::inspect(&owner.data, limits)?;
    let decision = decision::apply(&mut owner.data, operation, &mut charge)?;
    Ok(CreditTransition { owner, decision })
}
