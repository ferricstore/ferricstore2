//! What a recurring schedule does while its previous target still runs:
//! skip the occurrence, queue it with a doubling retry, or fail.

use super::super::super::term::{Term, map};
use super::super::definition::Definition;
use super::{Apply, Next, add, catchup, end_reason, ended, next};

const DEFAULT_OVERLAP_RETRY_MS: u64 = 1_000;
const MAX_OVERLAP_BACKOFF_MS: u64 = 30_000;

fn skipped(
    mut definition: Definition,
    due: u64,
    next_run: u64,
    reason: &str,
    now: u64,
) -> Definition {
    definition.skipped_count = definition.skipped_count.saturating_add(1);
    definition.last_skipped_at_ms = Some(due);
    definition.last_overlap_at_ms = Some(now);
    definition.last_overlap_target_id = definition.last_target_id.clone();
    definition.last_overlap_reason = Some(reason.as_bytes().to_vec());
    definition.next_run_at_ms = Some(next_run);
    definition.overlap_queued_due_at_ms = None;
    definition.overlap_retry_streak = None;
    definition
}

fn complete_skipped(definition: Definition, reason: &str) -> Apply {
    let mut definition = definition;
    definition.end_reason = Some(reason.as_bytes().to_vec());
    definition.next_run_at_ms = None;
    let result = map(vec![
        ("end_reason", Term::text(reason)),
        ("fire_count", Term::int(definition.fire_count)),
        ("skipped_count", Term::int(definition.skipped_count)),
    ]);
    Apply::Complete { definition, result }
}

pub(super) fn skip(
    definition: Definition,
    due: u64,
    reason: &str,
    now: u64,
    manual: bool,
) -> Result<(Apply, u64), String> {
    Ok(match next(&definition, due, (!manual).then_some(now))? {
        Next::At(run_at, coalesced) => {
            let next_definition = catchup(
                skipped(definition, due, run_at, reason, now),
                now,
                coalesced,
            );
            let count = next_definition.fire_count;
            let apply = match end_reason(&next_definition, run_at, count) {
                None => Apply::Reschedule {
                    definition: next_definition,
                    run_at,
                },
                Some(end) => complete_skipped(next_definition, end),
            };
            (apply, coalesced)
        }
        Next::EndAt(coalesced) => (
            complete_skipped(
                catchup(skipped(definition, due, due, reason, now), now, coalesced),
                "end_at_ms",
            ),
            coalesced,
        ),
        Next::Limit(coalesced) => (
            complete_skipped(
                catchup(skipped(definition, due, due, reason, now), now, coalesced),
                "timestamp_limit",
            ),
            coalesced,
        ),
    })
}

/// `overlap_retry_delay/2`: the base retry, doubling while the same
/// occurrence keeps waiting on the same target, up to 30 s (or the base).
fn retry_delay(definition: &Definition, due: u64, manual: bool) -> (u64, u64) {
    let base = definition
        .overlap_retry_ms
        .unwrap_or(DEFAULT_OVERLAP_RETRY_MS);
    if manual {
        return (base, 1);
    }
    let previous = definition.overlap_retry_streak.unwrap_or(1);
    let same = definition.overlap_queued_due_at_ms == Some(due)
        && definition.last_overlap_target_id == definition.last_target_id
        && previous > 0;
    let streak = if same {
        previous.saturating_add(1).min(16)
    } else {
        1
    };
    let factor = 1_u64
        .checked_shl(u32::try_from(streak.saturating_sub(1)).unwrap_or(0))
        .unwrap_or(u64::MAX);
    (
        base.saturating_mul(factor)
            .min(base.max(MAX_OVERLAP_BACKOFF_MS)),
        streak,
    )
}

pub(super) fn queue(
    definition: Definition,
    due: u64,
    reason: &str,
    now: u64,
    manual: bool,
) -> Apply {
    let (retry, streak) = retry_delay(&definition, due, manual);
    let mut queued = definition;
    queued.overlap_queued_due_at_ms = Some(due);
    queued.last_overlap_at_ms = Some(now);
    queued
        .last_overlap_target_id
        .clone_from(&queued.last_target_id);
    queued.last_overlap_reason = Some(reason.as_bytes().to_vec());
    match add(now, retry) {
        Some(run_at) => {
            queued.next_run_at_ms = Some(run_at);
            if !manual {
                queued.overlap_retry_streak = Some(streak);
            }
            Apply::Reschedule {
                definition: queued,
                run_at,
            }
        }
        None => complete_skipped(ended(queued, "timestamp_limit"), "timestamp_limit"),
    }
}

pub(super) fn fail(definition: Definition, reason: &str, now: u64) -> Apply {
    let error = map(vec![
        (
            "previous_target_id",
            Term::optional_bin(definition.last_target_id.as_deref()),
        ),
        ("reason", Term::text(reason)),
        ("schedule_id", Term::Bin(definition.id.clone())),
    ]);
    let mut failed = ended(definition, "overlap_failed");
    failed.last_overlap_at_ms = Some(now);
    failed.last_overlap_target_id = failed.last_target_id.clone();
    failed.last_overlap_reason = Some(reason.as_bytes().to_vec());
    Apply::Fail {
        definition: failed,
        error,
    }
}
