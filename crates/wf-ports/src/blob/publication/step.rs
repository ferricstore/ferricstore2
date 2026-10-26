//! One aligned source/result step; omitted predecessor blobs are never traversed.

use crate::blob::publication::scan::ValueEffectsCharge;
use crate::blob::publication::{RecordBatchValueEffect, RecordBatchValueEffectsError as E};
use crate::log::DecodedRequest;
use wf_domain::history::EventKind;
use wf_domain::workflow::batch::ScopedPlan;
use wf_domain::workflow::{Operation, Outcome};

pub(super) fn value_effect(
    decoded: &DecodedRequest<'_>,
    index: usize,
    scoped: &ScopedPlan,
    charge: &mut ValueEffectsCharge,
) -> Result<Option<RecordBatchValueEffect>, E> {
    charge.spend(1)?;
    let items = decoded
        .request()
        .operation
        .mutation_items()
        .ok_or(E::UnsupportedProfile)?;
    let source = items.get(index).ok_or(E::InvalidChanges)?;
    charge.spend(1)?;
    let fields = supplied_value_fields(&source.command.operation, charge)?;
    charge.spend(1)?;
    if (profile(decoded, charge)? == 7)
        != matches!(source.command.operation, Operation::StartAndClaim(_))
    {
        return Err(E::UnsupportedProfile);
    }
    charge.spend(1)?;
    let (left, right) = (source.partition.as_bytes(), scoped.partition().as_bytes());
    if left.len() != right.len() {
        return Err(E::InvalidChanges);
    }
    charge.spend(left.len().checked_add(right.len()).ok_or(E::Limit)?)?;
    if left != right {
        return Err(E::InvalidChanges);
    }
    charge.spend(3)?;
    let plan = scoped.plan();
    let (write, history) = match (plan.write(), plan.history(), plan.outcome()) {
        (Some(write), Some(history), Outcome::Changed) => (write, history),
        (None, None, Outcome::LegacyNoop | Outcome::NotEligible) => return Ok(None),
        _ => return Err(E::InvalidChanges),
    };
    let record = &write.record;
    charge.spend(1)?;
    let kind = match source.command.operation {
        Operation::Create(_) | Operation::StartAndClaim(_) => EventKind::Created,
        Operation::Transition(_) => EventKind::Transitioned,
        Operation::Complete(_) => EventKind::Completed,
        Operation::Fail(_) => EventKind::Failed,
        Operation::Retry(_) | Operation::RetryWithPolicy(_) => EventKind::Retry,
        Operation::ExtendLease(_) => EventKind::LeaseExtended,
        Operation::Cancel(_) => EventKind::Cancelled,
        _ => return Err(E::UnsupportedProfile),
    };
    charge.spend(1)?;
    if history.kind != kind || history.version != record.version() {
        return Err(E::InvalidChanges);
    }
    charge.spend(1)?;
    let (left, right) = (source.command.id.as_bytes(), record.id().as_bytes());
    if left.len() != right.len() {
        return Err(E::InvalidChanges);
    }
    charge.spend(left.len().checked_add(right.len()).ok_or(E::Limit)?)?;
    if left != right {
        return Err(E::InvalidChanges);
    }
    charge.spend(3)?;
    let postimage = [record.payload(), record.result(), record.error()];
    for (supplied, actual) in fields.into_iter().zip(postimage) {
        charge.spend(1)?;
        if let Some(left) = supplied {
            charge.spend(1)?;
            let right = actual.ok_or(E::InvalidChanges)?;
            if left.len() != right.len() {
                return Err(E::InvalidChanges);
            }
            charge.spend(left.len().checked_add(right.len()).ok_or(E::Limit)?)?;
            if left != right {
                return Err(E::InvalidChanges);
            }
        }
    }
    charge.spend(1)?;
    let item_index = u32::try_from(index).map_err(|_| E::Limit)?;
    charge.spend(5)?;
    Ok(Some(RecordBatchValueEffect {
        item_index,
        version: record.version(),
        supplied: fields.map(|field| field.is_some()),
        terminal: matches!(record.state(), b"completed" | b"failed" | b"cancelled"),
    }))
}

pub(super) fn profile(
    decoded: &DecodedRequest<'_>,
    charge: &mut ValueEffectsCharge,
) -> Result<u16, E> {
    let version = decoded.request().intent.version();
    if version != 8 {
        return Ok(version);
    }
    charge.spend(12)?;
    Ok(u16::from(
        decoded
            .workflow_value_metadata()
            .ok_or(E::UnsupportedProfile)?
            .profile,
    ))
}

pub(super) fn supplied_value_fields<'operation>(
    operation: &'operation Operation,
    charge: &mut ValueEffectsCharge,
) -> Result<[Option<&'operation [u8]>; 3], E> {
    Ok(match operation {
        Operation::Create(value) => {
            charge.spend(1)?;
            [value.payload.as_deref(), None, None]
        }
        Operation::Transition(value) => {
            charge.spend(1)?;
            [value.payload.as_deref(), None, None]
        }
        Operation::StartAndClaim(value) => {
            charge.spend(1)?;
            [value.payload.as_deref(), None, None]
        }
        Operation::Complete(value) => {
            charge.spend(2)?;
            [value.payload.as_deref(), value.value.as_deref(), None]
        }
        Operation::Fail(value) => {
            charge.spend(2)?;
            [value.payload.as_deref(), None, value.value.as_deref()]
        }
        Operation::Retry(value) => {
            charge.spend(2)?;
            [value.payload.as_deref(), None, value.error.as_deref()]
        }
        Operation::RetryWithPolicy(value) => {
            charge.spend(2)?;
            [value.payload.as_deref(), None, value.error.as_deref()]
        }
        Operation::Cancel(value) => {
            charge.spend(1)?;
            [None, None, value.error.as_deref()]
        }
        Operation::ExtendLease(_) => [None, None, None],
        _ => return Err(E::UnsupportedProfile),
    })
}
