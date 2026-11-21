//! `FLOW.REWIND` on the shard (ADR-014 step 10), as Elixir's
//! `do_flow_rewind/2` and `flow_rewind_record/4` apply it.
//!
//! A rewind restores the fields one of the Flow's history events recorded:
//! state, attempts, due time (or the request's), priority, payload, result
//! and error references, named values, attributes and state metadata. The
//! version and fence rise, the lease is cleared, the run state stays, a
//! `reason` becomes the error, and the Flow remembers the event in
//! `rewound_to_event_id`. A leased Flow, a parent or child, a wrong state
//! or an event no longer held is refused. Replaced value versions are kept
//! (ADR-016 phase C), so an event's values come back as it named them.

use std::collections::BTreeMap;

use wf_application::commands::flow::history::{HistoryEvent, HistoryKind};
use wf_application::commands::flow::json;
use wf_application::commands::flow::meta::{self, Attributes, StateMeta};
use wf_application::commands::flow::record::{
    FlowMeta, NamedValue, RETAINED_ERROR, RETAINED_NAMED, RETAINED_PAYLOAD, RETAINED_RESULT,
};
use wf_application::commands::flow::rewind::{self, Rewind};
use wf_application::commands::flow::term::Term;
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::history::EventKind;
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::admission::{AdmissionLimits, MAX_INLINE_BYTES};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_types::version::{Fence, TimestampMillis};

use super::super::command::Logged;
use super::flow::Executed;
use super::{Key, ShardState, flow_key};

fn rejected(text: &str) -> ServiceError {
    ServiceError::rejected(text.to_owned())
}

fn not_found() -> ServiceError {
    rejected("ERR flow rewind target event not found")
}

fn terminal_kind(state: &[u8]) -> EventKind {
    match state {
        b"completed" => EventKind::Completed,
        b"failed" => EventKind::Failed,
        b"cancelled" => EventKind::Cancelled,
        _ => EventKind::Transitioned,
    }
}

/// A payload, result or error's version in `meta`.
fn slot(meta: &mut FlowMeta, kind: u8) -> &mut u64 {
    match kind {
        RETAINED_PAYLOAD => &mut meta.payload_version,
        RETAINED_RESULT => &mut meta.result_version,
        _ => &mut meta.error_version,
    }
}

/// The bytes of the version an event named of a payload, result or error:
/// the current ones or a kept version (ADR-016 phase C). The value it
/// replaces is kept, so its references still read.
fn restore(
    meta: &mut FlowMeta,
    kind: u8,
    named: Option<u64>,
    current: Option<&[u8]>,
) -> Result<Option<Vec<u8>>, ServiceError> {
    let stored = *slot(meta, kind);
    if let (Some(wanted), Some(bytes)) = (named, current)
        && wanted == stored
    {
        return Ok(Some(bytes.to_vec()));
    }
    let restored = match named {
        None => None,
        Some(wanted) => {
            let kept = meta.retained(kind, b"", wanted).ok_or_else(not_found)?;
            Some((wanted, kept.bytes.clone()))
        }
    };
    if let Some(bytes) = current {
        meta.retain((kind, b"", stored), bytes);
    }
    let Some((wanted, bytes)) = restored else {
        return Ok(None);
    };
    *slot(meta, kind) = wanted;
    Ok(Some(bytes))
}

/// The payload, result and error an event named; a `reason` replaces the
/// error, keeping the one it replaces.
fn restore_all(
    meta: &mut FlowMeta,
    record: &WorkflowRecord,
    target: &HistoryEvent,
    request: &Rewind,
    next: u64,
) -> Result<Restored, ServiceError> {
    let payload = restore(meta, RETAINED_PAYLOAD, target.refs[0], record.payload())?;
    let result = restore(meta, RETAINED_RESULT, target.refs[1], record.result())?;
    let error = match &request.reason {
        Some(reason) => {
            if let Some(old) = record.error() {
                meta.retain((RETAINED_ERROR, &b""[..], meta.error_version), old);
            }
            meta.error_version = next;
            Some(reason.clone())
        }
        None => restore(meta, RETAINED_ERROR, target.refs[2], record.error())?,
    };
    Ok((payload, result, error))
}

/// A rewind's payload, result and error.
type Restored = (Option<Vec<u8>>, Option<Vec<u8>>, Option<Vec<u8>>);

/// The named values an event referenced, current or kept; the current
/// ones it leaves out or replaces are kept.
fn named_values(
    event: &HistoryEvent,
    meta: &mut FlowMeta,
) -> Result<BTreeMap<Vec<u8>, NamedValue>, ServiceError> {
    let restored = event_values(event, meta)?;
    let replaced: Vec<(Vec<u8>, NamedValue)> = meta
        .values
        .iter()
        .filter(|(name, value)| {
            restored
                .get(*name)
                .is_none_or(|kept| kept.version != value.version)
        })
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    for (name, value) in replaced {
        meta.retain((RETAINED_NAMED, &name, value.version), &value.bytes);
    }
    Ok(restored)
}

fn event_values(
    event: &HistoryEvent,
    meta: &FlowMeta,
) -> Result<BTreeMap<Vec<u8>, NamedValue>, ServiceError> {
    let current = &meta.values;
    let Some(text) = &event.extras[0] else {
        return Ok(BTreeMap::new());
    };
    let Some(Term::Map(refs)) = json::decode(text) else {
        return Err(not_found());
    };
    refs.iter()
        .map(|(name, entry)| {
            let version = match entry.get(b"version") {
                Some(Term::Int(version)) => u64::try_from(*version).ok(),
                _ => None,
            };
            match (current.get(name), version) {
                (Some(value), Some(version)) if value.version == version => {
                    Ok((name.clone(), value.clone()))
                }
                (_, Some(version)) => meta
                    .retained(RETAINED_NAMED, name, version)
                    .map(|kept| {
                        (
                            name.clone(),
                            NamedValue {
                                version,
                                bytes: kept.bytes.clone(),
                            },
                        )
                    })
                    .ok_or_else(not_found),
                (_, None) => Err(not_found()),
            }
        })
        .collect()
}

/// The attributes and state metadata an event recorded.
fn fields(event: &HistoryEvent) -> Result<(Attributes, StateMeta), ServiceError> {
    let attributes = match &event.extras[1] {
        None => Attributes::new(),
        Some(text) => meta::attributes(json::decode(text).as_ref()).map_err(|_| not_found())?,
    };
    let state_meta = match event.extras[2].as_deref().map(json::decode) {
        None => StateMeta::new(),
        Some(Some(Term::Map(states))) => states
            .iter()
            .map(|(state, entries)| {
                let entries = meta::state_meta_update(Some(entries)).map_err(|_| not_found())?;
                Ok((String::from_utf8_lossy(state).into_owned(), entries))
            })
            .collect::<Result<StateMeta, ServiceError>>()?,
        Some(_) => return Err(not_found()),
    };
    Ok((attributes, state_meta))
}

impl ShardState {
    /// `FLOW.REWIND` of the Flow in `partition`.
    pub(super) fn rewind(&mut self, wall: u64, request: &Rewind, partition: Partition) -> Executed {
        match self.try_rewind(wall, request, partition) {
            Ok(logged) => (Ok(Term::text("OK")), Some(logged)),
            Err(error) => (Err(error), None),
        }
    }

    fn try_rewind(
        &mut self,
        wall: u64,
        request: &Rewind,
        partition: Partition,
    ) -> Result<Logged, ServiceError> {
        let now = request.now_ms.unwrap_or(wall);
        let key: Key = flow_key(partition, request.id.clone());
        let defaults = self.defaults();
        let entry = self
            .hot(&key)
            .ok_or_else(|| rejected("ERR flow not found"))?;
        let record = &entry.record;
        let current = entry.meta.as_deref().unwrap_or(&defaults);
        if record.lease().is_some() {
            return Err(rejected("ERR flow cannot rewind leased flow"));
        }
        if current.parent_flow_id.is_some() || !current.child_groups.is_empty() {
            return Err(rejected("ERR flow cannot rewind parent or child flow"));
        }
        if request
            .expect_state
            .as_deref()
            .is_some_and(|state| state != record.state())
        {
            return Err(rejected("ERR flow wrong state"));
        }
        let (at, version) = rewind::event_version(&request.to_event);
        let target = entry
            .history
            .events()
            .into_iter()
            .find(|event| version > 0 && event.version == version && event.at == at)
            .ok_or_else(not_found)?;
        // Elixir answers a claim event (a running state) as not found too.
        if target.state.as_slice() == b"running" {
            return Err(not_found());
        }
        let mut meta = current.clone();
        let next = record
            .version()
            .checked_add(1)
            .ok_or(ServiceError::Unavailable)?;
        let (payload, result, error) = restore_all(&mut meta, record, &target, request, next)?;
        meta.values = named_values(&target, &mut meta)?;
        (meta.attributes, meta.state_meta) = fields(&target)?;
        meta.rewound_to = Some(request.to_event.clone());
        let fence = Fence::new(record.fence().get().saturating_add(1));
        let data = WorkflowRecordData {
            id: record.id().clone(),
            workflow_type: record.workflow_type().to_vec(),
            state: target.state.clone(),
            version: next,
            fence,
            attempts: target.attempts,
            created_at: record.created_at(),
            updated_at: TimestampMillis::new(now),
            next_run_at: request
                .run_at_ms
                .or(target.next_run_at)
                .map(TimestampMillis::new),
            run_state: record.run_state().map(<[u8]>::to_vec),
            lease: None,
            payload,
            result,
            error,
            latest_kind: terminal_kind(&target.state),
            latest_version: next,
            latest_fence: fence,
        };
        let limits = AdmissionLimits::new(MAX_INLINE_BYTES, usize::MAX, 1)
            .map_err(|_| ServiceError::Unavailable)?;
        let restored = restore_record(data, limits).map_err(|_| not_found())?;
        meta.terminal_until = matches!(
            target.state.as_slice(),
            b"completed" | b"failed" | b"cancelled"
        )
        .then(|| now.max(wall).saturating_add(meta.retention_ttl_ms));
        let priority = target.priority;
        let mut event = HistoryEvent::of(&restored, priority, &meta, &key.0);
        event.kind = HistoryKind::Rewound;
        self.put(
            key.clone(),
            (restored, priority, Some(Box::new(meta))),
            Some(&event),
        );
        self.after_image(&key)
    }
}
