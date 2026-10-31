//! Same-guard final-write observations before any retained reply is finalized.
use super::super::{
    admission::{table_error, total},
    store::RedbStateTransaction,
};
use super::read;
use crate::state::key;
use redb::ReadableTable;
use wf_domain::{
    governance::release::{
        ReleaseIntentRef, ReleaseOutboxError, ReleaseOutboxState, append_release_outbox,
    },
    history::EventKind,
    workflow::{
        batch::ScopedPlan,
        control::EntryIdentity,
        indexed::{IndependentIndexedDecision, IndexedDecision},
        request::RequestScope,
    },
};
use wf_ports::{
    log::{
        RecordBatchApplyDecision as D, RecordBatchApplyPlan, encode_release_intent,
        inspect_release_binding, inspect_release_intent,
    },
    state::StateTransactionError as E,
};
use wf_types::version::TimestampMillis;

pub(crate) struct ReleasePublicationPlan {
    pub(super) entry: EntryIdentity,
    pub(super) append: Option<(ReleaseOutboxState, ReleaseOutboxState)>,
    pub(super) entries: Vec<Vec<u8>>,
    pub(super) sorted: Vec<usize>,
}
pub(crate) enum ReleasePreparation {
    Publish(ReleasePublicationPlan),
    BacklogFull,
}

pub(crate) fn prepare(
    tx: &RedbStateTransaction<'_>,
    scope: &RequestScope,
    plan: &RecordBatchApplyPlan<'_>,
) -> Result<ReleasePreparation, E> {
    let context = read::require(tx, scope)?;
    let count = capacity(plan);
    if count > tx.admission.batch_items() {
        return Err(E::InvalidChanges);
    }
    tx.temporary(total(
        std::mem::size_of::<ReleasePublicationPlan>(),
        [count
            .checked_mul(std::mem::size_of::<Vec<u8>>() + std::mem::size_of::<usize>())
            .ok_or(E::Limit)?],
    )?)?;
    let mut prepared = ReleasePublicationPlan {
        entry: plan
            .control()
            .next()
            .last_entry()
            .ok_or(E::InvalidChanges)?,
        append: None,
        entries: Vec::new(),
        sorted: Vec::new(),
    };
    prepared
        .entries
        .try_reserve_exact(count)
        .map_err(|_| E::Allocation)?;
    prepared
        .sorted
        .try_reserve_exact(count)
        .map_err(|_| E::Allocation)?;
    tx.step()?;
    let table = tx.write.open_table(tx.rows_table).map_err(table_error)?;
    walk(tx, plan, |item| {
        with_binding(tx, &table, scope, item, context, |intent| {
            let record = &item.plan().write().ok_or(E::InvalidChanges)?.record;
            if record.state() == b"running" {
                return Ok(());
            }
            let Err(insertion) = locate(&prepared, intent.reservation_id)? else {
                return Ok(());
            };
            let intent = ReleaseIntentRef {
                created_at: record.updated_at(),
                ..intent
            };
            let bound = total(
                64,
                [
                    intent.flow_id.len(),
                    intent.partition.map_or(0, <[u8]>::len),
                    intent.scope.len(),
                    intent.reservation_id.len(),
                ],
            )?;
            tx.temporary(bound)?;
            let encoded = encode_release_intent(intent, bound).map_err(read::codec_error)?;
            let index = prepared.entries.len();
            prepared.entries.push(encoded);
            prepared.sorted.insert(insertion, index);
            Ok(())
        })
    })?;
    if !prepared.entries.is_empty() {
        let original =
            read::meta(tx, &table, scope)?.unwrap_or(ReleaseOutboxState { head: 1, tail: 0 });
        let next = match append_release_outbox(
            original,
            u64::try_from(prepared.entries.len()).map_err(|_| E::Limit)?,
        ) {
            Ok(next) => next,
            Err(ReleaseOutboxError::BacklogFull) => return Ok(ReleasePreparation::BacklogFull),
            Err(_) => return Err(E::Corrupt),
        };
        prepared.append = Some((original, next));
    }
    Ok(ReleasePreparation::Publish(prepared))
}

pub(super) fn capacity(plan: &RecordBatchApplyPlan<'_>) -> usize {
    match plan.decision() {
        D::Fresh {
            operation: IndexedDecision::Accepted(plan),
            ..
        } => plan.batch().items().len(),
        D::IndependentFresh {
            operation: IndependentIndexedDecision::Accepted(plan),
            ..
        } => plan.final_indices().len(),
        _ => 0,
    }
}

pub(super) fn walk<'a>(
    tx: &RedbStateTransaction<'_>,
    plan: &'a RecordBatchApplyPlan<'_>,
    mut visit: impl FnMut(&'a ScopedPlan) -> Result<(), E>,
) -> Result<(), E> {
    match plan.decision() {
        D::Fresh {
            operation: IndexedDecision::Accepted(plan),
            ..
        } => {
            for item in plan.batch().items() {
                if item.plan().write().is_some() {
                    visit(item)?;
                }
            }
        }
        D::IndependentFresh {
            operation: IndependentIndexedDecision::Accepted(plan),
            ..
        } => {
            let count = plan.final_indices().len();
            tx.temporary(
                count
                    .checked_mul(std::mem::size_of::<usize>())
                    .ok_or(E::Limit)?,
            )?;
            let mut selected = Vec::new();
            selected
                .try_reserve_exact(count)
                .map_err(|_| E::Allocation)?;
            selected.extend(plan.final_indices());
            selected.sort_unstable();
            for index in selected {
                visit(
                    plan.results()
                        .get(index)
                        .ok_or(E::InvalidChanges)?
                        .as_ref()
                        .map_err(|_| E::InvalidChanges)?,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn with_binding(
    tx: &RedbStateTransaction<'_>,
    table: &impl ReadableTable<&'static [u8], &'static [u8]>,
    scope: &RequestScope,
    item: &ScopedPlan,
    context: (u64, TimestampMillis),
    visit: impl FnOnce(ReleaseIntentRef<'_>) -> Result<(), E>,
) -> Result<(), E> {
    let record = &item.plan().write().ok_or(E::InvalidChanges)?.record;
    let partition = item.partition();
    let id = record.id();
    let key = tx.key(
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            partition.as_bytes(),
            id.as_bytes(),
        ],
        0,
        |cap| key::release::binding(scope, partition.as_bytes(), id.as_bytes(), cap),
    )?;
    read::read(tx, table, &key, |bytes| {
        let binding = inspect_release_binding(bytes, bytes.len()).map_err(read::codec_error)?;
        read::check_binding(tx, table, scope, (partition, id), binding, context)?;
        // A legacy Claim cannot acquire a paid binding through this writer.
        if item
            .plan()
            .history()
            .is_some_and(|history| history.kind == EventKind::Claimed)
        {
            return Err(E::UnsupportedPolicyProfile);
        }
        visit(binding.intent)
    })?;
    Ok(())
}

pub(super) fn locate(plan: &ReleasePublicationPlan, id: &[u8]) -> Result<Result<usize, usize>, E> {
    let (mut low, mut high) = (0, plan.sorted.len());
    while low < high {
        let mid = low + (high - low) / 2;
        let index = *plan.sorted.get(mid).ok_or(E::InvalidChanges)?;
        let encoded = plan.entries.get(index).ok_or(E::InvalidChanges)?;
        let candidate =
            inspect_release_intent(encoded, encoded.len()).map_err(read::codec_error)?;
        match candidate.reservation_id.cmp(id) {
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
            std::cmp::Ordering::Equal => return Ok(Ok(index)),
        }
    }
    Ok(Err(low))
}
