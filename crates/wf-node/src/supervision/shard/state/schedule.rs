//! Schedules on their shard (ADR-014 step 13): each is an internal Flow
//! whose payload is its definition. Every change rewrites the record and is
//! logged as its after-image, so replay restores it exactly: create or
//! overwrite, claim for a fire (a lease, as Elixir's schedule claim), the
//! fire's outcome under that lease, pause, resume, delete, and the reads
//! the node lists and fires from.

use wf_application::commands::flow::record::FlowMeta;
use wf_application::commands::flow::schedule::definition::Definition;
use wf_application::commands::flow::schedule::guard::Expect;
use wf_application::commands::flow::schedule::plan::Apply;
use wf_application::commands::flow::schedule::target::SCHEDULE_ATTRIBUTE;
use wf_application::commands::flow::schedule::{HISTORY_MAX_EVENTS, SCHEDULE_TYPE};
use wf_application::commands::flow::term::Term;
use wf_application::commands::flow::value_codec;
use wf_application::commands::queue::ServiceError;
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseToken};
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::admission::{AdmissionLimits, MAX_INLINE_BYTES};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_types::identity::RecordId;
use wf_types::version::{Fence, TimestampMillis};

use super::super::command::Logged;
use super::{Key, ShardState, query};

mod control;
mod ops;

pub(crate) use ops::{ScheduleOp, ScheduleReply};

/// A schedule as stored: its Flow fields and definition.
#[derive(Clone, Debug)]
pub(crate) struct Stored {
    pub(crate) key: Key,
    pub(crate) state: Vec<u8>,
    pub(crate) version: u64,
    pub(crate) updated_at: u64,
    pub(crate) attempts: u64,
    pub(crate) definition: Definition,
}

/// A finished target's schedule to wake: the schedule id and the target's.
pub(crate) type Wake = (Vec<u8>, Vec<u8>);

/// A schedule claimed for one fire, with its lease.
#[derive(Clone, Debug)]
pub(crate) struct Claimed {
    pub(crate) stored: Stored,
    pub(crate) token: Vec<u8>,
    pub(crate) fence: u64,
}

fn rejected(text: &str) -> ServiceError {
    ServiceError::rejected(text.to_owned())
}

fn not_found() -> ServiceError {
    rejected("ERR flow schedule not found")
}

fn data_of(record: &WorkflowRecord) -> WorkflowRecordData {
    WorkflowRecordData {
        id: record.id().clone(),
        workflow_type: record.workflow_type().to_vec(),
        state: record.state().to_vec(),
        version: record.version(),
        fence: record.fence(),
        attempts: record.attempts(),
        created_at: record.created_at(),
        updated_at: record.updated_at(),
        next_run_at: record.next_run_at(),
        run_state: record.run_state().map(<[u8]>::to_vec),
        lease: record.lease().cloned(),
        payload: record.payload().map(<[u8]>::to_vec),
        result: record.result().map(<[u8]>::to_vec),
        error: record.error().map(<[u8]>::to_vec),
        latest_kind: EventKind::Transitioned,
        latest_version: record.version(),
        latest_fence: record.fence(),
    }
}

fn terminal(state: &[u8]) -> bool {
    matches!(state, b"completed" | b"failed" | b"cancelled")
}

impl ShardState {
    fn stored(&self, key: &Key) -> Option<Stored> {
        let entry = self.hot(key)?;
        let record = &entry.record;
        if record.workflow_type() != SCHEDULE_TYPE.as_bytes() {
            return None;
        }
        Some(Stored {
            key: key.clone(),
            state: record.state().to_vec(),
            version: record.version(),
            updated_at: record.updated_at().get(),
            attempts: record.attempts(),
            definition: Definition::decode(record.payload().unwrap_or_default())?,
        })
    }

    /// Notes a scheduled target that just finished, so its queued schedule
    /// wakes once the round is durable, as Elixir's
    /// `Scheduler.notify_target_terminal/3`; replay wakes nothing.
    pub(super) fn note_scheduled(&mut self, key: &Key, meta: Option<&FlowMeta>) {
        if !self.live {
            return;
        }
        if let Some(Term::Bin(schedule)) =
            meta.and_then(|meta| meta.attributes.get(SCHEDULE_ATTRIBUTE))
        {
            self.schedule_wakes.push((schedule.clone(), key.1.clone()));
        }
    }

    /// The wakes to deliver once this round is durable.
    pub(crate) fn take_schedule_wakes(&mut self) -> Vec<Wake> {
        std::mem::take(&mut self.schedule_wakes)
    }

    /// The schedule under `key`.
    pub(crate) fn schedule(&self, key: &Key) -> Option<Stored> {
        self.stored(key)
    }

    /// Every schedule on this shard.
    pub(crate) fn schedules(&self) -> Vec<Stored> {
        let keys: Vec<Key> = self
            .catalog
            .get(SCHEDULE_TYPE.as_bytes())
            .into_iter()
            .flat_map(|states| states.values())
            .flat_map(|partitions| partitions.values())
            .flat_map(|members| members.iter().map(|(_, key)| key.clone()))
            .collect();
        keys.iter().filter_map(|key| self.stored(key)).collect()
    }

    /// Writes `data` (version and time raised) with `meta` and logs it.
    fn rewrite(
        &mut self,
        key: &Key,
        mut data: WorkflowRecordData,
        meta: FlowMeta,
        (kind, now): (EventKind, u64),
    ) -> Result<Logged, ServiceError> {
        let version = data
            .version
            .checked_add(1)
            .ok_or(ServiceError::Unavailable)?;
        data.version = version;
        data.updated_at = TimestampMillis::new(now);
        data.latest_kind = kind;
        data.latest_version = version;
        data.latest_fence = data.fence;
        let limits = AdmissionLimits::new(MAX_INLINE_BYTES, usize::MAX, 1)
            .map_err(|_| ServiceError::Unavailable)?;
        let record = restore_record(data, limits)
            .map_err(|_| rejected("ERR flow schedule definition too large"))?;
        let priority = self.hot(key).map_or(0, |entry| entry.priority);
        self.put(key.clone(), (record, priority, Some(Box::new(meta))), None);
        self.after_image(key)
    }

    fn meta_of(&self, key: &Key) -> FlowMeta {
        self.hot(key)
            .and_then(|entry| entry.meta.as_deref().cloned())
            .unwrap_or_else(|| FlowMeta {
                history_max_events: HISTORY_MAX_EVENTS,
                ..self.defaults()
            })
    }

    /// Creates the schedule, or with `overwrite` replaces one no live fire
    /// holds, a lease live by the node's clock `wall` (a caller's time
    /// cannot expire it); a `guard` replaces only the schedule the caller
    /// read.
    pub(crate) fn schedule_create(
        &mut self,
        key: &Key,
        mut definition: Definition,
        (overwrite, guard): (bool, Option<&Expect>),
        (now, wall): (u64, u64),
    ) -> Result<(Stored, Logged), ServiceError> {
        if let Some(guard) = guard {
            let stored = self.stored(key).ok_or_else(not_found)?;
            guard.check(&stored.state, stored.version)?;
        }
        definition.secret = query::cursor_key().to_vec();
        let next = definition.next_run_at_ms.map(TimestampMillis::new);
        let data = match self.hot(key) {
            Some(_) if !overwrite => return Err(rejected("ERR flow already exists")),
            Some(entry)
                if entry
                    .record
                    .lease()
                    .is_some_and(|lease| lease.deadline.get() > wall) =>
            {
                return Err(rejected("ERR flow schedule is currently leased"));
            }
            Some(entry) => {
                let mut data = data_of(&entry.record);
                data.state = b"active".to_vec();
                data.lease = None;
                data.run_state = None;
                data.result = None;
                data.error = None;
                data
            }
            None => WorkflowRecordData {
                id: RecordId::new(key.1.clone()).map_err(|_| ServiceError::InvalidId)?,
                workflow_type: SCHEDULE_TYPE.as_bytes().to_vec(),
                state: b"active".to_vec(),
                version: 0,
                fence: Fence::new(0),
                attempts: 0,
                created_at: TimestampMillis::new(definition.created_at_ms),
                updated_at: TimestampMillis::new(now),
                next_run_at: None,
                run_state: None,
                lease: None,
                payload: None,
                result: None,
                error: None,
                latest_kind: EventKind::Created,
                latest_version: 0,
                latest_fence: Fence::new(0),
            },
        };
        let kind = if data.version == 0 {
            EventKind::Created
        } else {
            EventKind::Transitioned
        };
        let mut data = data;
        data.next_run_at = next;
        data.payload = Some(definition.encode());
        let mut meta = self.meta_of(key);
        meta.terminal_until = None;
        let logged = self.rewrite(key, data, meta, (kind, now))?;
        let stored = self.stored(key).ok_or(ServiceError::Unavailable)?;
        Ok((stored, logged))
    }

    /// Leases the schedule under `key` for a fire from `lease_at`: an
    /// active one, or one whose lease ran out by then (a crashed fire's).
    /// Automatic fires lease on their command time, manual ones on the
    /// node's clock, as Elixir's.
    pub(crate) fn schedule_claim(
        &mut self,
        key: &Key,
        (now, lease_at): (u64, u64),
        (worker, lease_ms): (&[u8], u64),
        expect: &Expect,
    ) -> Result<(Claimed, Logged), ServiceError> {
        let stored = self.stored(key).ok_or_else(not_found)?;
        expect.check(&stored.state, stored.version)?;
        let entry = self.hot(key).ok_or_else(not_found)?;
        let expired = entry
            .record
            .lease()
            .is_some_and(|lease| lease.deadline.get() <= lease_at);
        if stored.state.as_slice() != b"active" && !expired {
            return Err(rejected("ERR flow schedule is not active"));
        }
        let mut data = data_of(&entry.record);
        data.fence = Fence::new(data.fence.get().saturating_add(1));
        let mut token = worker.to_vec();
        token.extend_from_slice(format!(":{now}:{}", data.fence.get()).as_bytes());
        data.lease = Some(Lease {
            worker: worker.to_vec(),
            token: LeaseToken::new(token.clone()).map_err(ServiceError::Domain)?,
            deadline: TimestampMillis::new(lease_at.saturating_add(lease_ms)),
        });
        data.state = b"running".to_vec();
        data.run_state = Some(b"active".to_vec());
        data.next_run_at = data.lease.as_ref().map(|lease| lease.deadline);
        let fence = data.fence.get();
        let meta = self.meta_of(key);
        let logged = self.rewrite(key, data, meta, (EventKind::Claimed, now))?;
        let stored = self.stored(key).ok_or(ServiceError::Unavailable)?;
        Ok((
            Claimed {
                stored,
                token,
                fence,
            },
            logged,
        ))
    }

    /// The keys of schedules due by `now`, and of fires whose lease ran out,
    /// at most `limit`.
    pub(crate) fn due_schedules(&self, now: u64, limit: usize) -> Vec<Key> {
        let mut due: Vec<(u64, u64, Key)> = self
            .queues
            .iter()
            .filter(|(queue, _)| {
                queue.flow_type.as_slice() == SCHEDULE_TYPE.as_bytes()
                    && matches!(queue.state.as_slice(), b"active" | b"running")
            })
            .flat_map(|(_, slots)| slots.range(..=(now, u64::MAX)))
            .map(|((at, sequence), key)| (*at, *sequence, key.clone()))
            .collect();
        due.sort_unstable_by_key(|(at, sequence, _)| (*at, *sequence));
        due.into_iter().take(limit).map(|(_, _, key)| key).collect()
    }

    /// Applies a fire's outcome under its lease.
    pub(crate) fn schedule_apply(
        &mut self,
        key: &Key,
        (token, fence): (&[u8], u64),
        apply: Apply,
        (now, wall): (u64, u64),
    ) -> Result<(Stored, Logged), ServiceError> {
        let entry = self.hot(key).ok_or_else(not_found)?;
        let held = entry
            .record
            .lease()
            .is_some_and(|lease| lease.token.as_bytes() == token)
            && entry.record.fence().get() == fence;
        if !held {
            return Err(rejected("ERR stale flow lease"));
        }
        let mut data = data_of(&entry.record);
        let mut meta = self.meta_of(key);
        data.lease = None;
        data.run_state = None;
        let kind = match apply {
            Apply::Reschedule { definition, run_at } => {
                data.state = b"active".to_vec();
                data.next_run_at = Some(TimestampMillis::new(run_at));
                data.payload = Some(definition.encode());
                EventKind::Transitioned
            }
            Apply::Complete { definition, result } => {
                data.state = b"completed".to_vec();
                data.next_run_at = None;
                data.payload = Some(definition.encode());
                data.result = value_codec::store(&result);
                meta.terminal_until = Some(now.max(wall).saturating_add(meta.retention_ttl_ms));
                EventKind::Completed
            }
            Apply::Fail { definition, error } => {
                data.state = b"failed".to_vec();
                data.next_run_at = None;
                data.payload = Some(definition.encode());
                data.error = value_codec::store(&error);
                meta.terminal_until = Some(now.max(wall).saturating_add(meta.retention_ttl_ms));
                EventKind::Failed
            }
        };
        let logged = self.rewrite(key, data, meta, (kind, now))?;
        let stored = self.stored(key).ok_or(ServiceError::Unavailable)?;
        Ok((stored, logged))
    }
}
