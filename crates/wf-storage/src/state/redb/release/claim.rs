//! Real paid state checks and exact accepted-record witnesses under one writer.
use super::{
    super::{
        admission::{table_error, total},
        credit::CreditReadContext,
        publication,
        store::RedbStateTransaction,
    },
    claimed, read,
};
use crate::state::key;
use wf_domain::{
    governance::{credit::CreditLimits, release::ReleaseIntentRef},
    workflow::{WorkflowRecord, request::RequestScope},
};
use wf_ports::{
    log::{CapturedGovernedClaimPayment as C, ReleaseBindingRef, encode_release_binding},
    state::{GroupClaimChanges, StateTransactionError as E},
};
use wf_types::{identity::PartitionId, version::TimestampMillis};

/// True only for current live paid IDs; this observation is repeated at publication.
pub(crate) fn available(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    capture: C<'_>,
    time: TimestampMillis,
    minimum: usize,
    credit: CreditLimits,
) -> Result<bool, E> {
    tx.checked(|| observe(tx, scope, capture, time, minimum, credit))
}

fn observe(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    capture: C<'_>,
    time: TimestampMillis,
    minimum: usize,
    credit: CreditLimits,
) -> Result<bool, E> {
    tx.require_governed_claim()?;
    let paid = capture.payment();
    let context = CreditReadContext::new(tx, scope, paid.scope(), credit)?;
    if paid.shard_id() >= context.shards || paid.count() < minimum {
        return Ok(false);
    }
    let Some(owner) = context.owner()? else {
        return Ok(false);
    };
    let leases = &owner.view().leases;
    let Ok(at) = leases.binary_search_by_key(&paid.shard_id(), |lease| lease.shard_id) else {
        return Ok(false);
    };
    let lease = &leases[at];
    if lease.epoch != paid.epoch() || lease.expires_at_ms <= time.get() {
        return Ok(false);
    }
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    for id in paid.reservation_ids() {
        let id = id.map_err(read::codec_error)?;
        let mut key =
            claimed::reservation_key(tx, scope, paid.scope(), paid.shard_id(), paid.epoch(), id)?;
        if !claimed::active(tx, &table, &key, id)? {
            return Ok(false);
        }
        key[6] = 24;
        if read::read(tx, &table, &key, claimed::marker)?.is_some() {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(in crate::state::redb) struct VerifiedGovernedClaimWrites<'a> {
    records: Vec<(&'a PartitionId, &'a WorkflowRecord)>,
}
impl VerifiedGovernedClaimWrites<'_> {
    pub(in crate::state::redb) fn permits(
        &self,
        partition: &PartitionId,
        record: &WorkflowRecord,
    ) -> bool {
        self.records
            .binary_search_by(|(p, r)| {
                (p.as_bytes(), r.id().as_bytes())
                    .cmp(&(partition.as_bytes(), record.id().as_bytes()))
            })
            .is_ok_and(|at| std::ptr::eq(self.records[at].1, record))
    }
}
pub(in crate::state::redb) fn witness<'a>(
    tx: &RedbStateTransaction<'_>,
    changes: &'a GroupClaimChanges<'_, '_>,
    capture: C<'_>,
    credit: CreditLimits,
) -> Result<VerifiedGovernedClaimWrites<'a>, E> {
    if changes
        .plan()
        .control()
        .next()
        .last_entry()
        .is_none_or(|entry| entry.prepared() != capture.prepared())
    {
        return Err(E::InvalidChanges);
    }
    if !available(
        tx,
        changes.scope(),
        capture,
        changes.plan().control().execution_time(),
        capture.requested_limit(),
        credit,
    )? {
        return Err(E::KnownAborted);
    }
    let count = changes
        .business()
        .map_or(0, |business| business.batch().items().len());
    tx.temporary(total(
        std::mem::size_of::<VerifiedGovernedClaimWrites<'_>>(),
        [count
            .checked_mul(std::mem::size_of::<(&PartitionId, &WorkflowRecord)>())
            .ok_or(E::Limit)?],
    )?)?;
    let mut records = Vec::new();
    records
        .try_reserve_exact(count)
        .map_err(|_| E::Allocation)?;
    if let Some(business) = changes.business() {
        for item in business.batch().items() {
            records.push((
                item.partition(),
                &item.plan().write().ok_or(E::InvalidChanges)?.record,
            ));
        }
    }
    records.sort_unstable_by(|(a, x), (b, y)| {
        (a.as_bytes(), x.id().as_bytes()).cmp(&(b.as_bytes(), y.id().as_bytes()))
    });
    if records
        .windows(2)
        .any(|p| (p[0].0, p[0].1.id()) == (p[1].0, p[1].1.id()))
    {
        return Err(E::InvalidChanges);
    }
    Ok(VerifiedGovernedClaimWrites { records })
}
pub(in crate::state::redb) fn publish(
    tx: &RedbStateTransaction<'_>,
    changes: &GroupClaimChanges<'_, '_>,
    capture: C<'_>,
) -> Result<(), E> {
    let Some(business) = changes.business() else {
        return Ok(());
    };
    let scope = changes.scope();
    let paid = capture.payment();
    let mut ids = paid.reservation_ids();
    for item in business.batch().items() {
        let id = ids
            .next()
            .ok_or(E::InvalidChanges)?
            .map_err(read::codec_error)?;
        let record = &item.plan().write().ok_or(E::InvalidChanges)?.record;
        let intent = ReleaseIntentRef {
            flow_id: record.id().as_bytes(),
            partition: Some(item.partition().as_bytes()),
            scope: paid.scope(),
            shard_id: paid.shard_id(),
            reservation_id: id,
            enforcement: paid.enforcement(),
            created_at: record.updated_at(),
        };
        let bound = total(
            256,
            [
                record.id().as_bytes().len(),
                item.partition().as_bytes().len(),
                paid.scope().len(),
                id.len(),
            ],
        )?;
        tx.temporary(bound)?;
        let bytes = encode_release_binding(
            ReleaseBindingRef {
                epoch: paid.epoch(),
                intent,
            },
            bound,
        )
        .map_err(read::codec_error)?;
        let binding_key = tx.key(
            &[
                scope.group().as_bytes(),
                scope.recovery().as_bytes(),
                scope.tenant().as_bytes(),
                item.partition().as_bytes(),
                record.id().as_bytes(),
            ],
            0,
            |cap| {
                key::release::binding(
                    scope,
                    item.partition().as_bytes(),
                    record.id().as_bytes(),
                    cap,
                )
            },
        )?;
        publication::put(tx, &binding_key, &bytes)?;
        let mut key =
            claimed::reservation_key(tx, scope, paid.scope(), paid.shard_id(), paid.epoch(), id)?;
        key[6] = 24;
        publication::put(tx, &key, b"FWGU\0\x01")?;
    }
    Ok(())
}
