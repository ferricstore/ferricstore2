//! The schedule requests a shard answers, and their dispatch.

use wf_application::commands::flow::schedule::definition::Definition;
use wf_application::commands::flow::schedule::guard::Expect;
use wf_application::commands::flow::schedule::plan::Apply;
use wf_application::commands::queue::ServiceError;

use super::super::super::command::Logged;
use super::super::{Key, ShardState};
use super::{Claimed, Stored};

/// What a schedule request asks of its shard.
pub(crate) enum ScheduleOp {
    /// Create, or with `overwrite` replace (only the schedule `guard`
    /// expects, when given).
    Create {
        key: Key,
        definition: Box<Definition>,
        overwrite: bool,
        guard: Option<Expect>,
        now: u64,
    },
    /// Read one schedule.
    Read { key: Key },
    /// Read every schedule on the shard.
    List,
    /// Lease one schedule for a manual fire.
    Claim {
        key: Key,
        now: u64,
        worker: Vec<u8>,
        lease_ms: u64,
        expect: Expect,
    },
    /// Lease up to `limit` due schedules.
    ClaimDue {
        now: u64,
        limit: usize,
        worker: Vec<u8>,
        lease_ms: u64,
    },
    /// Apply a fire's outcome under its lease.
    Apply {
        key: Key,
        token: Vec<u8>,
        fence: u64,
        apply: Box<Apply>,
        now: u64,
    },
    /// Pause (`pause`) or resume.
    Pause {
        key: Key,
        pause: bool,
        expect: Expect,
        now: u64,
    },
    /// Cancel.
    Delete { key: Key, expect: Expect, now: u64 },
}

/// A schedule request's answer.
pub(crate) enum ScheduleReply {
    /// One schedule, if stored.
    One(Option<Box<Stored>>),
    /// Several.
    Many(Vec<Stored>),
    /// Claimed schedules.
    Claimed(Vec<Claimed>),
    /// Done.
    Done,
}

impl ShardState {
    /// Leases up to `limit` due schedules, skipping any that cannot be.
    fn claim_due_schedules(
        &mut self,
        now: u64,
        limit: usize,
        (worker, lease_ms): (&[u8], u64),
    ) -> (Vec<Claimed>, Vec<Logged>) {
        let mut claimed = Vec::new();
        let mut logged = Vec::new();
        for key in self.due_schedules(now, limit) {
            let claim = self.schedule_claim(&key, (now, worker, lease_ms), &Expect::default());
            if let Ok((one, change)) = claim {
                claimed.push(one);
                logged.push(change);
            }
        }
        (claimed, logged)
    }

    /// Runs one schedule request; returns its answer and the changes to log.
    /// Terminal schedules are kept from the later of command and `wall`
    /// time, so a command clock in the past does not remove them at once.
    pub(crate) fn schedule_op(
        &mut self,
        op: ScheduleOp,
        wall: u64,
    ) -> (Result<ScheduleReply, ServiceError>, Vec<Logged>) {
        let one = |result: Result<(Stored, Logged), ServiceError>| match result {
            Ok((stored, logged)) => (Ok(ScheduleReply::One(Some(Box::new(stored)))), vec![logged]),
            Err(error) => (Err(error), Vec::new()),
        };
        match op {
            ScheduleOp::Create {
                key,
                definition,
                overwrite,
                guard,
                now,
            } => one(self.schedule_create(&key, *definition, (overwrite, guard.as_ref()), now)),
            ScheduleOp::Read { key } => (
                Ok(ScheduleReply::One(self.schedule(&key).map(Box::new))),
                Vec::new(),
            ),
            ScheduleOp::List => (Ok(ScheduleReply::Many(self.schedules())), Vec::new()),
            ScheduleOp::Claim {
                key,
                now,
                worker,
                lease_ms,
                expect,
            } => match self.schedule_claim(&key, (now, &worker, lease_ms), &expect) {
                Ok((claimed, logged)) => (Ok(ScheduleReply::Claimed(vec![claimed])), vec![logged]),
                Err(error) => (Err(error), Vec::new()),
            },
            ScheduleOp::ClaimDue {
                now,
                limit,
                worker,
                lease_ms,
            } => {
                let (claimed, logged) = self.claim_due_schedules(now, limit, (&worker, lease_ms));
                (Ok(ScheduleReply::Claimed(claimed)), logged)
            }
            ScheduleOp::Apply {
                key,
                token,
                fence,
                apply,
                now,
            } => one(self.schedule_apply(&key, (&token, fence), *apply, (now, wall))),
            ScheduleOp::Pause {
                key,
                pause,
                expect,
                now,
            } => one(self.schedule_pause(&key, (pause, &expect), now)),
            ScheduleOp::Delete { key, expect, now } => {
                match self.schedule_delete(&key, &expect, (now, wall)) {
                    Ok(logged) => (Ok(ScheduleReply::Done), vec![logged]),
                    Err(error) => (Err(error), Vec::new()),
                }
            }
        }
    }
}
