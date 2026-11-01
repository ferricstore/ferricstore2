//! Exact original create caller normalization followed by query-row admission.
mod model;
mod record;
use super::I;
use super::{
    OriginalQueryRecordError as Q, OriginalQueryRecordLimits,
    budget::{Budget, add},
};
use crate::blob::{
    OriginalValueBuildError as B, OriginalValuePartition as P, original_auto_partition_bytes,
    original_state_key_prefix, render_state_key_with_prefix,
};
pub use model::{OriginalCreatePreparationError, PreparedOriginalCreate};
use std::{borrow::Cow, mem::size_of};
use wf_domain::workflow::caller::create::{
    OriginalCreateContext, OriginalCreateLimits, OriginalCreateOutcome,
};
use wf_domain::workflow::caller::create::{OriginalCreateError, normalize_original_create};

/// Prepares original create attributes and validates their reserved query row.
/// This is source-helper data. Public reference rejection, value serialization,
/// trusted extension captures and native submission remain separate requirements.
/// # Errors
/// Source failures retain their measured work; operational failures abort preparation.
pub fn prepare_original_create<'r, 'a>(
    id: I<'r, 'a>,
    options: I<'r, 'a>,
    context: OriginalCreateContext<'r, 'a>,
    limits: OriginalCreateLimits,
) -> Result<OriginalCreateOutcome<PreparedOriginalCreate<'r, 'a>>, OriginalCreatePreparationError> {
    use OriginalCreateOutcome::{Accepted, RaisedArgumentError, Rejected};
    let (caller, prior_work, prior_peak) =
        match normalize_original_create(id, options, context, limits)
            .map_err(OriginalCreatePreparationError::Create)?
        {
            Accepted {
                value,
                work_units,
                temporary_bytes,
            } => (value, work_units, temporary_bytes),
            Rejected {
                message,
                work_units,
                temporary_bytes,
            } => {
                return Ok(Rejected {
                    message,
                    work_units,
                    temporary_bytes,
                });
            }
            RaisedArgumentError {
                work_units,
                temporary_bytes,
            } => {
                return Ok(RaisedArgumentError {
                    work_units,
                    temporary_bytes,
                });
            }
        };
    let mut b = Budget::new(OriginalQueryRecordLimits {
        maximum_output_bytes: limits.maximum_output_bytes,
        maximum_work_units: limits
            .maximum_work_units
            .checked_sub(prior_work)
            .filter(|n| *n > 0)
            .ok_or(OriginalCreatePreparationError::Create(
                OriginalCreateError::Limit,
            ))?,
        maximum_temporary_bytes: limits.maximum_temporary_bytes,
    })
    .map_err(OriginalCreatePreparationError::Query)?;
    let result = finish(caller, &mut b);
    let work_units = add(prior_work, b.work).map_err(OriginalCreatePreparationError::Query)?;
    let temporary_bytes = prior_peak.max(b.peak);
    match result {
        Ok(Ok(value)) => Ok(Accepted {
            value,
            work_units,
            temporary_bytes,
        }),
        Ok(Err(message)) => Ok(Rejected {
            message: message.into(),
            work_units,
            temporary_bytes,
        }),
        Err(Q::InvalidRecord) => Ok(Rejected {
            message: "ERR invalid flow query metadata".into(),
            work_units,
            temporary_bytes,
        }),
        Err(Q::MetadataTooLarge) => Ok(Rejected {
            message: "ERR flow query metadata exceeds 32629 bytes".into(),
            work_units,
            temporary_bytes,
        }),
        Err(error) => Err(OriginalCreatePreparationError::Query(error)),
    }
}

fn finish<'r, 'a>(
    caller: wf_domain::workflow::caller::create::OriginalCreateCaller<'r, 'a>,
    b: &mut Budget,
) -> Result<Result<PreparedOriginalCreate<'r, 'a>, &'static str>, Q> {
    b.admit(caller.owned_bytes())?;
    let Some(I::Binary(id)) = caller.field("id") else {
        return Err(Q::Limit);
    };
    let (auto, length) = if caller.explicit_partition().is_none() {
        original_auto_partition_bytes(id).map_err(|_| Q::Limit)?
    } else {
        ([0; 17], 0)
    };
    let partition = caller
        .explicit_partition()
        .unwrap_or(auto.get(..length).ok_or(Q::Limit)?);
    b.spend(
        add(partition.len(), id.len())?
            .checked_mul(16)
            .ok_or(Q::Limit)?,
    )?;
    let (prefix, prefix_len) =
        original_state_key_prefix(id, P::Explicit(partition)).map_err(build_error)?;
    let key_length = add(add(id.len(), 3)?, prefix_len)?;
    if key_length > 65_535 {
        return Ok(Err("ERR key too large (max 65535 bytes)"));
    }
    let owned = add(
        caller.owned_bytes(),
        add(
            add(length, key_length)?,
            size_of::<PreparedOriginalCreate<'_, '_>>().saturating_sub(size_of::<
                wf_domain::workflow::caller::create::OriginalCreateCaller<'_, '_>,
            >()),
        )?,
    )?;
    b.output(owned)?;
    b.admit(add(length, key_length)?)?;
    let key =
        render_state_key_with_prefix(id, prefix.get(..prefix_len).ok_or(Q::Limit)?, key_length)
            .map_err(build_error)?;
    let metadata_bytes = record::validate(&caller, partition, &key, b)?;
    let partition = if let Some(value) = caller.explicit_partition() {
        Cow::Borrowed(value)
    } else {
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(length).map_err(Q::ReserveFailed)?;
        bytes.extend_from_slice(partition);
        Cow::Owned(bytes)
    };
    Ok(Ok(PreparedOriginalCreate {
        caller,
        partition,
        key,
        metadata_bytes,
        owned,
    }))
}

fn build_error(error: B) -> Q {
    match error {
        B::ReserveFailed(error) => Q::ReserveFailed(error),
        B::InvalidInput | B::Limit => Q::Limit,
    }
}
