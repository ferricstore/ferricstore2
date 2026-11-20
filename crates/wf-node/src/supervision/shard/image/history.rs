//! The image of a Flow's history events (ADR-014 step 4), carried only by
//! snapshot images: a logged after-image leaves it out because replay
//! derives each event from the record version it stores.
//!
//! Layout: a `u32` count, then per event its kind code, time, version,
//! state, fence, attempts, optional lease (owner, deadline), optional next
//! run time, priority, three optional reference versions, and the optional
//! retry audit (decision, run state, next run time, retry limit, backoff
//! kind, base, cap and jitter, exhaustion target), then the optional JSON of
//! the value references, attributes and state metadata.

use wf_application::commands::flow::history::HistoryEvent;
use wf_domain::history::RetryHistory;
use wf_domain::policy::{Backoff, BackoffKind, RetryPolicy};
use wf_domain::retry::RetryDecision;
use wf_types::version::TimestampMillis;

use super::super::command::{Malformed, Reader, bin, optional_bin, optional_u64};
use super::super::state::events::{kind_code, kind_of};

pub(super) fn encode(out: &mut Vec<u8>, events: &[HistoryEvent]) -> Option<()> {
    out.extend_from_slice(&u32::try_from(events.len()).ok()?.to_be_bytes());
    for event in events {
        out.push(kind_code(event.kind)?);
        for number in [event.at, event.version] {
            out.extend_from_slice(&number.to_be_bytes());
        }
        bin(out, &event.state)?;
        for number in [event.fence, event.attempts] {
            out.extend_from_slice(&number.to_be_bytes());
        }
        match &event.lease {
            None => out.push(0),
            Some((owner, deadline)) => {
                out.push(1);
                bin(out, owner)?;
                out.extend_from_slice(&deadline.to_be_bytes());
            }
        }
        optional_u64(out, event.next_run_at);
        out.push(event.priority);
        for version in event.refs {
            optional_u64(out, version);
        }
        match &event.retry {
            None => out.push(0),
            Some(retry) => {
                out.push(1);
                encode_retry(out, retry)?;
            }
        }
        for extra in &event.extras {
            optional_bin(out, extra.as_deref())?;
        }
    }
    Some(())
}

const BACKOFFS: [BackoffKind; 4] = [
    BackoffKind::None,
    BackoffKind::Fixed,
    BackoffKind::Linear,
    BackoffKind::Exponential,
];

fn encode_retry(out: &mut Vec<u8>, retry: &RetryHistory) -> Option<()> {
    out.push(match retry.decision {
        RetryDecision::Scheduled => 0,
        RetryDecision::Exhausted => 1,
    });
    bin(out, &retry.run_state)?;
    optional_u64(out, retry.next_run_at.map(TimestampMillis::get));
    let policy = &retry.policy;
    out.extend_from_slice(&policy.max_retries.to_be_bytes());
    out.push(
        u8::try_from(
            BACKOFFS
                .iter()
                .position(|kind| *kind == policy.backoff.kind)?,
        )
        .ok()?,
    );
    out.extend_from_slice(&policy.backoff.base_ms.to_be_bytes());
    out.extend_from_slice(&policy.backoff.max_ms.to_be_bytes());
    out.push(policy.backoff.jitter_pct);
    bin(out, &policy.exhausted_to)
}

fn decode_retry(reader: &mut Reader<'_>) -> Result<RetryHistory, Malformed> {
    let decision = match reader.u8()? {
        0 => RetryDecision::Scheduled,
        1 => RetryDecision::Exhausted,
        _ => return Err(Malformed),
    };
    let run_state = reader.bin()?.to_vec();
    let next_run_at = reader.optional_u64()?.map(TimestampMillis::new);
    let max_retries = u16::from_be_bytes(reader.array()?);
    let kind = *BACKOFFS.get(usize::from(reader.u8()?)).ok_or(Malformed)?;
    let base_ms = reader.u64()?;
    let max_ms = reader.u64()?;
    let jitter_pct = reader.u8()?;
    let exhausted_to = reader.bin()?.to_vec();
    Ok(RetryHistory {
        decision,
        run_state,
        next_run_at,
        policy: RetryPolicy {
            max_retries,
            backoff: Backoff {
                kind,
                base_ms,
                max_ms,
                jitter_pct,
            },
            exhausted_to,
        },
    })
}

pub(super) fn decode(reader: &mut Reader<'_>) -> Result<Vec<HistoryEvent>, Malformed> {
    let count = u32::from_be_bytes(reader.array()?);
    let mut events = Vec::new();
    for _ in 0..count {
        let kind = kind_of(reader.u8()?).ok_or(Malformed)?;
        let at = reader.u64()?;
        let version = reader.u64()?;
        let state = reader.bin()?.to_vec();
        let fence = reader.u64()?;
        let attempts = reader.u64()?;
        let lease = match reader.u8()? {
            0 => None,
            1 => Some((reader.bin()?.to_vec(), reader.u64()?)),
            _ => return Err(Malformed),
        };
        events.push(HistoryEvent {
            kind,
            at,
            version,
            state,
            fence,
            attempts,
            lease,
            next_run_at: reader.optional_u64()?,
            priority: reader.u8()?,
            refs: [
                reader.optional_u64()?,
                reader.optional_u64()?,
                reader.optional_u64()?,
            ],
            retry: match reader.u8()? {
                0 => None,
                1 => Some(decode_retry(reader)?),
                _ => return Err(Malformed),
            },
            extras: [
                reader.optional_bin()?,
                reader.optional_bin()?,
                reader.optional_bin()?,
            ],
        });
    }
    Ok(events)
}
