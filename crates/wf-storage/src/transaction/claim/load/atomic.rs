//! Real root envelopes load reached state without a synthetic leaf decoder/gate.

use super::{
    BTreeMap, ClaimTransactionError, ClaimTransactionLimits, ClaimWindowEnd, GroupClaimTransaction,
    LoadedClaim, PolicySchemaCharge, RecordBatchApplyError, RequestScope, StateTransactionError,
    debit, reserve_slots, sum,
};
use wf_domain::claim::apply::ClaimEnvelope;
use wf_ports::log::FreshAtomicClaim;

impl LoadedClaim {
    pub(in crate::transaction) fn load_atomic<T: GroupClaimTransaction>(
        tx: &T,
        scope: &RequestScope,
        fresh: &FreshAtomicClaim<'_, '_>,
        envelope: &ClaimEnvelope<'_>,
        limits: ClaimTransactionLimits,
        charge: &mut PolicySchemaCharge,
        prior_staging: usize,
    ) -> Result<Self, ClaimTransactionError> {
        if fresh.prepared().scope() != scope
            || envelope.header.identity.prepared() != fresh.prepared().digest()
        {
            return Err(StateTransactionError::InvalidChanges.into());
        }
        let cap = limits.base.maximum_staging_bytes;
        let mut loaded = Self {
            queries: Vec::new(),
            records: Vec::new(),
            placements: BTreeMap::new(),
            policy: None,
            projection: None,
            parallel: Vec::new(),
            heads: Vec::new(),
            state_keys: Vec::new(),
            parallel_end: ClaimWindowEnd::RangeEnd,
            heads_complete: true,
            staging_used: prior_staging,
        };
        debit(&mut loaded.staging_used, 1024, cap)?;
        loaded.policy = tx.stored_policy_value(
            scope.tenant(),
            envelope.query.workflow_type,
            limits.base.schema,
            charge,
        )?;
        if let Some(policy) = &loaded.policy {
            loaded.projection = Some(super::policy::atomic_projection(
                policy,
                envelope.query.workflow_type,
                limits,
                charge,
                &mut loaded.staging_used,
            )?);
        }
        loaded.load_lanes(tx, scope, envelope.query, envelope.head_budget, cap)?;
        if loaded.heads_complete {
            loaded.load_due(
                tx,
                scope,
                envelope.query,
                fresh.control().execution_time(),
                envelope.scan_budget,
                cap,
            )?;
        }
        reserve_slots(
            &mut loaded.state_keys,
            loaded.queries.len(),
            &mut loaded.staging_used,
            cap,
        )?;
        for (partition, id) in &loaded.queries {
            debit(
                &mut loaded.staging_used,
                sum(&[
                    18,
                    fresh.prepared().isolation_bytes(),
                    partition.as_bytes().len(),
                    id.as_bytes().len(),
                ])?,
                cap,
            )?;
            loaded.state_keys.push(
                fresh
                    .prepared()
                    .state_key(partition, id)
                    .map_err(RecordBatchApplyError::Encoding)?,
            );
        }
        Ok(loaded)
    }
}
