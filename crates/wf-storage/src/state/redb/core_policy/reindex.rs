//! One canonical stream, original head custody and repair only after true End.

use super::super::{admission::entry, publication, store::RedbStateTransaction};
use wf_domain::claim::LaneKey;
use wf_domain::policy::complete::CompletePolicyValue;
use wf_domain::policy::definition::StateMode;
use wf_domain::workflow::indexed::index_delta_for_policy_modes;
use wf_domain::workflow::request::RequestScope;
use wf_ports::state::{GroupApplyTransaction, StateTransactionError, StoredWorkflowRecord};

mod heads;
mod rows;
pub(in crate::state::redb) use heads::PolicyReindexHeads;

pub(in crate::state::redb) fn scan(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    before: Option<&CompletePolicyValue>,
    after: &CompletePolicyValue,
) -> Result<PolicyReindexHeads, StateTransactionError> {
    let mut heads = PolicyReindexHeads::new(tx)?;
    let prefix = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
        ],
        0,
        |cap| crate::state::key::encode_record_prefix(scope, cap),
    )?;
    let mut previous: Option<StoredWorkflowRecord> = None;
    while let Some(owner) = rows::next(tx, scope, &prefix, previous.as_ref())? {
        if owner.record.workflow_type() == after.data().workflow_type {
            if tx.placement(&owner.partition)?.as_ref() != Some(tx.group) {
                return Err(StateTransactionError::Corrupt);
            }
            let record = entry(&owner);
            let logical = owner.record.run_state().unwrap_or(owner.record.state());
            let old_mode = mode(before, logical);
            let new_mode = mode(Some(after), logical);
            if LaneKey::for_record(&record).is_some()
                && (old_mode == StateMode::Fifo || new_mode == StateMode::Fifo)
            {
                heads.capture(tx, scope, &owner, old_mode)?;
                rows::verify(tx, scope, &owner, old_mode, true)?;
            } else {
                rows::verify(tx, scope, &owner, old_mode, false)?;
            }
            let delta = index_delta_for_policy_modes(record, old_mode, new_mode);
            publication::index_delta(tx, scope, &delta)?;
        }
        previous = Some(owner);
    }
    Ok(heads)
}
pub(super) fn mode(value: Option<&CompletePolicyValue>, state: &[u8]) -> StateMode {
    value
        .and_then(|p| p.data().states.get(state))
        .and_then(|p| p.mode)
        .unwrap_or(StateMode::Parallel)
}
