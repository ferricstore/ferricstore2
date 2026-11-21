//! The schedule thread (ADR-014 step 13): it runs schedule commands one at
//! a time, so a fire's steps across shards (claim, check the previous
//! target, create the target, apply the outcome) are plain sequential
//! calls, and while idle it fires due schedules, as Elixir's
//! `Flow.Scheduler` polls.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// A schedule command for the thread to run.
pub(crate) type Job = Box<dyn FnOnce() + Send>;

/// Most polls in a row before taking the next command, so a backlog of
/// due schedules does not starve clients.
const MAX_POLLS_IN_A_ROW: usize = 16;

/// Runs `jobs` until their channel closes and, every `interval`, polls
/// with `poll` (when given) until a poll fires nothing, between jobs as
/// well as while idle.
pub(crate) fn run(
    jobs: &Receiver<Job>,
    interval: Duration,
    mut poll: Option<impl FnMut() -> bool>,
) {
    let mut next_poll = Instant::now()
        .checked_add(interval)
        .unwrap_or_else(Instant::now);
    loop {
        let wait = next_poll.saturating_duration_since(Instant::now());
        match jobs.recv_timeout(wait) {
            Ok(job) => job(),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if Instant::now() < next_poll {
            continue;
        }
        if let Some(poll) = poll.as_mut() {
            poll_until_quiet(poll);
        }
        next_poll = Instant::now()
            .checked_add(interval)
            .unwrap_or_else(Instant::now);
    }
}

/// Polls until a poll fires nothing, at most `MAX_POLLS_IN_A_ROW` times.
fn poll_until_quiet(poll: &mut impl FnMut() -> bool) {
    for _ in 0..MAX_POLLS_IN_A_ROW {
        if !poll() {
            return;
        }
    }
}
