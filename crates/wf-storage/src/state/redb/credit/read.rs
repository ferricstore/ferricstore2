//! One exact scoped owner read; absence and operational refusal stay distinct.

use super::super::{
    admission::{storage_error, table_error, total},
    store::RedbStateTransaction,
};
use crate::state::key;
use redb::ReadableTable;
use wf_domain::governance::credit::{CreditLeaseData, CreditLimits, CreditOwner};
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::{CodecError, decode_credit_owner};
use wf_ports::state::StateTransactionError as E;

pub(super) fn owner(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    credit_scope: &[u8],
    credit: CreditLimits,
) -> Result<Option<CreditOwner>, E> {
    if scope.group() != tx.group || scope.recovery() != tx.recovery {
        return Err(E::NamespaceConflict);
    }
    if credit_scope.is_empty() || credit_scope.len() > 65_535 {
        return Err(E::InvalidChanges);
    }
    tx.admit_read(total(
        512,
        [tx.group.as_bytes().len(), tx.recovery.as_bytes().len()],
    )?)?;
    let shard_count = tx
        .value_manifest()?
        .credit_shard_count
        .ok_or(E::UnsupportedPolicyProfile)?;
    let physical = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            credit_scope,
        ],
        0,
        |maximum| key::encode_credit_owner_key(scope, credit_scope, maximum),
    )?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    tx.step()?;
    let Some(raw) = table
        .get(physical.as_slice())
        .map_err(|e| storage_error(&e))?
    else {
        return Ok(None);
    };
    if raw.value().len() > 900_000 {
        return Err(E::Corrupt);
    }
    let bound = raw
        .value()
        .len()
        .checked_div(61)
        .and_then(|n| n.checked_mul(std::mem::size_of::<CreditLeaseData>()))
        .and_then(|n| n.checked_add(raw.value().len()))
        .and_then(|n| n.checked_add(2048))
        .ok_or(E::Limit)?;
    tx.admit_read(bound)?;
    let owner = decode_credit_owner(raw.value(), credit, 900_000, bound).map_err(codec_error)?;
    if owner.view().scope != credit_scope
        || owner
            .view()
            .leases
            .iter()
            .any(|lease| lease.shard_id >= shard_count)
    {
        return Err(E::Corrupt);
    }
    Ok(Some(owner))
}

fn codec_error(error: CodecError) -> E {
    match error {
        CodecError::Allocation => E::Allocation,
        CodecError::ByteLimit
        | CodecError::ItemLimit
        | CodecError::OwnedLimit
        | CodecError::WorkLimit => E::Limit,
        _ => E::Corrupt,
    }
}
