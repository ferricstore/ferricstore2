//! One exact scoped owner read; absence and operational refusal stay distinct.

use super::super::{admission::total, store::RedbStateTransaction};
use wf_domain::governance::credit::{CreditLimits, CreditOwner};
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::CodecError;
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
    super::CreditReadContext::new(tx, scope, credit_scope, credit)?.owner()
}

pub(super) fn codec_error(error: CodecError) -> E {
    match error {
        CodecError::Allocation => E::Allocation,
        CodecError::ByteLimit
        | CodecError::ItemLimit
        | CodecError::OwnedLimit
        | CodecError::WorkLimit => E::Limit,
        _ => E::Corrupt,
    }
}
