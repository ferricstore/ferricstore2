//! Prospective logical cohorts and requested owners, separate from native RSS.

use std::mem::size_of;

use crate::consensus::{
    PreparedBatchItem, PreparedWorkflowValueBatch, WorkflowValueItemInput,
    WorkflowValuePolicyWitness,
};
use crate::log::{CodecError, CodecLimits, DecodedRequest};

pub(super) const BASE_WORK: usize = 65_536;
pub(super) const ITEM_WORK: usize = 4_096;
pub(super) const POLICY_WORK: usize = 1_024;
pub(super) const BYTE_WORK: usize = 128;
pub(super) const KEY_WORK: usize = 64;

pub(super) struct ValueBatchCharge {
    pub(super) maximum: usize,
    pub(super) spent: usize,
}

impl ValueBatchCharge {
    pub(super) const fn new(maximum: usize) -> Self {
        Self { maximum, spent: 0 }
    }

    pub(super) fn debit(&mut self, amount: usize) -> Result<(), CodecError> {
        self.spent = self
            .spent
            .checked_add(amount)
            .ok_or(CodecError::LengthOverflow)?;
        if self.spent > self.maximum {
            Err(CodecError::WorkLimit)
        } else {
            Ok(())
        }
    }
}

pub(super) fn owned(
    encoded: usize,
    items: usize,
    policies: usize,
    keys: usize,
    generic: bool,
) -> Result<usize, CodecError> {
    let mut bytes = size_of::<PreparedWorkflowValueBatch>();
    if generic {
        bytes = bytes
            .checked_add(size_of::<DecodedRequest<'static>>())
            .ok_or(CodecError::LengthOverflow)?;
    }
    // Public N=1 accounts for its pointee instead of a temporary item Vec.
    for (count, width) in [
        (items, size_of::<PreparedBatchItem>()),
        (items, size_of::<WorkflowValueItemInput>()),
        (policies, size_of::<WorkflowValuePolicyWitness>()),
        (encoded, 2),
    ] {
        bytes = bytes
            .checked_add(count.checked_mul(width).ok_or(CodecError::LengthOverflow)?)
            .ok_or(CodecError::LengthOverflow)?;
    }
    if generic {
        bytes = bytes
            .checked_add(
                items
                    .checked_mul(size_of::<Vec<u8>>())
                    .ok_or(CodecError::LengthOverflow)?,
            )
            .and_then(|value| value.checked_add(keys))
            .ok_or(CodecError::LengthOverflow)?;
    }
    Ok(bytes)
}

pub(super) fn limits(limits: CodecLimits, generic: bool) -> Result<(usize, usize), CodecError> {
    let count = limits.admission().batch_items();
    let bytes = limits.admission().decision_bytes();
    let keys = if generic { bytes } else { 0 };
    let owned = owned(bytes, count, count, keys, generic)?;
    let mut work = BASE_WORK;
    for (n, multiplier) in [
        (count, ITEM_WORK),
        (count, POLICY_WORK),
        (bytes, BYTE_WORK),
        (keys, KEY_WORK),
    ] {
        work = work
            .checked_add(
                n.checked_mul(multiplier)
                    .ok_or(CodecError::LengthOverflow)?,
            )
            .ok_or(CodecError::LengthOverflow)?;
    }
    Ok((owned, work))
}

pub(super) fn reserve<T>(count: usize, maximum_count: usize) -> Result<Vec<T>, CodecError> {
    if count > maximum_count {
        return Err(CodecError::ItemLimit);
    }
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| CodecError::Allocation)?;
    Ok(values)
}

pub(super) fn admit(attempted: usize, maximum: usize) -> Result<(), CodecError> {
    if attempted > maximum {
        Err(CodecError::OwnedLimit)
    } else {
        Ok(())
    }
}
