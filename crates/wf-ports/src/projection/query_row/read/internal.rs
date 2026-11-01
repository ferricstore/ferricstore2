//! Original internal-record overrides, without copying the complete record.
use super::super::{
    I,
    budget::{Budget, scope},
    model::OriginalQueryRecordError as E,
};
use super::model::{DecodedOriginalQueryRow, OriginalQueryInternalRecord, OriginalQueryScope};
use std::borrow::Cow;
pub(super) fn run<'r, 'a>(
    row: &'r DecodedOriginalQueryRow<'a>,
    b: &mut Budget,
) -> Result<OriginalQueryInternalRecord<'r, 'a>, E> {
    let record = row.record.as_ref().ok_or(E::InvalidRecord)?;
    let scope_prefix = match row.scope {
        OriginalQueryScope::NotRead => return Err(E::InvalidRecord),
        OriginalQueryScope::Unscoped => None,
        OriginalQueryScope::Scoped(bytes) => Some(bytes),
    };
    let logical = match record.field("partition_key") {
        Some(I::Nil) => None,
        Some(I::Binary(bytes)) => Some(bytes),
        _ => return Err(E::InvalidRecord),
    };
    let partition = match (logical, scope_prefix) {
        (Some(logical), Some(scope_prefix)) => {
            let value = wf_domain::metadata::scope::original_physical_partition(
                logical,
                Some(scope_prefix),
                b.peer_limits()?,
            )
            .map_err(scope)?;
            b.peer(value.work_units, value.temporary_bytes)?;
            b.output(value.value.len())?;
            if let Cow::Owned(bytes) = &value.value {
                b.admit(bytes.len())?;
            }
            Some(value.value)
        }
        (Some(logical), None) => {
            b.output(logical.len())?;
            Some(Cow::Borrowed(logical))
        }
        (None, None) => None,
        _ => return Err(E::InvalidRecord),
    };
    let terminal = matches!(
        record.field("state"),
        Some(I::Binary(b"completed" | b"failed" | b"cancelled"))
    );
    Ok(OriginalQueryInternalRecord {
        record,
        partition,
        retention: (terminal && row.expiry > 0).then_some(row.expiry),
    })
}
