//! Point keys bind complete typed identities without concatenation ambiguity.

use wf_domain::workflow::request::RequestScope;
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId};

use super::encode_components;
use crate::state::value::StateValueError;

/// Encodes the group-wide control key, independent of tenant identity.
///
/// # Errors
/// Rejects a zero cap, checked size overflow, excess bytes or allocation failure.
pub fn encode_control_key(
    group: &GroupId,
    recovery: &RecoveryId,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    encode_components(0, &[group.as_bytes(), recovery.as_bytes()], maximum_bytes)
}

/// Encodes one record identity within its complete tenant and recovery scope.
///
/// # Errors
/// Rejects a zero cap, checked size overflow, excess bytes or allocation failure.
pub fn encode_record_key(
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    encode_components(
        1,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            partition.as_bytes(),
            id.as_bytes(),
        ],
        maximum_bytes,
    )
}

/// Encodes a retained request outcome within its complete server-bound scope.
///
/// # Errors
/// Rejects a zero cap, checked size overflow, excess bytes or allocation failure.
pub fn encode_outcome_key(
    scope: &RequestScope,
    request: &RequestId,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    encode_components(
        2,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            request.as_bytes(),
        ],
        maximum_bytes,
    )
}

/// Encodes the exact retained cohort prefix for tenant, group and recovery.
///
/// The request component is absent; complete outcome keys extend this prefix.
/// # Errors
/// Rejects a zero cap, checked size overflow, excess bytes or allocation failure.
pub fn encode_outcome_prefix(
    scope: &RequestScope,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    encode_components(
        2,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
        ],
        maximum_bytes,
    )
}

/// Encodes an exact normalized-policy query, including an empty raw type.
///
/// Stored values still require nonempty schema-valid types. Query bytes have no
/// record-ID ceiling; the explicit encoded cap bounds their escaped expansion.
/// # Errors
/// Rejects a zero cap, checked size overflow, excess bytes or allocation failure.
pub fn encode_policy_key(
    scope: &RequestScope,
    workflow_type: &[u8],
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    encode_components(
        6,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            workflow_type,
        ],
        maximum_bytes,
    )
}

/// Encodes recovery-wide authoritative placement, independent of tenant/group.
///
/// The entire value is a nonempty typed group ID under this key version/family.
/// # Errors
/// Rejects a zero cap, checked size overflow, excess bytes or allocation failure.
pub fn encode_placement_key(
    recovery: &RecoveryId,
    partition: &PartitionId,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    encode_components(
        7,
        &[recovery.as_bytes(), partition.as_bytes()],
        maximum_bytes,
    )
}

/// Encodes the complete canonical tenant prefix, without inventing row membership.
/// # Errors
/// Refuses zero/excess caps, overflow or allocation failure.
pub fn encode_record_prefix(
    scope: &RequestScope,
    maximum_bytes: usize,
) -> Result<Vec<u8>, StateValueError> {
    encode_components(
        1,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
        ],
        maximum_bytes,
    )
}

/// Encodes one global policy/placement family prefix.
/// # Errors
/// Refuses other families, zero/excess caps or allocation failure.
pub fn encode_catalog_prefix(family: u8, maximum_bytes: usize) -> Result<Vec<u8>, StateValueError> {
    if !matches!(family, 6 | 7) {
        return Err(StateValueError::UnsupportedTag);
    }
    encode_components(family, &[], maximum_bytes)
}
