//! Governed claims and limit-credit releases (ADR-014 step 15), as
//! `ClaimDueAPI` and `Governance.LimitCache` run them.
//!
//! A governed claim first spends one credit per Flow it may take from the
//! limit's lease (on the shard the limit's scope routes to), then claims
//! with those credits, each claimed Flow stamped with one, and releases the
//! credits no Flow took. When no credit can be spent the claim answers
//! with no Flows if none could be claimed, and with the refusal otherwise.
//! An approximate limit spends through a node-local cache: a miss spends a
//! chunk (four times the claim, at most 1000) and keeps what the claim did
//! not use for the next claims, falling back to spending the claim's own
//! amount. The steps run on the schedule thread, one claim at a time.

use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
use std::time::Duration;

use wf_application::commands::flow::governance::Command as GovernanceCommand;
use wf_application::commands::flow::governance::limit::{self, Configuration, Fields, Op};
use wf_application::commands::flow::{FlowCommand, Term};
use wf_application::commands::queue::{ClaimFlows, ClaimedFlow, Partition, ServiceError};

use crate::supervision::shard::state::credit::CreditPool;
use crate::supervision::shard::{Releases, Request};

use super::claim;
use super::router::Router;

mod admit;
mod renew;
pub(crate) mod running;

pub(crate) use renew::renew_first;

const ANSWER: Duration = Duration::from_secs(30);
const MAX_CHUNK: u64 = 1_000;

/// How a claim is governed.
pub(crate) struct Governed {
    /// The limit's scope.
    pub(crate) scope: Vec<u8>,
    /// The lease's shard id.
    pub(crate) shard_id: u64,
    /// Spent directly rather than through the cache.
    pub(crate) strict: bool,
    /// Leased first when the spend fails (a policy's running limit).
    pub(crate) auto: Option<AutoLease>,
}

/// A policy running limit's lease: its size, limit and configuration.
pub(crate) struct AutoLease {
    pub(crate) lease_size: u64,
    pub(crate) limit: u64,
    pub(crate) config_version: Option<u64>,
}

/// Credits spent ahead per limit scope and shard, with their lease's
/// expiry.
pub(crate) type Cache = Arc<Mutex<HashMap<(Vec<u8>, u64), (u64, Vec<Vec<u8>>)>>>;

fn ask(router: &Router, scope: &[u8], command: limit::Command) -> Result<Term, ServiceError> {
    let partition = Partition::parse(scope);
    let (sender, receiver) = mpsc::channel();
    router.send(
        router.route(&partition),
        Request::Flow {
            command: Box::new(FlowCommand::Governance(Box::new(GovernanceCommand::Limit(
                command,
            )))),
            partition,
            done: Box::new(move |result| {
                let _sent = sender.send(result);
            }),
        },
    );
    receiver
        .recv_timeout(ANSWER)
        .unwrap_or(Err(ServiceError::Unavailable))
}

fn configuration(governed: &Governed) -> Configuration {
    governed
        .auto
        .as_ref()
        .map_or_else(Configuration::default, |auto| Configuration {
            limit: Some(auto.limit),
            config_version: auto.config_version,
            policy_version: None,
        })
}

/// Spends `amount` credits; returns their ids and the lease's expiry.
fn spend(
    router: &Router,
    governed: &Governed,
    amount: u64,
    (now, ttl): (u64, u64),
) -> Result<(Vec<Vec<u8>>, u64), ServiceError> {
    let fields = Fields {
        amount,
        ttl_ms: Some(ttl),
        now_ms: Some(now),
        ..Fields::default()
    };
    let command = limit::Command::internal(
        Op::Spend,
        governed.scope.clone(),
        governed.shard_id,
        fields,
        configuration(governed),
    );
    let reply = ask(router, &governed.scope, command)?;
    let ids = match reply.get(b"reservation_ids") {
        Some(Term::List(ids)) => ids
            .iter()
            .filter_map(|id| match id {
                Term::Bin(id) => Some(id.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let expires = match reply
        .get(b"lease")
        .and_then(|lease| lease.get(b"expires_at_ms"))
    {
        Some(Term::Int(at)) => u64::try_from(*at).unwrap_or(0),
        _ => 0,
    };
    Ok((ids, expires))
}

/// A policy limit's lease, taken when a spend fails.
fn lease(
    router: &Router,
    governed: &Governed,
    amount: u64,
    (now, ttl): (u64, u64),
) -> Result<(), ServiceError> {
    let Some(auto) = &governed.auto else {
        return Err(ServiceError::Unavailable);
    };
    let fields = Fields {
        amount: amount.max(auto.lease_size).clamp(1, MAX_CHUNK),
        ttl_ms: Some(ttl),
        now_ms: Some(now),
        configuration: configuration(governed),
        ..Fields::default()
    };
    let command = limit::Command::internal(
        Op::Lease,
        governed.scope.clone(),
        governed.shard_id,
        fields,
        Configuration::default(),
    );
    ask(router, &governed.scope, command).map(|_| ())
}

/// `spend_governance_limit/2`: a strict spend, leasing first when a policy
/// limit's spend fails.
fn spend_strict(
    router: &Router,
    governed: &Governed,
    amount: u64,
    times: (u64, u64),
) -> Result<Vec<Vec<u8>>, ServiceError> {
    match spend(router, governed, amount, times) {
        Ok((ids, _)) => Ok(ids),
        Err(error) if governed.auto.is_none() => Err(error),
        Err(_) => {
            lease(router, governed, amount, times)?;
            spend(router, governed, amount, times).map(|(ids, _)| ids)
        }
    }
}

/// `LimitCache.spend/3`: cached credits, else a chunk, else the amount.
fn spend_cached(
    router: &Router,
    governed: &Governed,
    cache: &Cache,
    amount: u64,
    times: (u64, u64),
) -> Result<Vec<Vec<u8>>, ServiceError> {
    let key = (governed.scope.clone(), governed.shard_id);
    let wanted = usize::try_from(amount).unwrap_or(usize::MAX);
    {
        let mut held = cache.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((expires, ids)) = held.get_mut(&key) {
            if *expires <= times.0 {
                held.remove(&key);
            } else if ids.len() >= wanted {
                return Ok(ids.drain(..wanted).collect());
            }
        }
    }
    let chunk = amount.saturating_mul(4).min(MAX_CHUNK).max(amount);
    if chunk > amount
        && let Ok((mut ids, expires)) = spend(router, governed, chunk, times)
    {
        let rest = ids.split_off(wanted.min(ids.len()));
        let mut held = cache.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = held.entry(key).or_insert((expires, Vec::new()));
        entry.0 = entry.0.max(expires);
        entry.1.extend(rest);
        return Ok(ids);
    }
    spend_strict(router, governed, amount, times)
}

/// Releases credits on the limit's lease, without waiting.
fn release(router: &Router, (scope, shard_id): (&[u8], u64), ids: &[Vec<u8>], now: Option<u64>) {
    for batch in ids.chunks(usize::try_from(MAX_CHUNK).unwrap_or(1)) {
        let fields = Fields {
            amount: u64::try_from(batch.len()).unwrap_or(0),
            now_ms: now,
            ids: batch.to_vec(),
            ..Fields::default()
        };
        let command = limit::Command::internal(
            Op::Release,
            scope.to_vec(),
            shard_id,
            fields,
            Configuration::default(),
        );
        let partition = Partition::parse(scope);
        router.send(
            router.route(&partition),
            Request::Flow {
                command: Box::new(FlowCommand::Governance(Box::new(GovernanceCommand::Limit(
                    command,
                )))),
                partition,
                done: Box::new(|_| {}),
            },
        );
    }
}

/// `governed_claim_definitely_empty?/4`: whether the claim could take no
/// Flow now, as the shards that may hold its Flows count.
fn empty(router: &Router, request: ClaimFlows) -> bool {
    let partitions: Option<Arc<[Partition]>> = request
        .partitions
        .as_ref()
        .map(|names| names.iter().map(|name| Partition::parse(name)).collect());
    let holders: Vec<usize> = router
        .holders(&request, partitions.as_deref())
        .into_iter()
        .map(|(index, _)| index)
        .collect();
    let limit = request.limit;
    let request = Arc::new(request);
    let (sender, receiver) = mpsc::channel();
    for index in &holders {
        let sender = sender.clone();
        router.send(
            *index,
            Request::Peek {
                request: Arc::clone(&request),
                limit,
                partitions: partitions.clone(),
                done: Box::new(move |peeked| {
                    let _sent = sender.send(peeked.available);
                }),
            },
        );
    }
    (0..holders.len()).all(|_| {
        receiver
            .recv_timeout(ANSWER)
            .is_ok_and(|available| available == 0)
    })
}

/// Runs one governed claim.
pub(crate) fn run(
    router: &Arc<Router>,
    mut request: ClaimFlows,
    governed: &Governed,
    cache: &Cache,
    wall: u64,
) -> Result<Vec<ClaimedFlow>, ServiceError> {
    let now = request.now_ms.unwrap_or(wall);
    let amount = u64::try_from(request.limit).unwrap_or(u64::MAX).max(1);
    let times = (now, request.lease_ms);
    let spent = if governed.strict {
        spend_strict(router, governed, amount, times)
    } else {
        spend_cached(router, governed, cache, amount, times)
    };
    let ids = match spent {
        Ok(ids) => ids,
        Err(error) => {
            return if empty(router, request) {
                Ok(Vec::new())
            } else {
                Err(error)
            };
        }
    };
    request.limit = request.limit.min(ids.len());
    request.block_ms = None;
    let pool = Arc::new(CreditPool::new(
        governed.scope.clone(),
        governed.shard_id,
        ids,
    ));
    let (sender, receiver) = mpsc::channel();
    claim::start(
        router,
        request,
        Some(Arc::clone(&pool)),
        Box::new(move |result| {
            let _sent = sender.send(result);
        }),
    );
    let claimed = receiver
        .recv_timeout(ANSWER)
        .unwrap_or(Err(ServiceError::Unavailable));
    let left = pool.left();
    if !left.is_empty() {
        release(
            router,
            (&governed.scope, governed.shard_id),
            &left,
            Some(now),
        );
    }
    claimed
}

/// The shards' credit releaser, routing through the router once it exists.
pub(crate) fn releaser(router: Arc<OnceLock<Weak<Router>>>) -> Releases {
    Arc::new(move |credits| {
        let Some(router) = router.get().and_then(Weak::upgrade) else {
            return;
        };
        let mut grouped: HashMap<(Vec<u8>, u64), Vec<Vec<u8>>> = HashMap::new();
        for credit in credits {
            grouped
                .entry((credit.scope, credit.shard_id))
                .or_default()
                .push(credit.reservation_id);
        }
        for ((scope, shard_id), ids) in grouped {
            release(&router, (&scope, shard_id), &ids, None);
        }
    })
}
