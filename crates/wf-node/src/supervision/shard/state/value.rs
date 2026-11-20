//! Named and shared values on the owning shard (ADR-014 step 5).
//!
//! A named value belongs to its Flow: writing different bytes needs the
//! override, raises the value's own version, the record's version and update
//! time, and adds a `value_put` history event; the same bytes change nothing.
//! A shared value lives under its reference until its optional deadline.
//! Reads resolve a reference's hash tag to a partition of this shard.

use wf_application::commands::flow::history::{HistoryEvent, HistoryKind};
use wf_application::commands::flow::keys;
use wf_application::commands::flow::record::{FlowMeta, NamedValue};
use wf_application::commands::flow::term::{Term, map};
use wf_application::commands::flow::value::{self, ValuePut, ValueRef};
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::history::EventKind;
use wf_domain::workflow::WorkflowRecord;
use wf_domain::workflow::admission::{AdmissionLimits, MAX_INLINE_BYTES};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_types::version::TimestampMillis;

use super::super::command::Logged;
use super::flow::Executed;
use super::{Key, ShardState};

/// A shared value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Shared {
    /// The partition its reference names, which routes it on restore.
    pub(crate) partition: Partition,
    pub(crate) bytes: Vec<u8>,
    /// When it stops being readable, if ever.
    pub(crate) expires_at: Option<u64>,
}

/// The record with its version raised and its update time set to `now`.
///
/// The latest-event witness moves to the new version. Elixir's latest event
/// becomes `value_put`, which the domain has no kind for; a retry witness
/// becomes a lease-extension witness so a later retry with the same fence is
/// not taken for a duplicate, as in Elixir.
fn touched(record: &WorkflowRecord, now: u64) -> Option<WorkflowRecord> {
    let (latest_kind, _, _) = record.latest_event_witness();
    let latest_kind = match latest_kind {
        EventKind::Retry => EventKind::LeaseExtended,
        kind => kind,
    };
    let version = record.version().checked_add(1)?;
    let data = WorkflowRecordData {
        id: record.id().clone(),
        workflow_type: record.workflow_type().to_vec(),
        state: record.state().to_vec(),
        version,
        fence: record.fence(),
        attempts: record.attempts(),
        created_at: record.created_at(),
        updated_at: TimestampMillis::new(now.max(record.updated_at().get())),
        next_run_at: record.next_run_at(),
        run_state: record.run_state().map(<[u8]>::to_vec),
        lease: record.lease().cloned(),
        payload: record.payload().map(<[u8]>::to_vec),
        result: record.result().map(<[u8]>::to_vec),
        error: record.error().map(<[u8]>::to_vec),
        latest_kind,
        latest_version: version,
        latest_fence: record.fence(),
    };
    let limits = AdmissionLimits::new(MAX_INLINE_BYTES, usize::MAX, 1).ok()?;
    restore_record(data, limits).ok()
}

fn conflict(name: &[u8]) -> ServiceError {
    ServiceError::rejected(format!(
        "ERR flow value {} already exists with different digest; use OVERRIDE true",
        String::from_utf8_lossy(name)
    ))
}

/// Writes named values into `meta` with Elixir's rules; `version` of a new
/// or replaced value is its previous version plus one.
pub(super) fn write_values<'a>(
    meta: &mut FlowMeta,
    values: impl IntoIterator<Item = (&'a Vec<u8>, &'a Vec<u8>)>,
    overrides: &[Vec<u8>],
) -> Result<(), ServiceError> {
    for (name, bytes) in values {
        let version = match meta.values.get(name) {
            Some(existing) if existing.bytes == *bytes => continue,
            Some(_) if !overrides.contains(name) => return Err(conflict(name)),
            Some(existing) => existing.version.saturating_add(1),
            None => 1,
        };
        meta.values.insert(
            name.clone(),
            NamedValue {
                version,
                bytes: bytes.clone(),
            },
        );
    }
    Ok(())
}

impl ShardState {
    /// `FLOW.VALUE.PUT` on the shard owning its Flow or reference.
    pub(crate) fn value_put(
        &mut self,
        wall: u64,
        put: &ValuePut,
        partition: Partition,
    ) -> Executed {
        let result = match &put.named {
            Some((owner, name)) => self.named_put(wall, put, (owner, name), partition),
            None => Ok(self.shared_put(wall, put, &partition)),
        };
        match result {
            Ok((reply, logged)) => (Ok(reply), logged),
            Err(error) => (Err(error), None),
        }
    }

    fn named_put(
        &mut self,
        wall: u64,
        put: &ValuePut,
        (owner, name): (&[u8], &[u8]),
        partition: Partition,
    ) -> Result<(Term, Option<Logged>), ServiceError> {
        let now = put.now_ms.unwrap_or(wall);
        let key: Key = (partition, owner.to_vec());
        let defaults = self.defaults();
        let entry = self
            .records
            .get(&key)
            .ok_or_else(|| ServiceError::rejected("ERR flow not found".to_owned()))?;
        let mut meta = entry.meta.as_deref().cloned().unwrap_or(defaults);
        let before = meta.values.get(name).map(|value| value.version);
        let overrides = if put.override_value {
            vec![name.to_vec()]
        } else {
            Vec::new()
        };
        write_values(&mut meta, [(&name.to_vec(), &put.value)], &overrides)?;
        let version = meta.values.get(name).map_or(1, |value| value.version);
        let stored = before != Some(version);
        let reply = |partition: &Partition| {
            map(vec![
                ("name", Term::Bin(name.to_vec())),
                ("version", Term::int(version)),
                (
                    "ref",
                    Term::Bin(keys::named_value_ref(partition, owner, name, version)),
                ),
                ("partition_key", Term::Bin(partition.name())),
                ("owner_flow_id", Term::Bin(owner.to_vec())),
                ("stored", Term::Bool(stored)),
                ("created", Term::Bool(stored && before.is_none())),
            ])
        };
        if !stored {
            return Ok((reply(&key.0), None));
        }
        let priority = entry.priority;
        let record = touched(&entry.record, now).ok_or(ServiceError::Unavailable)?;
        let mut event = HistoryEvent::of(&record, priority, &meta, &key.0);
        event.kind = HistoryKind::ValuePut;
        self.put(
            key.clone(),
            (record, priority, Some(Box::new(meta))),
            Some(&event),
        );
        let logged = self.after_image(&key)?;
        Ok((reply(&key.0), Some(logged)))
    }

    fn shared_put(
        &mut self,
        wall: u64,
        put: &ValuePut,
        partition: &Partition,
    ) -> (Term, Option<Logged>) {
        let Some(reference) = put.shared_ref.clone() else {
            return (Term::Nil, None);
        };
        let now = put.now_ms.unwrap_or(wall);
        let expires_at = put.ttl_ms.map(|ttl| now.saturating_add(ttl));
        self.store_shared(
            partition.clone(),
            reference.clone(),
            put.value.clone(),
            expires_at,
        );
        let reply = map(vec![
            ("ref", Term::Bin(reference.clone())),
            (
                "partition_key",
                Term::optional_bin(put.partition_key.as_deref()),
            ),
        ]);
        let logged = Logged::Shared {
            partition: partition.clone(),
            reference,
            bytes: put.value.clone(),
            expires_at,
        };
        (reply, Some(logged))
    }

    /// Stores a shared value, live or from the log.
    pub(crate) fn store_shared(
        &mut self,
        partition: Partition,
        reference: Vec<u8>,
        bytes: Vec<u8>,
        expires_at: Option<u64>,
    ) {
        self.shared.insert(
            reference,
            Shared {
                partition,
                bytes,
                expires_at,
            },
        );
    }

    /// The shared values for a snapshot image, unexpired at `now`.
    pub(crate) fn shared_values(&self, now: u64) -> Vec<(&Vec<u8>, &Shared)> {
        let mut values: Vec<(&Vec<u8>, &Shared)> = self
            .shared
            .iter()
            .filter(|(_, value)| value.expires_at.is_none_or(|at| at > now))
            .collect();
        values.sort_unstable_by(|left, right| left.0.cmp(right.0));
        values
    }

    /// The partition of this shard whose hash tag is `tag`.
    fn partition_for_tag(&mut self, tag: &[u8]) -> Option<Partition> {
        if let Some(bucket) = tag.strip_prefix(b"fa:") {
            return std::str::from_utf8(bucket)
                .ok()?
                .parse()
                .ok()
                .map(Partition::Auto);
        }
        let tag = std::str::from_utf8(tag).ok()?;
        if !self.tags.contains_key(tag) && self.tags_at != self.next_sequence {
            self.rebuild_tags();
        }
        self.tags.get(tag).cloned()
    }

    /// Indexes the hash tags of every named partition this shard holds.
    fn rebuild_tags(&mut self) {
        self.tags_at = self.next_sequence;
        let named = self
            .records
            .keys()
            .map(|(partition, _)| partition)
            .filter(|partition| matches!(partition, Partition::Named(_)));
        for partition in named {
            if !self.tags.values().any(|known| known == partition) {
                self.tags.insert(partition.tag(), partition.clone());
            }
        }
    }

    fn value_bytes(&mut self, now: u64, reference: &[u8]) -> Option<Vec<u8>> {
        let parsed = value::parse_ref(reference)?;
        if let ValueRef::Shared = parsed.target {
            let shared = self.shared.get(reference)?;
            return shared
                .expires_at
                .is_none_or(|at| at > now)
                .then(|| shared.bytes.clone());
        }
        let partition = self.partition_for_tag(&parsed.tag)?;
        let defaults = self.defaults();
        match parsed.target {
            ValueRef::Flow { kind, id, version } => {
                let entry = self.records.get(&(partition, id))?;
                let meta = entry.meta.as_deref().unwrap_or(&defaults);
                let record = &entry.record;
                let (bytes, stored) = match kind {
                    b'p' => (record.payload(), meta.payload_version),
                    b'r' => (record.result(), meta.result_version),
                    _ => (record.error(), meta.error_version),
                };
                (stored == version)
                    .then(|| bytes.map(<[u8]>::to_vec))
                    .flatten()
            }
            ValueRef::Named {
                owner,
                name,
                version,
            } => {
                let entry = self.records.get(&(partition, owner))?;
                let value = entry.meta.as_deref()?.values.get(&name)?;
                (value.version == version).then(|| value.bytes.clone())
            }
            ValueRef::Shared => None,
        }
    }

    /// An error stored as a term (an active-time timeout's), decoded.
    fn term_value(&mut self, reference: &[u8]) -> Option<Term> {
        let parsed = value::parse_ref(reference)?;
        let ValueRef::Flow {
            kind: b'e',
            id,
            version,
        } = parsed.target
        else {
            return None;
        };
        let partition = self.partition_for_tag(&parsed.tag)?;
        let entry = self.records.get(&(partition, id))?;
        let meta = entry.meta.as_deref()?;
        if !meta.error_term || meta.error_version != version {
            return None;
        }
        match Term::decode(entry.record.error()?) {
            Some((term, [])) => Some(term),
            _ => None,
        }
    }

    /// `FLOW.VALUE.MGET` for the references this shard owns.
    pub(crate) fn read_values(
        &mut self,
        now: u64,
        refs: Vec<(usize, Vec<u8>)>,
        max_bytes: u64,
    ) -> Vec<(usize, Term)> {
        refs.into_iter()
            .map(|(position, reference)| {
                if let Some(term) = self.term_value(&reference) {
                    return (position, term);
                }
                let bytes = self.value_bytes(now, &reference);
                (
                    position,
                    value::read_term(&reference, bytes.as_deref(), max_bytes),
                )
            })
            .collect()
    }
}
