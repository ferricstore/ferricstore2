//! Limits, as `Governance.CreditLease` and `LimitStore` keep them: a
//! scope's credits, leased out per node shard. A lease takes up to the
//! credits asked for from what is free (marking credits on other leases for
//! reclaim when there are none); a spend moves a lease's available credits
//! into use under generated reservation ids; a release by those ids returns
//! them to the lease. Expired leases are reclaimed; a lease whose spent
//! credits are all released starts a new generation, as Elixir retires a
//! released reservation epoch.

use std::collections::{BTreeMap, BTreeSet};

use super::super::FlowError;
use super::super::MAX_EXACT;
use super::super::effect::denial::{Denial, Field};
use super::super::term::{Term, map};

mod decide;
mod request;
mod stored;

pub use request::{Command, Configuration, Fields, List, Op, parse, parse_list};

/// Most reservation ids one lease generation records.
const MAX_RECORDED: u64 = 256 * 256;

/// One shard's lease of a scope's credits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    /// The shard.
    pub shard_id: u64,
    /// Its generation.
    pub epoch: u64,
    /// When it lapses.
    pub expires_at_ms: u64,
    /// Credits it may spend.
    pub available: u64,
    /// Credits spent and not released.
    pub in_use: u64,
    /// Credits another shard asked for back.
    pub pending_reclaim: u64,
    /// The last spend.
    pub last_spend_at_ms: Option<u64>,
    /// Reservation ids in use in this generation.
    pub active: BTreeSet<Vec<u8>>,
    /// Reservation ids recorded in this generation.
    pub recorded: u64,
}

/// A scope's credits and their leases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    /// The scope.
    pub scope: Vec<u8>,
    /// Credits in all.
    pub limit: u64,
    /// Credits not leased.
    pub free: u64,
    /// The configuration's version.
    pub config_version: u64,
    /// The newest lease generation.
    pub epoch: u64,
    /// The policy version's fingerprint: SHA-256 of its Erlang term.
    pub policy_version: Option<[u8; 32]>,
    /// Leases by shard.
    pub leases: BTreeMap<u64, Lease>,
    /// Spend batches so far, numbering generated reservation ids.
    pub batches: u64,
}

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

impl Lease {
    fn new(shard_id: u64, epoch: u64) -> Self {
        Self {
            shard_id,
            epoch,
            expires_at_ms: 0,
            available: 0,
            in_use: 0,
            pending_reclaim: 0,
            last_spend_at_ms: None,
            active: BTreeSet::new(),
            recorded: 0,
        }
    }

    /// The lease as Elixir replies with it.
    #[must_use]
    pub fn view(&self) -> Term {
        map(vec![
            ("shard_id", Term::int(self.shard_id)),
            ("epoch", Term::int(self.epoch)),
            ("expires_at_ms", Term::int(self.expires_at_ms)),
            ("available", Term::int(self.available)),
            ("in_use", Term::int(self.in_use)),
            ("pending_reclaim", Term::int(self.pending_reclaim)),
            ("drain_rate", Term::Float(0.0)),
            (
                "last_spend_at_ms",
                Term::optional_int(self.last_spend_at_ms),
            ),
        ])
    }
}

impl Owner {
    /// `CreditLease.owner/3`.
    #[must_use]
    pub fn new(
        scope: &[u8],
        limit: u64,
        config_version: u64,
        policy_version: Option<[u8; 32]>,
    ) -> Self {
        Self {
            scope: scope.to_vec(),
            limit,
            free: limit,
            config_version,
            epoch: 0,
            policy_version,
            leases: BTreeMap::new(),
            batches: 0,
        }
    }

    /// The owner as Elixir replies with it.
    #[must_use]
    pub fn view(&self) -> Term {
        let hash = self
            .policy_version
            .map(|digest| Term::Bin(crate::routing::base64url_nopad(&digest).into_bytes()));
        map(vec![
            ("scope", Term::Bin(self.scope.clone())),
            ("limit", Term::int(self.limit)),
            ("free", Term::int(self.free)),
            ("config_version", Term::int(self.config_version)),
            ("epoch", Term::int(self.epoch)),
            (
                "leases",
                Term::Map(
                    self.leases
                        .iter()
                        .map(|(shard, lease)| (shard.to_string().into_bytes(), lease.view()))
                        .collect(),
                ),
            ),
            ("policy_version_hash", hash.unwrap_or(Term::Nil)),
        ])
    }

    fn exhausted(&self, requested: u64) -> FlowError {
        let mut message = b"Governance credits exhausted for ".to_vec();
        message.extend_from_slice(&self.scope);
        let denial = Denial::new(
            "GOVERNANCE_LIMIT_EXCEEDED",
            message,
            vec![
                ("policy", Some(b"limit".to_vec())),
                ("scope", Some(self.scope.clone())),
            ],
        )
        .with("requested", Field::Int(requested))
        .with("free", Field::Int(self.free))
        .with("limit", Field::Int(self.limit));
        FlowError::Governance(Box::new(denial))
    }

    /// `rebalance_capacity/1`: drains available credits, lowest shard first,
    /// when more are leased than the limit allows.
    fn rebalance(&mut self) {
        let allocated = |leases: &BTreeMap<u64, Lease>| {
            leases.values().fold(0_u64, |sum, lease| {
                sum.saturating_add(lease.available)
                    .saturating_add(lease.in_use)
            })
        };
        let mut excess = allocated(&self.leases).saturating_sub(self.limit);
        for lease in self.leases.values_mut() {
            let drained = lease.available.min(excess);
            lease.available = lease.available.saturating_sub(drained);
            lease.pending_reclaim = lease.pending_reclaim.min(lease.available);
            excess = excess.saturating_sub(drained);
        }
        self.free = self.limit.saturating_sub(allocated(&self.leases));
    }

    /// `reclaim_expired/2`.
    pub(crate) fn reclaim(&mut self, now: u64) {
        self.leases.retain(|_, lease| lease.expires_at_ms > now);
        self.rebalance();
    }

    /// `CreditLease.reconfigure/4`.
    fn reconfigure(
        &mut self,
        limit: u64,
        version: u64,
        policy: Option<[u8; 32]>,
    ) -> Result<(), FlowError> {
        if version < self.config_version {
            return Ok(());
        }
        let conflict = policy.is_some() && self.policy_version != policy;
        if version == self.config_version {
            return if limit != self.limit || conflict {
                Err(rejected("ERR flow limit config_version conflict"))
            } else {
                Ok(())
            };
        }
        self.limit = limit;
        self.config_version = version;
        self.policy_version = policy;
        self.rebalance();
        Ok(())
    }

    /// `CreditLease.grant/4`, after reclaiming expired leases. A refusal
    /// marks credits on the other leases for reclaim.
    fn grant(
        &mut self,
        shard: u64,
        requested: u64,
        (now, ttl): (u64, u64),
    ) -> Result<Lease, FlowError> {
        self.reclaim(now);
        let grant = requested.min(self.free);
        if grant == 0 {
            self.mark_reclaim(shard, requested);
            return Err(self.exhausted(requested));
        }
        if !self.leases.contains_key(&shard) && self.epoch >= MAX_EXACT {
            return Err(rejected("ERR flow limit lease generation exhausted"));
        }
        let next = self.epoch.saturating_add(1);
        let lease = self
            .leases
            .entry(shard)
            .or_insert_with(|| Lease::new(shard, next));
        lease.available = lease.available.saturating_add(grant);
        lease.expires_at_ms = lease.expires_at_ms.max(now.saturating_add(ttl));
        lease.pending_reclaim = lease.pending_reclaim.saturating_sub(grant);
        let lease = lease.clone();
        self.free = self.free.saturating_sub(grant);
        self.epoch = self.epoch.max(lease.epoch);
        Ok(lease)
    }

    fn mark_reclaim(&mut self, asking: u64, requested: u64) {
        let mut remaining = requested;
        for (shard, lease) in &mut self.leases {
            if remaining == 0 {
                break;
            }
            if *shard == asking {
                continue;
            }
            let reclaim = lease
                .available
                .saturating_sub(lease.pending_reclaim)
                .min(remaining);
            lease.pending_reclaim = lease.pending_reclaim.saturating_add(reclaim);
            remaining = remaining.saturating_sub(reclaim);
        }
    }

    /// `flow_limit_retire_released_epoch/3`: a lease with recorded
    /// reservations, none in use, starts a new generation.
    fn retire(&mut self, shard: u64) -> Result<(), FlowError> {
        let Some(lease) = self.leases.get(&shard) else {
            return Ok(());
        };
        if lease.in_use != 0 || lease.recorded == 0 {
            return Ok(());
        }
        if self.epoch >= MAX_EXACT {
            return Err(rejected("ERR flow limit lease generation is exhausted"));
        }
        self.epoch = self.epoch.saturating_add(1);
        let epoch = self.epoch;
        if let Some(lease) = self.leases.get_mut(&shard) {
            lease.epoch = epoch;
            lease.recorded = 0;
            lease.active.clear();
        }
        Ok(())
    }

    /// Reservation ids for a spend of `amount` on a lease of `epoch`, in
    /// Elixir's `flr1:<epoch>:<batch>:<n>` form.
    fn reservation_ids(&self, shard: u64, epoch: u64, amount: u64) -> Vec<Vec<u8>> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&self.scope);
        hasher.update(shard.to_be_bytes());
        hasher.update(self.batches.saturating_add(1).to_be_bytes());
        let digest = hasher.finalize();
        let batch = crate::routing::base64url_nopad(digest.get(..16).unwrap_or(&digest));
        (1..=amount)
            .map(|index| format!("flr1:{epoch}:{batch}:{index}").into_bytes())
            .collect()
    }

    /// `CreditLease.spend/4` under ids generated for the lease generation
    /// read before the spend.
    fn spend(
        &mut self,
        shard: u64,
        amount: u64,
        (now, ttl, epoch): (u64, Option<u64>, u64),
    ) -> Result<(Lease, Vec<Vec<u8>>), FlowError> {
        let ids = self.reservation_ids(shard, epoch, amount);
        let Some(held) = self.leases.get(&shard) else {
            return Err(self.exhausted(amount));
        };
        if held.epoch != epoch {
            return Err(rejected("ERR flow limit lease generation changed"));
        }
        if held.last_spend_at_ms.is_some_and(|last| now < last) {
            return Err(rejected(
                "ERR flow limit now_ms cannot precede last_spend_at_ms",
            ));
        }
        if held.available < amount {
            return Err(self.exhausted(amount));
        }
        if held.recorded.saturating_add(amount) > MAX_RECORDED {
            return Err(rejected("ERR flow limit reservation history is full"));
        }
        let lease = self.leases.get_mut(&shard).ok_or(FlowError::Unavailable)?;
        lease.available = lease.available.saturating_sub(amount);
        lease.in_use = lease.in_use.saturating_add(amount);
        lease.last_spend_at_ms = Some(now);
        if let Some(ttl) = ttl {
            lease.expires_at_ms = lease.expires_at_ms.max(now.saturating_add(ttl));
        }
        lease.recorded = lease.recorded.saturating_add(amount);
        lease.active.extend(ids.iter().cloned());
        let lease = lease.clone();
        self.batches = self.batches.saturating_add(1);
        Ok((lease, ids))
    }

    /// Releases the given ids in use on the shard's lease, then retires
    /// its generation once nothing is in use.
    fn release(&mut self, shard: u64, ids: &[Vec<u8>]) -> Result<(), FlowError> {
        if let Some(lease) = self.leases.get_mut(&shard) {
            let freed = ids.iter().filter(|id| lease.active.remove(*id)).count();
            let released = u64::try_from(freed).unwrap_or(u64::MAX).min(lease.in_use);
            lease.in_use = lease.in_use.saturating_sub(released);
            lease.available = lease.available.saturating_add(released);
            self.rebalance();
            self.retire(shard)?;
        }
        Ok(())
    }
}

/// `policy_version_fingerprint/1`: SHA-256 of the version's Erlang
/// external term (a binary, or an integer), as Elixir hashes it.
#[must_use]
pub fn fingerprint(version: &Term) -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let mut term = vec![131_u8];
    match version {
        Term::Bin(bytes) => {
            term.push(109);
            term.extend_from_slice(&u32::try_from(bytes.len()).ok()?.to_be_bytes());
            term.extend_from_slice(bytes);
        }
        Term::Int(number) if (0..=255).contains(number) => {
            term.push(97);
            term.push(u8::try_from(*number).ok()?);
        }
        Term::Int(number) if i32::try_from(*number).is_ok() => {
            term.push(98);
            term.extend_from_slice(&i32::try_from(*number).ok()?.to_be_bytes());
        }
        Term::Int(number) => {
            let magnitude = number.unsigned_abs().to_le_bytes();
            let length = magnitude
                .iter()
                .rposition(|byte| *byte != 0)?
                .saturating_add(1);
            term.push(110);
            term.push(u8::try_from(length).ok()?);
            term.push(u8::from(*number < 0));
            term.extend_from_slice(magnitude.get(..length)?);
        }
        _ => return None,
    }
    Some(Sha256::digest(&term).into())
}
