//! A Flow's history kept as one compact byte stream (ADR-014 step 4).
//!
//! Every Flow, including those of the queue fast path, gets an event per
//! record version, so the in-memory form matters: events are packed with
//! LEB128 integers into one `Vec<u8>` and decoded only when `FLOW.HISTORY`
//! reads them. The stream also remembers where its last event starts, so a
//! change can log its own event without decoding the rest.
//!
//! Event layout: kind code, then `at`, version, fence and attempts as
//! LEB128; state as length and bytes; a lease flag with owner and deadline;
//! next run time plus one (0 for none); priority; the payload, result and
//! error reference versions (0 for none); a retry flag with the audit.

use wf_application::commands::flow::history::{HistoryEvent, HistoryKind};
use wf_domain::history::RetryHistory;
use wf_domain::policy::{Backoff, BackoffKind, RetryPolicy};
use wf_domain::retry::RetryDecision;
use wf_types::version::TimestampMillis;

use super::super::image::KINDS;

const BACKOFFS: [BackoffKind; 4] = [
    BackoffKind::None,
    BackoffKind::Fixed,
    BackoffKind::Linear,
    BackoffKind::Exponential,
];

/// Encoded events, oldest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct History {
    bytes: Vec<u8>,
    count: u32,
    last_start: u32,
}

fn varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let [low, ..] = value.to_le_bytes();
        let byte = low & 0x7F;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn bytes(out: &mut Vec<u8>, value: &[u8]) {
    varint(out, u64::try_from(value.len()).unwrap_or(u64::MAX));
    out.extend_from_slice(value);
}

/// The stream ended inside a value.
struct Truncated;

struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn u8(&mut self) -> Option<u8> {
        let (&byte, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(byte)
    }

    fn varint(&mut self) -> Option<u64> {
        let mut value = 0_u64;
        for shift in (0..64).step_by(7) {
            let byte = self.u8()?;
            value |= u64::from(byte & 0x7F).checked_shl(shift)?;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn bytes(&mut self) -> Option<&'a [u8]> {
        let length = usize::try_from(self.varint()?).ok()?;
        let (head, rest) = self.0.split_at_checked(length)?;
        self.0 = rest;
        Some(head)
    }

    /// A value stored plus one, with 0 for none.
    fn optional(&mut self) -> Result<Option<u64>, Truncated> {
        self.varint()
            .map(|value| value.checked_sub(1))
            .ok_or(Truncated)
    }
}

/// The domain event kinds by their index, then 14 for `value_put`.
pub(crate) fn kind_code(kind: HistoryKind) -> Option<u8> {
    match kind {
        HistoryKind::Event(kind) => {
            u8::try_from(KINDS.iter().position(|known| *known == kind)?).ok()
        }
        HistoryKind::ValuePut => Some(14),
        HistoryKind::Signaled => Some(15),
        HistoryKind::Rewound => Some(16),
    }
}

pub(crate) fn kind_of(code: u8) -> Option<HistoryKind> {
    match code {
        14 => Some(HistoryKind::ValuePut),
        15 => Some(HistoryKind::Signaled),
        16 => Some(HistoryKind::Rewound),
        code => KINDS
            .get(usize::from(code))
            .copied()
            .map(HistoryKind::Event),
    }
}

fn encode(out: &mut Vec<u8>, event: &HistoryEvent) -> Option<()> {
    out.push(kind_code(event.kind)?);
    for number in [event.at, event.version, event.fence, event.attempts] {
        varint(out, number);
    }
    bytes(out, &event.state);
    match &event.lease {
        None => out.push(0),
        Some((owner, deadline)) => {
            out.push(1);
            bytes(out, owner);
            varint(out, *deadline);
        }
    }
    varint(out, event.next_run_at.map_or(0, |at| at.saturating_add(1)));
    out.push(event.priority);
    for version in event.refs {
        varint(out, version.unwrap_or(0));
    }
    match &event.retry {
        None => out.push(0),
        Some(retry) => {
            out.push(1);
            encode_retry(out, retry)?;
        }
    }
    // A bit per present extra (value references, attributes, state
    // metadata), then each one's JSON.
    let mut present = 0_u8;
    for (bit, extra) in event.extras.iter().enumerate() {
        if extra.is_some() {
            present |= 1_u8.checked_shl(u32::try_from(bit).ok()?)?;
        }
    }
    out.push(present);
    for extra in event.extras.iter().flatten() {
        bytes(out, extra);
    }
    Some(())
}

fn encode_retry(out: &mut Vec<u8>, retry: &RetryHistory) -> Option<()> {
    out.push(match retry.decision {
        RetryDecision::Scheduled => 0,
        RetryDecision::Exhausted => 1,
    });
    bytes(out, &retry.run_state);
    varint(
        out,
        retry.next_run_at.map_or(0, |at| at.get().saturating_add(1)),
    );
    let policy = &retry.policy;
    varint(out, u64::from(policy.max_retries));
    out.push(
        u8::try_from(
            BACKOFFS
                .iter()
                .position(|kind| *kind == policy.backoff.kind)?,
        )
        .ok()?,
    );
    varint(out, policy.backoff.base_ms);
    varint(out, policy.backoff.max_ms);
    out.push(policy.backoff.jitter_pct);
    bytes(out, &policy.exhausted_to);
    Some(())
}

fn decode_retry(cursor: &mut Cursor<'_>) -> Option<RetryHistory> {
    let decision = match cursor.u8()? {
        0 => RetryDecision::Scheduled,
        1 => RetryDecision::Exhausted,
        _ => return None,
    };
    let run_state = cursor.bytes()?.to_vec();
    let next_run_at = cursor.optional().ok()?.map(TimestampMillis::new);
    let max_retries = u16::try_from(cursor.varint()?).ok()?;
    let kind = *BACKOFFS.get(usize::from(cursor.u8()?))?;
    let base_ms = cursor.varint()?;
    let max_ms = cursor.varint()?;
    let jitter_pct = cursor.u8()?;
    let exhausted_to = cursor.bytes()?.to_vec();
    Some(RetryHistory {
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

fn decode(cursor: &mut Cursor<'_>) -> Option<HistoryEvent> {
    let kind = kind_of(cursor.u8()?)?;
    let at = cursor.varint()?;
    let version = cursor.varint()?;
    let fence = cursor.varint()?;
    let attempts = cursor.varint()?;
    let state = cursor.bytes()?.to_vec();
    let lease = match cursor.u8()? {
        0 => None,
        1 => Some((cursor.bytes()?.to_vec(), cursor.varint()?)),
        _ => return None,
    };
    let next_run_at = cursor.optional().ok()?;
    let priority = cursor.u8()?;
    let refs = [
        cursor.optional().ok()?,
        cursor.optional().ok()?,
        cursor.optional().ok()?,
    ];
    let refs = refs.map(|version| version.map(|value| value.saturating_add(1)));
    let retry = match cursor.u8()? {
        0 => None,
        1 => Some(decode_retry(cursor)?),
        _ => return None,
    };
    let present = cursor.u8()?;
    let mut extras: [Option<Vec<u8>>; 7] = Default::default();
    for (bit, extra) in extras.iter_mut().enumerate() {
        if present & 1_u8.checked_shl(u32::try_from(bit).ok()?)? != 0 {
            *extra = Some(cursor.bytes()?.to_vec());
        }
    }
    Some(HistoryEvent {
        kind,
        at,
        version,
        state,
        fence,
        attempts,
        lease,
        next_run_at,
        priority,
        refs,
        retry,
        extras,
    })
}

impl History {
    /// Appends an event, then drops the oldest beyond `cap`.
    pub(crate) fn push(&mut self, event: &HistoryEvent, cap: u64) {
        let start = self.bytes.len();
        let mut encoded = Vec::with_capacity(64);
        if encode(&mut encoded, event).is_none() {
            return;
        }
        // Most Flows keep a few events: grow exactly instead of doubling.
        self.bytes.reserve_exact(encoded.len());
        self.bytes.extend_from_slice(&encoded);
        self.last_start = u32::try_from(start).unwrap_or(u32::MAX);
        self.count = self.count.saturating_add(1);
        while u64::from(self.count) > cap.max(1) && self.drop_first() {}
    }

    fn drop_first(&mut self) -> bool {
        let mut cursor = Cursor(&self.bytes);
        if decode(&mut cursor).is_none() {
            return false;
        }
        let length = self.bytes.len().saturating_sub(cursor.0.len());
        self.bytes.drain(..length);
        self.count = self.count.saturating_sub(1);
        self.last_start = self
            .last_start
            .saturating_sub(u32::try_from(length).unwrap_or(u32::MAX));
        true
    }

    /// How many events are kept.
    pub(crate) const fn len(&self) -> u32 {
        self.count
    }

    /// Every event, oldest first; stops at the first undecodable one.
    pub(crate) fn events(&self) -> Vec<HistoryEvent> {
        let mut cursor = Cursor(&self.bytes);
        let mut events = Vec::with_capacity(usize::try_from(self.count).unwrap_or(0));
        while !cursor.0.is_empty() {
            match decode(&mut cursor) {
                Some(event) => events.push(event),
                None => break,
            }
        }
        events
    }

    /// The newest `count` events, oldest first.
    pub(crate) fn last_n(&self, count: usize) -> Vec<HistoryEvent> {
        let mut events = self.events();
        let skip = events.len().saturating_sub(count);
        events.drain(..skip);
        events
    }

    /// The newest event.
    pub(crate) fn last(&self) -> Option<HistoryEvent> {
        let start = usize::try_from(self.last_start).ok()?;
        decode(&mut Cursor(self.bytes.get(start..)?))
    }

    /// A history of these events, in order.
    pub(crate) fn of(events: &[HistoryEvent], cap: u64) -> Self {
        let mut history = Self::default();
        for event in events {
            history.push(event, cap);
        }
        history
    }
}
