//! Sized reads on a caller's already-authenticated coherent table snapshot.

use std::mem::size_of;

use wf_domain::workflow::request::RequestScope;
use wf_ports::blob::{
    WorkflowValueReadFailure, WorkflowValueReadLimits, WorkflowValueSizedReadBatch,
    WorkflowValueSizedReadValue,
};
use wf_ports::state::StateTransactionError as Error;
use wf_types::version::{HlcTimestamp, TimestampMillis};

use super::super::super::budget::{ValueCharge, product, total};
use super::super::{StateStoreLimits, eligible};
use super::{count_eligible, duplicate, metadata, point};

pub(in crate::state::redb) fn read_captured(
    rows: &redb::ReadOnlyTable<&'static [u8], &'static [u8]>,
    charge: &mut ValueCharge,
    (store_limits, alias_format, limits, incarnation): (
        StateStoreLimits,
        u16,
        WorkflowValueReadLimits,
        u128,
    ),
    scope: &RequestScope,
    references: &[&[u8]],
    time: (HlcTimestamp, TimestampMillis),
    (threshold, held, failed): (usize, usize, &mut bool),
) -> Result<WorkflowValueSizedReadBatch, Error> {
    if references.len() > limits.maximum_references {
        return Err(Error::Limit);
    }
    if threshold == 0 {
        return Err(Error::InvalidChanges);
    }
    if !matches!(alias_format, 1 | 2) {
        return Err(Error::UnsupportedLayout);
    }
    let fixed = total(metadata(scope)?, [held, size_of::<Option<u128>>()])?;
    let slots_bytes = product(
        references.len(),
        size_of::<Result<Option<usize>, WorkflowValueReadFailure>>(),
    )?;
    let value_bytes = product(references.len(), size_of::<WorkflowValueSizedReadValue>())?;
    let count = count_eligible(references, charge)?;
    let mut slots = charge.reserve(references.len(), fixed)?;
    let mut values = charge.reserve(references.len(), total(fixed, [slots_bytes])?)?;
    let live = total(fixed, [slots_bytes, value_bytes])?;
    if count == 0 {
        charge.work(product(references.len(), 128)?)?;
        slots.resize(references.len(), Ok(None));
        return Ok(WorkflowValueSizedReadBatch { values, slots });
    }
    let mut returned = 0;
    charge.work(product(references.len(), 128)?)?;
    for (index, reference) in references.iter().enumerate() {
        charge.work(total(128, [product(reference.len(), 4)?])?)?;
        if !eligible(reference) {
            slots.push(Ok(None));
            continue;
        }
        if let Some(previous) = duplicate(reference, &references[..index], charge)? {
            slots.push(slots[previous]);
            continue;
        }
        let found = match point::captured(
            rows,
            charge,
            (store_limits, alias_format, Some(incarnation)),
            scope,
            reference,
            time,
            (threshold, total(live, [returned])?),
        ) {
            Ok(value) => value,
            Err(Error::Unavailable) => {
                *failed = true;
                Err(WorkflowValueReadFailure::ReadFailed)
            }
            Err(error) => return Err(error),
        };
        charge.candidate()?;
        let slot = match found {
            Ok(Some(value)) => {
                if let WorkflowValueSizedReadValue::Stored(bytes) = &value {
                    returned = total(returned, [bytes.len()])?;
                }
                let index = values.len();
                values.push(value);
                Ok(Some(index))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        };
        slots.push(slot);
    }
    Ok(WorkflowValueSizedReadBatch { values, slots })
}
