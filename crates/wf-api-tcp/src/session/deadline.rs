//! A request's `deadline_ms` (audit gap G62), as Elixir's `check_deadline`:
//! a time on the server's clock after which the request must not run. Zero,
//! or a time not yet passed, runs it; a passed time refuses it before it
//! runs; anything but a non-negative integer is a bad request. The option
//! is taken off before the command reads its options.

use std::time::{SystemTime, UNIX_EPOCH};

use wf_application::commands::flow::Term;

use crate::response::{self, STATUS_BAD_REQUEST, STATUS_ERROR};

use super::connection::ReplyTo;

#[cfg(test)]
mod tests;

const FIELD: &[u8] = b"deadline_ms";

/// Why a request's deadline refuses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Late {
    /// The deadline passed before the request ran.
    Exceeded,
    /// The deadline is not a non-negative integer.
    Invalid,
}

impl Late {
    /// Elixir's refusal text.
    pub(super) const fn message(self) -> &'static str {
        match self {
            Self::Exceeded => "ERR native request deadline exceeded",
            Self::Invalid => "ERR native request deadline must be a non-negative integer",
        }
    }

    /// Sends the refusal with Elixir's status and code.
    pub(super) fn refuse(self, reply: ReplyTo) {
        let (status, code) = match self {
            Self::Exceeded => (STATUS_ERROR, "deadline_exceeded"),
            Self::Invalid => (STATUS_BAD_REQUEST, "invalid_deadline"),
        };
        reply.send(
            0,
            &response::refusal(status, code, self.message()).unwrap_or_default(),
        );
    }
}

/// Checks the deadline `args` carry, if any.
pub(super) fn check(args: &Term) -> Result<(), Late> {
    check_at(args, now_ms())
}

/// Checks the deadline `args` carry against the server's clock at `now`
/// (milliseconds since the epoch): it passed when it is before `now`.
fn check_at(args: &Term, now: i64) -> Result<(), Late> {
    let Term::Map(entries) = args else {
        return Ok(());
    };
    match entries.iter().find(|(key, _)| key.as_slice() == FIELD) {
        None => Ok(()),
        Some((_, Term::Int(deadline))) if *deadline >= 0 => {
            if *deadline > 0 && *deadline < now {
                Err(Late::Exceeded)
            } else {
                Ok(())
            }
        }
        Some(_) => Err(Late::Invalid),
    }
}

/// Checks the deadline `args` carry and takes it off them.
pub(super) fn admit(mut args: Term) -> Result<Term, Late> {
    check(&args)?;
    if let Term::Map(entries) = &mut args {
        entries.retain(|(key, _)| key.as_slice() != FIELD);
    }
    Ok(args)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|since| i64::try_from(since.as_millis()).ok())
        .unwrap_or(i64::MAX)
}
