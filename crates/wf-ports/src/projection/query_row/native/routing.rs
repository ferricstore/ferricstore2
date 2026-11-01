//! Explicit captured routing must match the actual native physical partition.
use super::super::budget::{Budget, add};
use super::{NativeQueryPreparationError as E, map_query};
use std::ops::Range;
use wf_domain::{
    metadata::record::{OriginalRecordMetadata as M, OriginalRecordMetadataField as F},
    metadata::scope::{original_logical_partition, original_physical_partition},
    value::OriginalValueNode as N,
};
use wf_types::identity::PartitionId;

pub(super) fn validate(
    m: &M,
    physical: &PartitionId,
    b: &mut Budget,
) -> Result<(Range<usize>, Option<Range<usize>>), E> {
    let N::Binary { bytes: partition } = m.field(F::Partition).ok_or(E::UnknownRouting)? else {
        return Err(E::RoutingMismatch);
    };
    let partition = partition.clone();
    let logical = m
        .document()
        .byte_range(partition.clone())
        .ok_or(E::RoutingMismatch)?;
    let scope = match m.field(F::ScopePrefix).ok_or(E::UnknownRouting)? {
        N::Binary { bytes } => Some(bytes.clone()),
        N::Atom { bytes, .. } if m.document().byte_range(bytes.clone()) == Some(b"nil") => None,
        _ => return Err(E::RoutingMismatch),
    };
    let isolation = scope
        .clone()
        .map(|r| m.document().byte_range(r).ok_or(E::RoutingMismatch))
        .transpose()?;
    if logical.is_empty() || isolation.is_some_and(|s| s.is_empty() || s.len() > 8192) {
        return Err(E::RoutingMismatch);
    }
    let result =
        original_physical_partition(logical, isolation, b.peer_limits().map_err(map_query)?)
            .map_err(scope_error)?;
    b.peer(result.work_units, result.temporary_bytes)
        .map_err(map_query)?;
    b.spend(add(result.value.len(), physical.as_bytes().len()).map_err(map_query)?)
        .map_err(map_query)?;
    if result.value.as_ref() != physical.as_bytes() {
        return Err(E::RoutingMismatch);
    }
    if let Some(prefix) = isolation {
        let result = original_logical_partition(
            physical.as_bytes(),
            prefix,
            b.peer_limits().map_err(map_query)?,
        )
        .map_err(scope_error)?;
        b.peer(result.work_units, result.temporary_bytes)
            .map_err(map_query)?;
        b.spend(add(result.value.len(), logical.len()).map_err(map_query)?)
            .map_err(map_query)?;
        if result.value != logical {
            return Err(E::RoutingMismatch);
        }
    }
    Ok((partition, scope))
}
fn scope_error(error: wf_domain::metadata::scope::OriginalScopeError) -> E {
    use wf_domain::metadata::scope::OriginalScopeError as S;
    match error {
        S::InvalidSystemMetadata => E::RoutingMismatch,
        other => map_query(super::super::budget::scope(other)),
    }
}
