//! Actual opaque refs borrow; native versioned refs render with the existing codec.
use super::StrongWorkflowRecordValueError as E;
use super::budget::{Budget, add, product};
use std::borrow::Cow;
use wf_domain::workflow::active::{ActiveValueKind, ActiveValueRef};
use wf_ports::{
    blob::{OriginalValuePartition, OriginalValueRefKind, render_original_value_ref},
    projection::query_row::NativeQueryRecord,
};

pub(super) fn collect<'a>(
    record: &'a NativeQueryRecord,
    b: &mut Budget,
) -> Result<[Option<Cow<'a, [u8]>>; 3], E> {
    let refs = &record.value_binding().refs;
    let mut result = std::array::from_fn(|_| None);
    for (slot, source) in result.iter_mut().zip([
        refs.payload.as_ref(),
        refs.result.as_ref(),
        refs.error.as_ref(),
    ]) {
        *slot = match source {
            None => None,
            Some(ActiveValueRef::Opaque(bytes)) => {
                b.spend(add(128, bytes.len())?)?;
                Some(Cow::Borrowed(bytes.as_slice()))
            }
            Some(ActiveValueRef::Versioned {
                partition,
                id,
                kind,
                version,
            }) => {
                // The renderer has a49-byte fixed tag and at most20 version digits.
                let maximum = add(id.as_bytes().len(), 96)?.min(65_535);
                b.spend(add(
                    512,
                    product(
                        add(
                            add(id.as_bytes().len(), partition.as_bytes().len())?,
                            maximum,
                        )?,
                        16,
                    )?,
                )?)?;
                b.admit(maximum)?;
                let kind = match kind {
                    ActiveValueKind::Payload => OriginalValueRefKind::Payload,
                    ActiveValueKind::Result => OriginalValueRefKind::Result,
                    ActiveValueKind::Error => OriginalValueRefKind::Error,
                };
                Some(Cow::Owned(
                    render_original_value_ref(
                        id.as_bytes(),
                        kind,
                        *version,
                        OriginalValuePartition::Explicit(partition.as_bytes()),
                        maximum,
                    )
                    .map_err(E::Reference)?,
                ))
            }
        };
    }
    Ok(result)
}
