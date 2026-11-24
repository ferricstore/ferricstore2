//! Flow commands beyond the queue fast path, decided on the owning shard
//! (ADR-014).
//!
//! Each command reads or changes one Flow. A change goes through the domain
//! reducer for the lifecycle record and is logged as the Flow's whole
//! after-image, so replay stores it without deciding anything again.

use std::collections::BTreeMap;

use wf_application::commands::flow::create::CreateFlow;
use wf_application::commands::flow::create::MaxActive;
use wf_application::commands::flow::get::GetFlow;
use wf_application::commands::flow::history::{self, HistoryQuery};
use wf_application::commands::flow::meta::{self, StateMeta};
use wf_application::commands::flow::policy;
use wf_application::commands::flow::record::{FlowMeta, Include, NamedValue, View};
use wf_application::commands::flow::{FlowCommand, Term};
use wf_application::commands::queue::{Partition, ServiceError};
use wf_domain::workflow::{self, CommittedCommand, Create, Operation};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

use super::super::command::Logged;
use super::super::image::{self, Kept};
use super::{Entry, Key, Parts, ShardState, flow_key};

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
        // A finished Flow on disk is loaded for the round (ADR-016).
        if let Some((id, _)) = command.target()
            && let Err(error) = self.warm(&(partition.clone(), id.to_vec()))
        {
            return (Err(error), None);
        }
        match command {
            FlowCommand::Create(create) => self.create_flow(now, *create, partition),
            FlowCommand::Get(get) => (Ok(self.get_flow(now, &get, partition)), None),
            FlowCommand::History(query) => (Ok(self.history_term(&query, partition)), None),
            FlowCommand::Mutate(mutation) => self.mutate(now, &mutation, partition),
            FlowCommand::ValuePut(put) => self.value_put(now, &put, partition),
            FlowCommand::StartAndClaim(start) => self.start_and_claim(now, &start, partition),
            FlowCommand::Signal(signal) => self.signal(now, &signal, partition),
            FlowCommand::SpawnChildren(spawn) => self.spawn_children(now, &spawn, partition),
            FlowCommand::Rewind(rewind) => self.rewind(now, &rewind, partition),
            FlowCommand::Query(query) => (self.query(&query, &partition, now), None),
            FlowCommand::Effect(command) => self.effect(now, &command, partition),
            FlowCommand::Governance(command) => self.governance(now, &command),
            // Batches and reads by reference are split over shards before
            // they reach one; policies have requests of their own.
            FlowCommand::Many(_)
            | FlowCommand::CreateMany(_)
            | FlowCommand::ValueMget(_)
            | FlowCommand::RunSteps(_)
            | FlowCommand::PolicySet(_)
            | FlowCommand::PolicyGet(_)
            | FlowCommand::RetentionCleanup(_)
            | FlowCommand::Info(_)
            | FlowCommand::QueryIndexes(_)
            | FlowCommand::Schedule(_) => (Err(ServiceError::Unavailable), None),
        }
    }

    /// The reply view of a Flow; its indexed state metadata key is its
    /// type's policy's now (audit gap G31).
    fn view<'a>(&'a self, key: &'a Key, entry: &'a Entry, defaults: &'a FlowMeta) -> View<'a> {
        View {
            record: &entry.record,
            partition: &key.0,
            priority: entry.priority,
            meta: entry.meta.as_deref().unwrap_or(defaults),
            indexed_state_meta: self
                .policy(entry.record.workflow_type())
                .and_then(|installed| installed.policy.indexed_state_meta.as_deref()),
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
    pub(super) fn record_term(&self, key: &Parts, include: &Include) -> Option<Term> {
        let defaults = self.defaults();
        // A query row may name a finished Flow on disk (ADR-016).
        let (key, entry) = self.seen_kv(key)?;
        Some(self.view(key, &entry, &defaults).term(include))
    }

    /// `FLOW.HISTORY`: the selected events, or none for a missing Flow.
    fn history_term(&self, query: &HistoryQuery, partition: Partition) -> Term {
        let key = (partition, query.id.clone());
        let defaults = self.defaults();
        let Some((key, entry)) = self.hot_kv(&key) else {
            return Term::List(Vec::new());
        };
        let meta = entry.meta.as_deref().unwrap_or(&defaults);
        let events = entry.history.events();
        Term::List(
            query
                .select(&events)
                .into_iter()
                .map(|event| {
                    let term = history::event_term(event, &entry.record, &key.0, meta);
                    if query.values {
                        super::value::with_values(
                            term,
                            &entry.record,
                            meta,
                            (event.refs, query.max_bytes),
                        )
                    } else {
                        term
                    }
                })
                .collect(),
        )
    }

    fn get_flow(&mut self, now: u64, get: &GetFlow, partition: Partition) -> Term {
        let key = (partition, get.id.clone());
        let defaults = self.defaults();
        let Some((stored, entry)) = self.hot_kv(&key) else {
            return Term::Nil;
        };
        let term = self.view(stored, entry, &defaults).term(&get.include);
        let attached = super::value::requested_refs(entry, &get.include.values);
        let term = self.with_attached(term, now, attached);
        self.with_payload_ref(term, now, &key, &get.include)
    }

    /// Logs the stored Flow under `key` as its after-image.
    pub(super) fn after_image(&mut self, key: &Key) -> Result<Logged, ServiceError> {
        self.after_image_events(key, 1)
    }

    /// The after-image of `key` carrying the `count` events its change
    /// appended and the value versions it kept, so replay keeps what the
    /// record alone does not show; those versions are logged from now on.
    pub(super) fn after_image_events(
        &mut self,
        key: &Key,
        count: usize,
    ) -> Result<Logged, ServiceError> {
        let entry = self.hot(key).ok_or(ServiceError::Unavailable)?;
        let latest = if count == 1 {
            entry.history.last().into_iter().collect()
        } else {
            entry.history.last_n(count)
        };
        let image = image::encode_one(
            &key.0,
            &key.1,
            entry.priority,
            (
                &entry.record,
                entry.meta.as_deref(),
                Some(latest.as_slice()),
            ),
            Kept::Unlogged,
        )
        .ok_or(ServiceError::Unavailable)?;
        if let Some(meta) = self
            .hot_mut(key)
            .and_then(|entry| entry.meta.as_deref_mut())
        {
            meta.retained_unlogged.clear();
        }
        Ok(Logged::Put {
            partition: key.0.clone(),
            image,
        })
    }

    /// Creates a Flow at its command time (the client's `now_ms`, else
    /// `wall`, the shard's clock).
    pub(super) fn create_flow(
        &mut self,
        wall: u64,
        create: CreateFlow,
        partition: Partition,
    ) -> Executed {
        let now = create.now_ms.unwrap_or(wall);
        let key = flow_key(partition, create.id.clone());
        if let Some(existing) = self.hot(&key) {
            return (self.duplicate(&key, existing, &create), None);
        }
        let explicit = Some(create.partition_key.is_some());
        let entry = (create.flow_type.as_slice(), create.state.as_slice());
        let mut meta = match self
            .fifo_guard(entry, false, create.priority, explicit)
            .and_then(|()| self.new_meta(&create))
        {
            Ok(meta) => meta,
            Err(error) => return (Err(error), None),
        };
        // A Flow created finished keeps its retention from the command's
        // time, never before the shard's clock, as one that finishes later
        // does (audit gaps G13, G40).
        if super::children::terminal(&create.state) {
            let start = now.max(wall);
            meta.terminal_until = Some(start.saturating_add(meta.retention_ttl_ms));
        }
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
            (write.record, create.priority, Some(Box::new(meta))),
            None,
        );
        match self.after_image(&key) {
            Ok(logged) => (Ok(ok()), Some(logged)),
            Err(error) => (Err(error), None),
        }
    }

    pub(super) fn new_meta(&self, create: &CreateFlow) -> Result<FlowMeta, ServiceError> {
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
        // Elixir's `flow_lifecycle_for_create/2`: the type's policy for the
        // create state, under the create's own options.
        let policy = self
            .policy(&create.flow_type)
            .map(|installed| &installed.policy);
        let limits = policy::lifecycle(
            policy,
            Some(&create.state),
            self.lifecycle_defaults(),
            [
                create.retention_ttl_ms,
                create.history_hot_max_events,
                create.history_max_events,
            ],
        );
        // `resolve_max_active_ms/2`: `infinity` opts out of any limit.
        let max_active_ms = match create.max_active_ms {
            Some(MaxActive::Infinity) => None,
            Some(limit) => Some(limit),
            None => policy
                .and_then(|policy| policy.max_active_ms)
                .map(MaxActive::Ms),
        };
        Ok(FlowMeta {
            parent_flow_id: create.parent_flow_id.clone(),
            root_flow_id: create
                .root_flow_id
                .clone()
                .filter(|root| *root != create.id),
            correlation_id: create.correlation_id.clone(),
            retention_ttl_ms: limits.ttl_ms,
            max_active_ms,
            history_hot_max_events: limits.history_hot_max_events,
            history_max_events: limits.history_max_events,
            attributes: create.attributes.clone(),
            state_meta,
            values,
            ..defaults
        })
        .and_then(|mut meta| {
            let partition = Partition::for_new_flow(&create.id, create.partition_key.as_deref());
            super::value::attach(&mut meta, &create.value_refs, &partition)?;
            if let Some(reference) = &create.payload_ref {
                super::value::check_slot(&partition, reference)?;
                meta.payload_ref = Some(reference.clone());
            }
            Ok(meta)
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
