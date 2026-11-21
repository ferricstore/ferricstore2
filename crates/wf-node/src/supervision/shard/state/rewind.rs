//! `FLOW.REWIND` on the shard (ADR-014 step 10), as Elixir's
//! `do_flow_rewind/2` and `flow_rewind_record/4` apply it.
//!
//! A rewind restores the fields one of the Flow's history events recorded:
//! state, attempts, due time (or the request's), priority, payload, result
//! and error references, named values, attributes and state metadata. The
//! version and fence rise, the lease is cleared, the run state stays, a
//! `reason` becomes the error, and the Flow remembers the event in
//! `rewound_to_event_id`. A leased Flow, a parent or child, a wrong state
//! or an event no longer held is refused. This node keeps only the current
//! bytes of each value, so an event whose payload, result, error or named
//! value version has been replaced since is refused as not found, where
//! Elixir may still hold the older version.

use std::collections::BTreeMap;

use wf_application::commands::flow::history::{HistoryEvent, HistoryKind};
use wf_application::commands::flow::json;
use wf_application::commands::flow::meta::{self, Attributes, StateMeta};
use wf_application::commands::flow::record::NamedValue;
use wf_application::commands::flow::rewind::{self, Rewind};
use wf_application::commands::flow::term::Term;
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::history::EventKind;
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

/// The current bytes of a value when they are the version the event named.
fn held(
    named: Option<u64>,
    bytes: Option<&[u8]>,
    stored: u64,
) -> Result<Option<Vec<u8>>, ServiceError> {
    match (named, bytes) {
        (None, _) => Ok(None),
        (Some(version), Some(bytes)) if version == stored => Ok(Some(bytes.to_vec())),
        _ => Err(not_found()),
    }
}

/// The named values an event referenced, from the ones still held.
fn named_values(
    event: &HistoryEvent,
    current: &BTreeMap<Vec<u8>, NamedValue>,
) -> Result<BTreeMap<Vec<u8>, NamedValue>, ServiceError> {
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
            match current.get(name) {
                Some(value) if Some(value.version) == version => Ok((name.clone(), value.clone())),
                _ => Err(not_found()),
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
            .records
            .get(&key)
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
        let payload = held(target.refs[0], record.payload(), meta.payload_version)?;
        let result = held(target.refs[1], record.result(), meta.result_version)?;
        let error = match &request.reason {
            Some(reason) => {
                meta.error_version = next;
                meta.error_term = false;
                Some(reason.clone())
            }
            None => held(target.refs[2], record.error(), meta.error_version)?,
        };
        if error.is_none() {
            meta.error_term = false;
        }
        meta.values = named_values(&target, &meta.values)?;
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
