//! Deciding a limit command from the limit's stored form. Like Elixir,
//! the owner is stored again after a refusal that changed it (a reclaim,
//! a reclaim mark, a new owner).

use super::super::super::FlowError;
use super::super::super::term::{Term, map};
use super::super::{Change, Decided};
use super::request::{Configuration, Op};
use super::{Command, Owner};

fn rejected(text: &str) -> FlowError {
    FlowError::rejected(text.to_owned())
}

/// `flow_limit_apply_configuration/2`.
fn configure(owner: &mut Owner, configuration: &Configuration) -> Result<(), FlowError> {
    let Some(limit) = configuration.limit else {
        return Ok(());
    };
    match configuration.config_version {
        None if limit == owner.limit => Ok(()),
        None => Err(rejected(
            "ERR flow limit config_version is required to change limit",
        )),
        Some(version) => owner.reconfigure(limit, version, configuration.policy_version),
    }
}

fn stored(owner: &Owner, previous: Option<&Owner>, reply: Result<Term, FlowError>) -> Decided {
    if previous == Some(owner) {
        return (reply, None);
    }
    match owner.encode() {
        Some(bytes) => (reply, Some(Change::Put(bytes))),
        None => (Err(FlowError::Unavailable), None),
    }
}

impl Command {
    /// Decides the command from the limit's stored form.
    pub fn decide(&self, stored_form: Option<&[u8]>, wall: u64) -> Decided {
        let fields = match &self.fields {
            Ok(fields) => fields,
            Err(error) => return (Err(error.clone()), None),
        };
        let existing = match stored_form.map(Owner::decode) {
            None => None,
            Some(Some(owner)) => Some(owner),
            Some(None) => return (Err(rejected("ERR flow limit record is corrupt")), None),
        };
        let now = fields.now_ms.unwrap_or(wall);
        let shard = self.shard_id;
        let Some(mut owner) = existing.clone().or_else(|| {
            let configuration = &fields.configuration;
            (self.op == Op::Lease)
                .then_some(configuration.limit)
                .flatten()
                .map(|limit| {
                    Owner::new(
                        &self.scope,
                        limit,
                        configuration.config_version.unwrap_or(0),
                        configuration.policy_version,
                    )
                })
        }) else {
            return match self.op {
                Op::Get => (Ok(Term::Nil), None),
                Op::Lease => (
                    Err(rejected(
                        "ERR flow limit limit must be a non-negative integer",
                    )),
                    None,
                ),
                Op::Spend | Op::Release | Op::Renew => {
                    (Err(rejected("ERR flow limit not found")), None)
                }
            };
        };
        let previous = existing.as_ref();
        match self.op {
            Op::Get => {
                owner.reclaim(now);
                let reply = Ok(owner.view());
                stored(&owner, previous, reply)
            }
            Op::Lease => {
                if let Err(error) = configure(&mut owner, &fields.configuration) {
                    return (Err(error), None);
                }
                let ttl = fields.ttl_ms.unwrap_or(0);
                let reply = owner
                    .grant(shard, fields.amount, (now, ttl))
                    .map(|lease| map(vec![("owner", owner.view()), ("lease", lease.view())]));
                stored(&owner, previous, reply)
            }
            Op::Spend => {
                let configuration = match &self.late {
                    Ok(configuration) => configuration,
                    Err(error) => return (Err(error.clone()), None),
                };
                if let Err(error) = configure(&mut owner, configuration) {
                    return (Err(error), None);
                }
                // The generation the spend's ids name is read before the
                // spend reclaims or retires anything.
                let epoch = previous
                    .and_then(|owner| owner.leases.get(&shard))
                    .map_or(0, |lease| lease.epoch);
                owner.reclaim(now);
                if let Err(error) = owner.retire(shard) {
                    return (Err(error), None);
                }
                let reply = owner
                    .spend(shard, fields.amount, (now, fields.ttl_ms, epoch))
                    .map(|(lease, ids)| {
                        map(vec![
                            ("owner", owner.view()),
                            ("lease", lease.view()),
                            (
                                "reservation_ids",
                                Term::List(ids.into_iter().map(Term::Bin).collect()),
                            ),
                        ])
                    });
                stored(&owner, previous, reply)
            }
            Op::Release => {
                let reply = owner.release(shard, &fields.ids).map(|()| owner.view());
                stored(&owner, previous, reply)
            }
            Op::Renew => {
                let reply = renew(&mut owner, shard, (now, fields.ttl_ms.unwrap_or(0)));
                stored(&owner, previous, reply)
            }
        }
    }
}

/// Elixir's `CreditLease.renew/3`: the shard's lease lasts at least until
/// `now + ttl`; a lease already reclaimed is gone.
fn renew(owner: &mut Owner, shard: u64, (now, ttl): (u64, u64)) -> Result<Term, FlowError> {
    owner.reclaim(now);
    let until = now.saturating_add(ttl);
    match owner.leases.get_mut(&shard) {
        Some(lease) => {
            lease.expires_at_ms = lease.expires_at_ms.max(until);
            Ok(lease.view())
        }
        None => Err(rejected("ERR flow limit lease expired")),
    }
}
