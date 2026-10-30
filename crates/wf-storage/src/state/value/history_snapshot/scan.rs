//! Complete borrowed parsing and correspondence precede every variable owner.

use wf_domain::history::{EventKind, HistoryEntry, RetryHistory};
use wf_domain::policy::{Backoff, RetryPolicy, validate_retry};
use wf_domain::retry::RetryDecision;
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::request::RequestScope;
use wf_types::identity::{PartitionId, RecordId};
use wf_types::version::{Fence, TimestampMillis};

use super::super::history::{decision, policy_fields, read_time};
use super::super::record::{RecordHistoryView, ScannedWorkflowRecord, event_version};
use super::super::{
    StateValueError, add, canonical_history_position, copy, finish, flag, raw, read_u8, read_u64,
    scan_record_snapshot, start, start_versioned,
};
use super::admit;

pub(crate) struct ScannedHistorySnapshot<'a> {
    pub(super) record: ScannedWorkflowRecord<'a>,
    pub(super) history: BorrowedHistoryFields<'a>,
}

impl<'a> ScannedHistorySnapshot<'a> {
    pub(crate) const fn core(&self) -> &ScannedWorkflowRecord<'a> {
        &self.record
    }
    pub(crate) const fn event_id(&self) -> &'a [u8] {
        self.history.event_id
    }
    pub(crate) const fn version(&self) -> u64 {
        self.history.version
    }
    pub(crate) fn event_owned_bytes(&self) -> Result<usize, StateValueError> {
        self.history.owned_bytes()
    }
    pub(crate) fn into_event(self, maximum: usize) -> Result<HistoryEntry, StateValueError> {
        if maximum == 0 {
            return Err(StateValueError::InvalidValue);
        }
        admit(self.history.owned_bytes()?, maximum)?;
        self.history.into_owned()
    }
}

pub(super) struct BorrowedHistoryFields<'a> {
    event_id: &'a [u8],
    kind: EventKind,
    occurred_at: TimestampMillis,
    version: u64,
    fence: Fence,
    attempts: u64,
    state: &'a [u8],
    next_run_at: Option<TimestampMillis>,
    lease_deadline: TimestampMillis,
    worker: Option<&'a [u8]>,
    retry: Option<BorrowedRetryFields<'a>>,
}

struct BorrowedRetryFields<'a> {
    decision: RetryDecision,
    run_state: &'a [u8],
    next_run_at: Option<TimestampMillis>,
    maximum: u16,
    backoff: Backoff,
    target: &'a [u8],
}

impl<'a> BorrowedHistoryFields<'a> {
    pub(super) fn from_event(event: &'a HistoryEntry) -> Self {
        Self {
            event_id: event.event_id.as_bytes(),
            kind: event.kind,
            occurred_at: event.occurred_at,
            version: event.version,
            fence: event.fence,
            attempts: event.attempts,
            state: &event.state,
            next_run_at: event.next_run_at,
            lease_deadline: event.lease_deadline,
            worker: event.worker.as_deref(),
            retry: event.retry.as_ref().map(|retry| BorrowedRetryFields {
                decision: retry.decision,
                run_state: &retry.run_state,
                next_run_at: retry.next_run_at,
                maximum: retry.policy.max_retries,
                backoff: retry.policy.backoff,
                target: &retry.policy.exhausted_to,
            }),
        }
    }

    pub(super) fn validate(
        &self,
        record: &RecordHistoryView<'_>,
        limits: AdmissionLimits,
    ) -> Result<(), StateValueError> {
        if canonical_history_position(self.event_id)?.1 != self.version
            || self.version != record.version
            || self.fence.get() != record.fence
            || self.attempts != record.attempts
            || self.occurred_at.get() != record.updated
            || self.state != record.state
            || self.next_run_at.map(TimestampMillis::get) != record.next
            || self.worker != record.worker
            || self.lease_deadline.get() != record.deadline
            || (self.kind, self.version, self.fence.get()) != record.latest
            || (self.retry.is_some() && self.kind != EventKind::Retry)
        {
            return Err(StateValueError::InvalidValue);
        }
        for field in [
            Some(self.state),
            self.worker,
            self.retry.as_ref().map(|retry| retry.run_state),
            self.retry.as_ref().map(|retry| retry.target),
        ]
        .into_iter()
        .flatten()
        {
            admit(field.len(), limits.inline_bytes())?;
        }
        if let Some(retry) = &self.retry {
            validate_retry(Some(retry.maximum), Some(retry.backoff), Some(retry.target))
                .map_err(|_| StateValueError::InvalidValue)?;
        }
        Ok(())
    }

    pub(super) fn owned_bytes(&self) -> Result<usize, StateValueError> {
        let mut bytes = 512;
        for field in [
            Some(self.event_id),
            Some(self.state),
            self.worker,
            self.retry.as_ref().map(|retry| retry.run_state),
            self.retry.as_ref().map(|retry| retry.target),
        ]
        .into_iter()
        .flatten()
        {
            add(&mut bytes, field.len())?;
        }
        Ok(bytes)
    }

    pub(super) fn into_owned(self) -> Result<HistoryEntry, StateValueError> {
        Ok(HistoryEntry {
            event_id: String::from_utf8(copy(self.event_id)?)
                .map_err(|_| StateValueError::InvalidValue)?,
            kind: self.kind,
            occurred_at: self.occurred_at,
            version: self.version,
            fence: self.fence,
            attempts: self.attempts,
            state: copy(self.state)?,
            next_run_at: self.next_run_at,
            lease_deadline: self.lease_deadline,
            worker: self.worker.map(copy).transpose()?,
            retry: self
                .retry
                .map(BorrowedRetryFields::into_owned)
                .transpose()?,
        })
    }
}

impl BorrowedRetryFields<'_> {
    fn into_owned(self) -> Result<RetryHistory, StateValueError> {
        Ok(RetryHistory {
            decision: self.decision,
            run_state: copy(self.run_state)?,
            next_run_at: self.next_run_at,
            policy: RetryPolicy {
                max_retries: self.maximum,
                backoff: self.backoff,
                exhausted_to: copy(self.target)?,
            },
        })
    }
}

pub(super) fn scan<'a>(
    bytes: &'a [u8],
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    admission: AdmissionLimits,
    maximum_wire_bytes: usize,
) -> Result<ScannedHistorySnapshot<'a>, StateValueError> {
    let mut offset = start(bytes, *b"FWHS", maximum_wire_bytes)?;
    let record_bytes = raw(bytes, &mut offset)?;
    let history_bytes = raw(bytes, &mut offset)?;
    finish(bytes, offset)?;
    let record = scan_record_snapshot(
        record_bytes,
        scope,
        partition,
        id,
        admission,
        maximum_wire_bytes,
    )?;
    let history = scan_history(history_bytes, scope, partition, id, maximum_wire_bytes)?;
    history.validate(&record.history_view(), admission)?;
    Ok(ScannedHistorySnapshot { record, history })
}

fn scan_history<'a>(
    bytes: &'a [u8],
    scope: &RequestScope,
    partition: &PartitionId,
    id: &RecordId,
    maximum: usize,
) -> Result<BorrowedHistoryFields<'a>, StateValueError> {
    let (mut offset, version) = start_versioned(bytes, *b"FWSH", maximum)?;
    if version != 2 {
        return Err(StateValueError::UnsupportedVersion);
    }
    for expected in [
        scope.group().as_bytes(),
        scope.recovery().as_bytes(),
        scope.tenant().as_bytes(),
        partition.as_bytes(),
        id.as_bytes(),
    ] {
        if raw(bytes, &mut offset)? != expected {
            return Err(StateValueError::InvalidValue);
        }
    }
    let event_id = raw(bytes, &mut offset)?;
    let kind = event_version(read_u8(bytes, &mut offset)?, 2)?;
    let occurred_at = TimestampMillis::new(read_u64(bytes, &mut offset)?);
    let version = read_u64(bytes, &mut offset)?;
    let fence = Fence::new(read_u64(bytes, &mut offset)?);
    let attempts = read_u64(bytes, &mut offset)?;
    let state = raw(bytes, &mut offset)?;
    let next_run_at = read_time(bytes, &mut offset)?;
    let lease_deadline = TimestampMillis::new(read_u64(bytes, &mut offset)?);
    let worker = if flag(bytes, &mut offset)? {
        Some(raw(bytes, &mut offset)?)
    } else {
        None
    };
    let retry = if flag(bytes, &mut offset)? {
        let decision = decision(bytes, &mut offset)?;
        let run_state = raw(bytes, &mut offset)?;
        let next_run_at = read_time(bytes, &mut offset)?;
        let (maximum, backoff, target) = policy_fields(bytes, &mut offset)?;
        Some(BorrowedRetryFields {
            decision,
            run_state,
            next_run_at,
            maximum,
            backoff,
            target,
        })
    } else {
        None
    };
    finish(bytes, offset)?;
    Ok(BorrowedHistoryFields {
        event_id,
        kind,
        occurred_at,
        version,
        fence,
        attempts,
        state,
        next_run_at,
        lease_deadline,
        worker,
        retry,
    })
}
