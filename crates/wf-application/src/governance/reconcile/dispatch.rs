//! Exact grouped original IDs through the existing public credit caller.
use super::super::{
    PublicCreditCallRequest, PublicCreditCallResult as R, submit_public_credit_call,
};
use super::{ReleaseReconcileLimits as L, ReleaseReconcileStats, model::ReleaseStatus as S, plan};
use wf_domain::{
    governance::{
        caller::{CreditCallKind, CreditIdList, CreditOptionValue as V, normalize_credit_call},
        release::ReleaseIntentRef,
    },
    workflow::request::{RequestScope, RetainedStatus},
};
use wf_ports::consensus::{PublicCreditSubmitPort, WorkflowReleaseSubmitPort};

pub(super) fn release<P: PublicCreditSubmitPort + WorkflowReleaseSubmitPort>(
    port: &mut P,
    source: &RequestScope,
    intents: &[ReleaseIntentRef<'_>],
    decisions: &mut [S; 256],
    context: (&mut ReleaseReconcileStats, usize, u64, L),
) {
    let (stats, base, now, limits) = context;
    let mut order = [0_usize; 256];
    let mut length = 0;
    for (at, _) in intents.iter().enumerate() {
        if decisions[at] == S::Release {
            order[length] = at;
            length += 1;
            decisions[at] = S::Error;
        }
    }
    order[..length].sort_unstable_by_key(|i| (intents[*i].scope, intents[*i].shard_id, *i));
    let mut begin = 0;
    while begin < length {
        let first = intents[order[begin]];
        let end = begin
            + order[begin..length]
                .iter()
                .take_while(|i| {
                    intents[**i].scope == first.scope && intents[**i].shard_id == first.shard_id
                })
                .count();
        if plan::admit(base, stats, limits)
            && group(
                port,
                source,
                intents,
                &order[begin..end],
                now,
                stats,
                limits,
            )
        {
            for at in &order[begin..end] {
                decisions[*at] = S::Released;
            }
        }
        begin = end;
    }
}
fn group<P: PublicCreditSubmitPort + WorkflowReleaseSubmitPort>(
    port: &mut P,
    outbox: &RequestScope,
    intents: &[ReleaseIntentRef<'_>],
    indices: &[usize],
    now: u64,
    stats: &mut ReleaseReconcileStats,
    limits: L,
) -> bool {
    let first = intents[indices[0]];
    let Ok(route) = port.workflow_release_credit_route(outbox, first.scope, limits.read) else {
        return false;
    };
    if !valid_route(&route, outbox, first.shard_id, limits) {
        return false;
    }
    let mut unique = [0_usize; 256];
    unique[..indices.len()].copy_from_slice(indices);
    unique[..indices.len()].sort_unstable_by_key(|i| (intents[*i].reservation_id, *i));
    let mut count = 0;
    let mut previous = None;
    for at in 0..indices.len() {
        let index = unique[at];
        let id = intents[index].reservation_id;
        if previous != Some(id) {
            unique[count] = index;
            count += 1;
            previous = Some(id);
        }
    }
    unique[..count].sort_unstable();
    let mut ids = [&[][..]; 256];
    for (id, index) in ids.iter_mut().zip(&unique[..count]) {
        *id = intents[*index].reservation_id;
    }
    let Ok(amount) = i128::try_from(count) else {
        return false;
    };
    let options = [
        ("shard_id", V::Integer(i128::from(first.shard_id))),
        ("amount", V::Integer(amount)),
        (
            "reservation_ids",
            V::Ids(CreditIdList::Borrowed(&ids[..count])),
        ),
        ("now_ms", V::Integer(i128::from(now))),
    ];
    let Ok(caller) = normalize_credit_call(
        CreditCallKind::Release,
        first.scope,
        route.shard_count,
        &options,
        limits.credit.maximum_ids_bytes,
    ) else {
        return false;
    };
    let Ok((id, stamp)) = port.prepare_workflow_release_command(&route.source, now) else {
        return false;
    };
    if stamp.wall().get() < now {
        return false;
    }
    let result = submit_public_credit_call(
        PublicCreditCallRequest {
            source: &route.source,
            target: &route.target,
            catalog: &route.catalog,
            request: &id,
            caller,
        },
        port,
        limits.credit,
    );
    match result {
        Ok(R::Retained(outcome)) => outcome.status() == RetainedStatus::Accepted,
        Ok(R::Unknown { original, .. }) => {
            stats.pending.push(original);
            false
        }
        _ => false,
    }
}
fn valid_route(
    route: &wf_ports::consensus::WorkflowReleaseCreditRoute,
    outbox: &RequestScope,
    shard: u64,
    limits: L,
) -> bool {
    let owned = [
        route.source.tenant().as_bytes(),
        route.source.group().as_bytes(),
        route.source.recovery().as_bytes(),
        route.target.tenant().as_bytes(),
        route.target.group().as_bytes(),
        route.target.recovery().as_bytes(),
        &route.catalog,
    ]
    .into_iter()
    .try_fold(4096_usize, |n, b| n.checked_add(b.len()));
    route.source.tenant() == outbox.tenant()
        && route.target.tenant() == outbox.tenant()
        && route.catalog.len() <= 65535
        && route.shard_count > 0
        && route.shard_count <= wf_domain::workflow::MAX_EXACT_MILLIS
        && shard < route.shard_count
        && owned.is_some_and(|n| n <= limits.read.maximum_bytes)
}
