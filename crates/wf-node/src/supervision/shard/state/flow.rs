//! Flow commands beyond the queue fast path, decided on the owning shard
//! (ADR-014).
//!
//! Each command reads or changes one Flow. A change goes through the domain
//! reducer for the lifecycle record and is logged as the Flow's whole
//! after-image, so replay stores it without deciding anything again.

use std::collections::BTreeMap;

use wf_application::commands::flow::create::CreateFlow;
use wf_application::commands::flow::get::GetFlow;
use wf_application::commands::flow::meta::{self, StateMeta};
use wf_application::commands::flow::record::{FlowMeta, Include, NamedValue, View};
use wf_application::commands::flow::{FlowCommand, Term};
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::workflow::{self, CommittedCommand, Create, Operation};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

use super::super::command::Logged;
use super::super::image;
use super::{Entry, Key, ShardState};

/// A command's reply and the change to log, if any.
pub(crate) type Executed = (Result<Term, ServiceError>, Option<Logged>);

fn ok() -> Term {
    Term::text("OK")
}

impl ShardState {
    /// Decides one Flow command at committed time `now`; a command that
    /// carries the client's clock uses that instead, as Elixir does.
    pub(crate) fn execute(
        &mut self,
        now: u64,
        command: FlowCommand,
        partition: Partition,
    ) -> Executed {
        match command {
            FlowCommand::Create(create) => {
                let now = create.now_ms.unwrap_or(now);
                self.create_flow(now, *create, partition)
            }
            FlowCommand::Get(get) => (Ok(self.get_flow(&get, partition)), None),
            FlowCommand::Mutate(mutation) => self.mutate(now, &mutation, partition),
            // Batches are split over shards before they reach one.
            FlowCommand::Many(_) => (Err(ServiceError::Unavailable), None),
        }
    }

    fn view<'a>(key: &'a Key, entry: &'a Entry, defaults: &'a FlowMeta) -> View<'a> {
        View {
            record: &entry.record,
            partition: &key.0,
            priority: entry.priority,
            meta: entry.meta.as_deref().unwrap_or(defaults),
        }
    }

    /// The default fields of a Flow made by the queue fast path.
    pub(super) fn defaults(&self) -> FlowMeta {
        FlowMeta {
            retention_ttl_ms: self.retention_ms,
            ..FlowMeta::default()
        }
    }

    /// The reply map of the Flow under `key`, if stored.
    pub(super) fn record_term(&self, key: &Key, include: &Include) -> Option<Term> {
        let defaults = self.defaults();
        let (key, entry) = self.records.get_key_value(key)?;
        Some(Self::view(key, entry, &defaults).term(include))
    }

    fn get_flow(&self, get: &GetFlow, partition: Partition) -> Term {
        let key = (partition, get.id.clone());
        let defaults = self.defaults();
        self.records
            .get_key_value(&key)
            .map_or(Term::Nil, |(key, entry)| {
                Self::view(key, entry, &defaults).term(&get.include)
            })
    }

    /// Logs the stored Flow under `key` as its after-image.
    pub(super) fn after_image(&self, key: &Key) -> Result<Logged, ServiceError> {
        let entry = self.records.get(key).ok_or(ServiceError::Unavailable)?;
        let image = image::encode_one(
            &key.0,
            &key.1,
            entry.priority,
            (&entry.record, entry.meta.as_deref()),
        )
        .ok_or(ServiceError::Unavailable)?;
        Ok(Logged::Put {
            partition: key.0.clone(),
            image,
        })
    }

    fn create_flow(&mut self, now: u64, create: CreateFlow, partition: Partition) -> Executed {
        let key = (partition, create.id.clone());
        if let Some(existing) = self.records.get(&key) {
            return (self.duplicate(&key, existing, &create), None);
        }
        let meta = match self.new_meta(&create) {
            Ok(meta) => meta,
            Err(error) => return (Err(error), None),
        };
        let command = CommittedCommand {
            id: match RecordId::new(create.id.clone()) {
                Ok(id) => id,
                Err(_) => return (Err(ServiceError::InvalidId), None),
            },
            now: TimestampMillis::new(now),
            operation: Operation::Create(Create {
                workflow_type: create.flow_type,
                state: create.state,
                run_at: create.run_at_ms.map(TimestampMillis::new),
                payload: create.payload,
            }),
        };
        let write = match workflow::reduce(None, &command) {
            Ok(decision) => decision.into_parts().1,
            Err(error) => return (Err(ServiceError::Domain(error)), None),
        };
        let Some(write) = write else {
            return (Err(ServiceError::Unavailable), None);
        };
        self.put(
            key.clone(),
            write.record,
            create.priority,
            Some(Box::new(meta)),
        );
        match self.after_image(&key) {
            Ok(logged) => (Ok(ok()), Some(logged)),
            Err(error) => (Err(error), None),
        }
    }

    fn new_meta(&self, create: &CreateFlow) -> Result<FlowMeta, ServiceError> {
        let state_meta =
            meta::merge_state_meta(&StateMeta::new(), &create.state, &create.state_meta)?;
        let values: BTreeMap<Vec<u8>, NamedValue> = create
            .values
            .iter()
            .map(|(name, bytes)| {
                (
                    name.clone(),
                    NamedValue {
                        version: 1,
                        bytes: bytes.clone(),
                    },
                )
            })
            .collect();
        let defaults = self.defaults();
        Ok(FlowMeta {
            parent_flow_id: create.parent_flow_id.clone(),
            root_flow_id: create
                .root_flow_id
                .clone()
                .filter(|root| *root != create.id),
            correlation_id: create.correlation_id.clone(),
            retention_ttl_ms: create.retention_ttl_ms.unwrap_or(defaults.retention_ttl_ms),
            max_active_ms: create.max_active_ms,
            history_hot_max_events: create
                .history_hot_max_events
                .unwrap_or(defaults.history_hot_max_events),
            history_max_events: create
                .history_max_events
                .unwrap_or(defaults.history_max_events),
            attributes: create.attributes.clone(),
            state_meta,
            values,
            ..defaults
        })
    }

    /// Elixir's answer to a create of an existing Flow: `OK` for an
    /// idempotent retry that matches it, otherwise a conflict.
    fn duplicate(
        &self,
        key: &Key,
        existing: &Entry,
        create: &CreateFlow,
    ) -> Result<Term, ServiceError> {
        if !create.idempotent {
            return Err(ServiceError::rejected("ERR flow already exists".to_owned()));
        }
        let defaults = self.defaults();
        let meta = existing.meta.as_deref().unwrap_or(&defaults);
        let record = &existing.record;
        let root = create
            .root_flow_id
            .clone()
            .unwrap_or_else(|| create.id.clone());
        let identity = record.workflow_type() == create.flow_type.as_slice()
            && meta.parent_flow_id == create.parent_flow_id
            && meta.root_flow_id.clone().unwrap_or_else(|| key.1.clone()) == root
            && meta.correlation_id == create.correlation_id;
        let payload = record.payload() == create.payload.as_deref();
        let matches = if record.version() > 1 {
            // Changed since: compare only what no later command changes, and
            // the payload while the created one is still in place.
            identity && (meta.payload_version != 1 || payload)
        } else {
            let fresh = self.new_meta(create)?;
            identity
                && payload
                && record.state() == create.state.as_slice()
                && existing.priority == create.priority
                && meta.retention_ttl_ms == fresh.retention_ttl_ms
                && meta.max_active_ms == fresh.max_active_ms
                && meta.history_hot_max_events == fresh.history_hot_max_events
                && meta.history_max_events == fresh.history_max_events
        };
        if matches {
            Ok(ok())
        } else {
            Err(ServiceError::rejected(
                "ERR flow idempotency conflict".to_owned(),
            ))
        }
    }
}
