//! One fire across the shards: read the previous target, plan, create the
//! new target (accepting one this schedule already created), and apply the
//! outcome under the schedule's lease, as `Schedule.fire_one/3` does.

use wf_application::commands::flow;
use wf_application::commands::flow::schedule::definition::Definition;
use wf_application::commands::flow::schedule::plan::{self, Apply, Plan};
use wf_application::commands::flow::schedule::target;
use wf_application::commands::flow::term::{Term, map};
use wf_application::commands::queue::ServiceError;

use crate::supervision::shard::state::schedule::{Claimed, ScheduleOp, Stored};

use super::super::router::Router;
use super::{flow_call, one, op, signal};

fn target_record(
    router: &Router,
    definition: &Definition,
    target_id: &[u8],
) -> Result<Term, ServiceError> {
    let mut options = vec![(b"id".to_vec(), Term::Bin(target_id.to_vec()))];
    if let Some(partition) = definition.target_text("partition_key") {
        options.push((b"partition_key".to_vec(), Term::Bin(partition.to_vec())));
    }
    flow_call(router, flow::parse("FLOW.GET", &Term::Map(options))?)
}

/// The last target's id while it still runs.
fn previous(router: &Router, definition: &Definition) -> Result<Option<Vec<u8>>, ServiceError> {
    let Some(last) = &definition.last_target_id else {
        return Ok(None);
    };
    let record = target_record(router, definition, last)?;
    let active = match record.get(b"state") {
        Some(Term::Bin(state)) => {
            !matches!(state.as_slice(), b"completed" | b"failed" | b"cancelled")
        }
        _ => false,
    };
    Ok(active.then(|| last.clone()))
}

/// Creates a fire's target, accepting one this schedule already created.
fn create_target(
    router: &Router,
    definition: &Definition,
    target_id: &[u8],
    now: u64,
) -> Result<(), ServiceError> {
    let command = flow::parse(
        "FLOW.CREATE",
        &target::create_options(definition, target_id, now),
    )?;
    match flow_call(router, command) {
        Ok(_) => Ok(()),
        Err(error) if error.to_string() == "ERR flow already exists" => {
            if target::owned(
                &target_record(router, definition, target_id)?,
                definition,
                target_id,
            ) {
                Ok(())
            } else {
                Err(ServiceError::rejected(
                    "ERR scheduled target id already exists with different owner".to_owned(),
                ))
            }
        }
        Err(error) => Err(error),
    }
}

fn apply(
    router: &Router,
    claimed: &Claimed,
    then: Apply,
    now: u64,
) -> Result<Stored, ServiceError> {
    let reply = op(
        router,
        ScheduleOp::Apply {
            key: claimed.stored.key.clone(),
            token: claimed.token.clone(),
            fence: claimed.fence,
            apply: Box::new(then),
            now,
        },
        None,
    )?;
    one(reply).ok_or(ServiceError::Unavailable)
}

/// One fire's outcome and the schedule after it.
pub(super) enum Outcome {
    Fired(Vec<u8>, u64, Stored),
    Skipped(String, u64, Stored),
}

/// Fires a claimed schedule: automatic when `fire_at` is `None`.
pub(super) fn fire_one(
    router: &Router,
    claimed: &Claimed,
    fire_at: Option<u64>,
    now: u64,
) -> Result<Outcome, ServiceError> {
    let definition = claimed.stored.definition.clone();
    let constrained = definition
        .overlap
        .is_some_and(|policy| policy.name() != "allow");
    let previous = if constrained {
        previous(router, &definition)?
    } else {
        None
    };
    let planned = match plan::plan(definition.clone(), fire_at, now, previous.as_deref()) {
        Ok(planned) => planned,
        Err(reason) => {
            let mut failed = definition;
            failed.last_planning_error = Some(reason.as_bytes().to_vec());
            failed.end_reason = Some(b"planning_failed".to_vec());
            failed.next_run_at_ms = None;
            let error = map(vec![("reason", Term::text(&reason))]);
            apply(
                router,
                claimed,
                Apply::Fail {
                    definition: failed,
                    error,
                },
                now,
            )?;
            signal(
                router,
                &claimed.stored.definition.id,
                "schedule_failed_planning",
                now,
            );
            return Err(ServiceError::rejected(reason));
        }
    };
    match planned {
        Plan::Fire {
            target_id,
            then,
            coalesced,
        } => {
            if let Err(error) = create_target(router, &definition, &target_id, now) {
                if !matches!(error, ServiceError::Rejected(_)) {
                    // A failure part-way hands the schedule back at once; a
                    // refusal keeps the lease as its backoff, as Elixir does.
                    let due = fire_at.or(definition.next_run_at_ms).unwrap_or(now);
                    let release = Apply::Reschedule {
                        definition,
                        run_at: due,
                    };
                    let _released = apply(router, claimed, release, now);
                }
                return Err(error);
            }
            let stored = apply(router, claimed, then, now)?;
            signal(router, &stored.definition.id, "schedule_fired", now);
            Ok(Outcome::Fired(target_id, coalesced, stored))
        }
        Plan::Hold {
            reason,
            then,
            coalesced,
        } => {
            let stored = apply(router, claimed, then, now)?;
            signal(
                router,
                &stored.definition.id,
                "schedule_skipped_overlap",
                now,
            );
            Ok(Outcome::Skipped(reason, coalesced, stored))
        }
        Plan::Fail { reason, then } => {
            let stored = apply(router, claimed, then, now)?;
            signal(
                router,
                &stored.definition.id,
                "schedule_failed_overlap",
                now,
            );
            Err(ServiceError::rejected(reason))
        }
    }
}
