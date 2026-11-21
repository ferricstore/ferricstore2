//! The shard's catalog of Flows by type, state and partition, each ordered
//! by update time (ADR-014 step 11), and the reads it answers: `FLOW.INFO`,
//! `FLOW.STATS`, `FLOW.ATTRIBUTES` and `FLOW.ATTRIBUTE_VALUES`.
//!
//! It plays the part of Elixir's per-state index sorted sets. Every stored
//! Flow is in it, kept in step by [`ShardState::index`] and
//! [`ShardState::unindex`], so it is rebuilt with the records on replay and
//! from snapshots. A state count is the size of its sets; an attribute read
//! visits the Flows of its type and state and leaves out terminal Flows
//! whose retention has ended.

use std::collections::BTreeSet;

use super::Map;

use wf_application::commands::flow::info::{Counter, Kind, Read, Seen, Tally};
use wf_application::commands::flow::meta::Attributes;
use wf_application::commands::queue::Partition;

use super::{Entry, Key, RUNNING, ShardState};

/// One partition's Flows of a type and state, by update time and id.
pub(super) type Members = BTreeSet<(u64, Vec<u8>)>;
/// Type, then state, then partition, to the Flows there.
pub(super) type Catalog = Map<Vec<u8>, Map<Vec<u8>, Map<Partition, Members>>>;

static NO_ATTRIBUTES: Attributes = Attributes::new();

fn place(entry: &Entry) -> (&[u8], &[u8], u64) {
    let record = &entry.record;
    (
        record.workflow_type(),
        record.state(),
        record.updated_at().get(),
    )
}

impl ShardState {
    pub(super) fn catalog_insert(&mut self, key: &Key, entry: &Entry) {
        let (flow_type, state, updated) = place(entry);
        // The usual case finds every level present, so nothing is copied
        // but the member.
        if let Some(members) = self
            .catalog
            .get_mut(flow_type)
            .and_then(|states| states.get_mut(state))
            .and_then(|partitions| partitions.get_mut(&key.0))
        {
            members.insert((updated, key.1.clone()));
            return;
        }
        self.catalog
            .entry(flow_type.to_vec())
            .or_default()
            .entry(state.to_vec())
            .or_default()
            .entry(key.0.clone())
            .or_default()
            .insert((updated, key.1.clone()));
    }

    pub(super) fn catalog_remove(&mut self, key: &Key, entry: &Entry) {
        let (flow_type, state, updated) = place(entry);
        let Some(states) = self.catalog.get_mut(flow_type) else {
            return;
        };
        if let Some(partitions) = states.get_mut(state) {
            let emptied = partitions.get_mut(&key.0).is_some_and(|members| {
                members.remove(&(updated, key.1.clone()));
                members.is_empty()
            });
            if emptied {
                partitions.remove(&key.0);
            }
            if partitions.is_empty() {
                states.remove(state);
            }
        }
        if states.is_empty() {
            self.catalog.remove(flow_type);
        }
    }

    /// The Flows `read` visits: its type's states and partitions in scope.
    fn visited<'a>(
        &'a self,
        read: &'a Read,
    ) -> impl Iterator<Item = (&'a [u8], &'a Partition, &'a Members)> + 'a {
        let wanted = read.states();
        self.catalog
            .get(&read.flow_type)
            .into_iter()
            .flatten()
            .filter(move |(state, _)| {
                wanted
                    .as_ref()
                    .is_none_or(|wanted| wanted.contains(&state.as_slice()))
            })
            .flat_map(move |(state, partitions)| {
                partitions
                    .iter()
                    .filter(|(partition, _)| read.covers(partition))
                    .map(move |(partition, members)| (state.as_slice(), partition, members))
            })
    }

    /// This shard's part of `read` at wall time `now`.
    pub(crate) fn tally(&self, read: &Read, now: u64) -> Tally {
        let names = self
            .policy(&read.flow_type)
            .and_then(|installed| installed.policy.indexed_attributes.as_deref())
            .unwrap_or_default();
        let mut counter = Counter::new(read, names);
        let counts_only = match &read.kind {
            Kind::Info => true,
            Kind::Stats { filters, .. } => filters.is_empty(),
            Kind::Names { .. } | Kind::Values { .. } => false,
        };
        for (state, partition, members) in self.visited(read) {
            if counts_only {
                let leased = self.leased_among(state, partition, members);
                let flows = u64::try_from(members.len()).unwrap_or(u64::MAX);
                counter.add_count(state, flows, leased);
            } else {
                self.visit(&mut counter, (state, partition, members), now);
            }
        }
        counter.finish()
    }

    /// Counts each Flow of one partition's members.
    fn visit(
        &self,
        counter: &mut Counter<'_>,
        (state, partition, members): (&[u8], &Partition, &Members),
        now: u64,
    ) {
        let entries = members
            .iter()
            .filter_map(|(_, id)| self.records.get(&(partition.clone(), id.clone())));
        for entry in entries {
            let meta = entry.meta.as_deref();
            counter.add(&Seen {
                state,
                leased: entry.record.lease().is_some(),
                expired: self
                    .removal_slot(entry)
                    .is_some_and(|(until, _)| until <= now),
                attributes: meta.map_or(&NO_ATTRIBUTES, |meta| &meta.attributes),
                indexed: meta.map_or(&[], |meta| meta.indexed_attributes.as_slice()),
            });
        }
    }

    /// Leased Flows among running members; only running Flows hold leases.
    fn leased_among(&self, state: &[u8], partition: &Partition, members: &Members) -> u64 {
        if state != RUNNING {
            return 0;
        }
        let leased = members
            .iter()
            .filter(|(_, id)| {
                self.records
                    .get(&(partition.clone(), id.clone()))
                    .is_some_and(|entry| entry.record.lease().is_some())
            })
            .count();
        u64::try_from(leased).unwrap_or(u64::MAX)
    }
}
