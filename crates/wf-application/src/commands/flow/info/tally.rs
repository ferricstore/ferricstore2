//! Counting one shard's Flows for a read, joining the shards' counts and
//! answering as the Elixir server does.

use std::collections::BTreeMap;

use super::super::meta::{self, Attributes};
use super::super::term::{Term, map};
use super::inspect::{index_value, inspect};
use super::{INFO_STATES, Kind, Read, Scope, StateFilter};

/// One Flow as a read sees it.
#[derive(Clone, Copy, Debug)]
pub struct Seen<'a> {
    /// Its state.
    pub state: &'a [u8],
    /// Whether a worker holds its lease.
    pub leased: bool,
    /// A terminal Flow whose retention has ended; attribute reads skip it.
    pub expired: bool,
    /// Its attributes.
    pub attributes: &'a Attributes,
    /// The attribute names its type's policy indexed when it was last
    /// written: the names a state `any` read sees.
    pub indexed: &'a [String],
}

/// One shard's part of a read's answer; parts join with [`Tally::merge`].
#[derive(Clone, Debug, PartialEq)]
pub enum Tally {
    /// INFO: Flows per [`INFO_STATES`] entry and leased Flows.
    Info {
        /// Counts in [`INFO_STATES`] order.
        states: [u64; 5],
        /// Leased Flows.
        inflight: u64,
    },
    /// A STATS state count.
    Count(u64),
    /// A STATS attribute count: per filter candidate, in request order, the
    /// Flows holding it and those of them matching every filter.
    Candidates(Vec<(u64, u64)>),
    /// ATTRIBUTES: values held per indexed name.
    Names(BTreeMap<String, u64>),
    /// `ATTRIBUTE_VALUES`: Flows per value, by the value's `inspect/1` text.
    Values(BTreeMap<Vec<u8>, (Term, u64)>),
}

/// The values an attribute holds: each element of a list, or the value.
fn held(value: &Term) -> &[Term] {
    match value {
        Term::List(items) => items,
        value => std::slice::from_ref(value),
    }
}

/// `Attributes.matches?/2`: every filter's value is held; a list filter
/// needs all of its values.
fn matches(attributes: &Attributes, filters: &Attributes) -> bool {
    filters.iter().all(|(name, expected)| {
        attributes
            .get(name)
            .is_some_and(|value| match (value, expected) {
                (Term::List(values), Term::List(wanted)) => {
                    wanted.iter().all(|item| values.contains(item))
                }
                (Term::List(values), expected) => values.contains(expected),
                (value, expected) => value == expected,
            })
    })
}

/// The `(name, value)` filters Elixir picks a candidate index from.
fn candidates(filters: &Attributes) -> Vec<(&str, &Term)> {
    filters
        .iter()
        .flat_map(|(name, value)| held(value).iter().map(move |item| (name.as_str(), item)))
        .collect()
}

fn add(count: &mut u64) {
    *count = count.saturating_add(1);
}

/// Counts one shard's Flows for `read`.
pub struct Counter<'a> {
    read: &'a Read,
    /// The type's policy-indexed attribute names.
    names: &'a [String],
    any: bool,
    tally: Tally,
}

impl<'a> Counter<'a> {
    /// A counter for `read`; `names` are the attribute names the type's
    /// current policy indexes.
    #[must_use]
    pub fn new(read: &'a Read, names: &'a [String]) -> Self {
        let (tally, state) = match &read.kind {
            Kind::Info => (
                Tally::Info {
                    states: [0; 5],
                    inflight: 0,
                },
                None,
            ),
            Kind::Stats { state, filters } if filters.is_empty() => (Tally::Count(0), Some(state)),
            Kind::Stats { state, filters } => (
                Tally::Candidates(vec![(0, 0); candidates(filters).len()]),
                Some(state),
            ),
            Kind::Names { state, .. } => (Tally::Names(BTreeMap::new()), Some(state)),
            Kind::Values { state, .. } => (Tally::Values(BTreeMap::new()), Some(state)),
        };
        Self {
            read,
            names,
            any: state == Some(&StateFilter::Any),
            tally,
        }
    }

    /// Whether `flow` has `name` in the index the read uses: every
    /// attribute in one state, only policy-indexed ones across states.
    fn visible(&self, flow: &Seen<'_>, name: &str) -> bool {
        !self.any || flow.indexed.iter().any(|indexed| indexed == name)
    }

    /// Counts `flows` Flows in `state`, `leased` of them leased, without
    /// visiting them: enough for INFO and a STATS state count.
    pub fn add_count(&mut self, state: &[u8], flows: u64, leased: u64) {
        match &mut self.tally {
            Tally::Info { states, inflight } => {
                if let Some(slot) = INFO_STATES
                    .iter()
                    .position(|known| *known == state)
                    .and_then(|index| states.get_mut(index))
                {
                    *slot = slot.saturating_add(flows);
                }
                *inflight = inflight.saturating_add(leased);
            }
            Tally::Count(count) => *count = count.saturating_add(flows),
            _ => {}
        }
    }

    /// Counts one Flow the read visits.
    pub fn add(&mut self, flow: &Seen<'_>) {
        if let Tally::Info { states, inflight } = &mut self.tally {
            if let Some(slot) = INFO_STATES
                .iter()
                .position(|state| *state == flow.state)
                .and_then(|index| states.get_mut(index))
            {
                add(slot);
            }
            if flow.leased {
                add(inflight);
            }
            return;
        }
        if let Tally::Count(count) = &mut self.tally {
            add(count);
            return;
        }
        if flow.expired {
            return;
        }
        match &self.read.kind {
            Kind::Stats { filters, .. } => self.add_candidates(flow, filters),
            Kind::Names { .. } => self.add_names(flow),
            Kind::Values { attribute, .. } => self.add_values(flow, attribute),
            Kind::Info => {}
        }
    }

    fn add_candidates(&mut self, flow: &Seen<'_>, filters: &Attributes) {
        let matched = matches(flow.attributes, filters);
        let holds: Vec<bool> = candidates(filters)
            .into_iter()
            .map(|(name, value)| {
                self.visible(flow, name)
                    && flow
                        .attributes
                        .get(name)
                        .is_some_and(|held_value| held(held_value).contains(value))
            })
            .collect();
        if let Tally::Candidates(counts) = &mut self.tally {
            for ((found, hits), holds) in counts.iter_mut().zip(holds) {
                *found = found.saturating_add(u64::from(holds));
                *hits = hits.saturating_add(u64::from(holds && matched));
            }
        }
    }

    fn add_names(&mut self, flow: &Seen<'_>) {
        let names: Vec<(String, u64)> = self
            .names
            .iter()
            .filter(|name| self.visible(flow, name))
            .filter_map(|name| {
                let value = flow.attributes.get(name)?;
                Some((name.clone(), u64::try_from(held(value).len()).ok()?))
            })
            .collect();
        if let Tally::Names(counts) = &mut self.tally {
            for (name, values) in names {
                let count = counts.entry(name).or_insert(0);
                *count = count.saturating_add(values);
            }
        }
    }

    fn add_values(&mut self, flow: &Seen<'_>, attribute: &str) {
        if !self.names.iter().any(|name| name == attribute) || !self.visible(flow, attribute) {
            return;
        }
        let Some(value) = flow.attributes.get(attribute) else {
            return;
        };
        if let Tally::Values(counts) = &mut self.tally {
            for item in held(value) {
                let (_, count) = counts
                    .entry(inspect(item))
                    .or_insert_with(|| (item.clone(), 0));
                add(count);
            }
        }
    }

    /// The shard's part.
    #[must_use]
    pub fn finish(self) -> Tally {
        self.tally
    }
}

fn sum(left: u64, right: u64) -> u64 {
    left.saturating_add(right)
}

impl Tally {
    /// Both parts together.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        match (self, other) {
            (
                Self::Info { states, inflight },
                Self::Info {
                    states: more,
                    inflight: extra,
                },
            ) => {
                let mut joined = states;
                for (slot, add) in joined.iter_mut().zip(more) {
                    *slot = sum(*slot, add);
                }
                Self::Info {
                    states: joined,
                    inflight: sum(inflight, extra),
                }
            }
            (Self::Count(left), Self::Count(right)) => Self::Count(sum(left, right)),
            (Self::Candidates(left), Self::Candidates(right)) => Self::Candidates(
                left.into_iter()
                    .zip(right)
                    .map(|((found, hits), (more, extra))| (sum(found, more), sum(hits, extra)))
                    .collect(),
            ),
            (Self::Names(mut left), Self::Names(right)) => {
                for (name, count) in right {
                    let slot = left.entry(name).or_insert(0);
                    *slot = sum(*slot, count);
                }
                Self::Names(left)
            }
            (Self::Values(mut left), Self::Values(right)) => {
                for (key, (value, count)) in right {
                    let slot = left.entry(key).or_insert((value, 0));
                    slot.1 = sum(slot.1, count);
                }
                Self::Values(left)
            }
            (left, _) => left,
        }
    }
}

/// `select_scored_candidate/1`: the candidate fewest Flows hold, ties
/// broken by name and indexed value; its matching Flows are the count.
fn selected(filters: &Attributes, counts: &[(u64, u64)]) -> u64 {
    candidates(filters)
        .into_iter()
        .zip(counts)
        .min_by(
            |((name, value), (found, _)), ((other, other_value), (more, _))| {
                (found, *name, index_value(value)).cmp(&(more, *other, index_value(other_value)))
            },
        )
        .map_or(0, |(_, (_, hits))| *hits)
}

fn state_term(state: &StateFilter) -> Term {
    match state {
        StateFilter::Any => Term::text("any"),
        StateFilter::Only(state) => Term::Bin(state.clone()),
    }
}

/// The reply to `read` from all shards' joined parts.
#[must_use]
pub fn reply(read: &Read, tally: Tally) -> Term {
    let flow_type = Term::Bin(read.flow_type.clone());
    match (&read.kind, tally) {
        (Kind::Info, Tally::Info { states, inflight }) => {
            let [queued, running, completed, failed, cancelled] = states;
            let partition_key = match &read.scope {
                Scope::Auto => Term::Nil,
                Scope::Partition(partition) => Term::Bin(partition.name()),
            };
            map(vec![
                ("cancelled", Term::int(cancelled)),
                ("completed", Term::int(completed)),
                ("failed", Term::int(failed)),
                ("inflight", Term::int(inflight)),
                ("partition_key", partition_key),
                ("queued", Term::int(queued)),
                ("running", Term::int(running)),
                ("type", flow_type),
            ])
        }
        (Kind::Stats { state, filters }, tally) => {
            let count = match tally {
                Tally::Count(count) => count,
                Tally::Candidates(counts) => selected(filters, &counts),
                _ => 0,
            };
            map(vec![
                ("attributes", meta::attributes_term(filters)),
                ("count", Term::int(count)),
                ("state", state_term(state)),
                ("type", flow_type),
            ])
        }
        (Kind::Names { count, .. }, Tally::Names(names)) => {
            let mut entries: Vec<(String, u64)> =
                names.into_iter().filter(|(_, held)| *held > 0).collect();
            entries.sort_by(|(name, held), (other, more)| more.cmp(held).then(name.cmp(other)));
            Term::List(
                entries
                    .into_iter()
                    .take(*count)
                    .map(|(name, held)| {
                        map(vec![
                            ("count", Term::int(held)),
                            ("name", Term::text(&name)),
                        ])
                    })
                    .collect(),
            )
        }
        (Kind::Values { count, .. }, Tally::Values(values)) => {
            let mut entries: Vec<(Vec<u8>, (Term, u64))> = values.into_iter().collect();
            entries.sort_by(|(key, (_, held)), (other, (_, more))| {
                more.cmp(held).then(key.cmp(other))
            });
            Term::List(
                entries
                    .into_iter()
                    .take(*count)
                    .map(|(_, (value, held))| {
                        map(vec![("count", Term::int(held)), ("value", value)])
                    })
                    .collect(),
            )
        }
        _ => Term::List(Vec::new()),
    }
}
