//! `FLOW.QUERY` on the shard that owns the queried partition (ADR-014 step
//! 12). Candidates come from the catalog, narrowed by the type and state
//! the query fixes, as record rows (the GET map's allowlisted fields); the
//! application evaluates, sorts, pages and answers. A query in update order
//! walks the catalog's sets in that order instead, one update time at a
//! time, until the page is full ([`walk`]). A point read takes the one
//! record; an event query takes the Flow's history.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::iter::Peekable;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use wf_application::commands::flow::history;
use wf_application::commands::flow::query::field::Field;
use wf_application::commands::flow::query::fql::{Direction, Shape, Source};
use wf_application::commands::flow::query::walk::{self, Walk};
use wf_application::commands::flow::query::{Failure, Query, cursor, run};
use wf_application::commands::flow::record::Include;
use wf_application::commands::flow::term::Term;
use wf_application::commands::queue::{Partition, ServiceError};

use super::catalog::Members;
use super::{Key, Parts, ShardState};

/// One catalog set's members in walk order.
type Ordered<'a> = Peekable<Box<dyn Iterator<Item = &'a (u64, Key)> + 'a>>;

/// `members` in `direction`, from update time `start` on when a cursor
/// gives one. `floor` is the least key of the set's partition, so the range
/// takes every member of that update time.
fn ordered<'a>(
    members: &'a Members,
    start: Option<u64>,
    direction: Direction,
    floor: &Key,
) -> Ordered<'a> {
    let walked: Box<dyn Iterator<Item = &'a (u64, Key)> + 'a> = match (direction, start) {
        (Direction::Asc, None) => Box::new(members.iter()),
        (Direction::Asc, Some(time)) => Box::new(members.range((time, Arc::clone(floor))..)),
        // A cursor's update time fits an `i64`, so the bound past it does
        // not saturate.
        (Direction::Desc, Some(time)) => Box::new(
            members
                .range(..(time.saturating_add(1), Arc::clone(floor)))
                .rev(),
        ),
        (Direction::Desc, None) => Box::new(members.iter().rev()),
    };
    walked.peekable()
}

/// The update time the walk takes next: the latest or earliest at the head
/// of a set.
fn next_time(heads: &mut [Ordered<'_>], direction: Direction) -> Option<u64> {
    let times = heads
        .iter_mut()
        .filter_map(|head| head.peek().map(|(time, _)| *time));
    match direction {
        Direction::Asc => times.min(),
        Direction::Desc => times.max(),
    }
}

/// A fresh cursor key: cursors this shard issues open only here and only
/// until it restarts.
pub(super) fn cursor_key() -> cursor::Key {
    let mut digest = Sha256::new();
    for round in 0_u64..4 {
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(round);
        digest.update(hasher.finish().to_be_bytes());
    }
    digest.update(std::process::id().to_be_bytes());
    digest.finalize().into()
}

/// The members of update time `time`, taken from the heads of the sets.
fn taken<'a>(heads: &mut [Ordered<'a>], time: u64) -> Vec<&'a Key> {
    let mut keys = Vec::new();
    for head in heads {
        while let Some((_, key)) = head.next_if(|(updated, _)| *updated == time) {
            keys.push(key);
        }
    }
    keys
}

impl ShardState {
    /// Answers `query` over `partition` at time `now`.
    pub(super) fn query(
        &self,
        query: &Query,
        partition: &Partition,
        now: u64,
    ) -> Result<Term, ServiceError> {
        let answered = match query.source {
            Source::Events => run::events(
                query,
                self.query_events(query, partition),
                &self.cursor_key,
                now,
            ),
            Source::Runs => match walk::native(query) {
                Some(direction) => self.walk(query, partition, direction, now),
                None => run::runs(
                    query,
                    run::Runs {
                        rows: self.query_rows(query, partition),
                        key: self.cursor_key,
                        now_ms: now,
                    },
                ),
            },
        };
        answered.map_err(|failure| ServiceError::Query(Box::new(failure)))
    }

    fn row(&self, key: &Parts) -> Option<Term> {
        self.record_term(key, &Include::default()).map(run::row)
    }

    /// The rows the query could match in `partition`.
    fn query_rows(&self, query: &Query, partition: &Partition) -> Vec<Term> {
        if query.shape == Shape::Point {
            return query
                .run_id()
                .and_then(|id| self.row(&(partition.clone(), id.to_vec())))
                .into_iter()
                .collect();
        }
        self.query_sets(query, partition)
            .into_iter()
            .flatten()
            .filter_map(|(_, key)| self.row(key))
            .collect()
    }

    /// The catalog sets of `partition` in the types and states the query
    /// fixes.
    fn query_sets(&self, query: &Query, partition: &Partition) -> Vec<&Members> {
        let types = query.only(&Field::Type);
        let states = query.only(&Field::State);
        let allowed = |wanted: Option<&Vec<&[u8]>>, value: &[u8]| {
            wanted.is_none_or(|wanted| wanted.contains(&value))
        };
        self.catalog
            .iter()
            .filter(|(flow_type, _)| allowed(types.as_ref(), flow_type))
            .flat_map(|(_, by_state)| by_state.iter())
            .filter(|(state, _)| allowed(states.as_ref(), state))
            .filter_map(|(_, by_partition)| by_partition.get(partition))
            .collect()
    }

    /// Answers `query` in update order: the sets are merged by update time
    /// in `direction`, and each time's rows are offered until the page is
    /// full.
    fn walk(
        &self,
        query: &Query,
        partition: &Partition,
        direction: Direction,
        now: u64,
    ) -> Result<Term, Failure> {
        let mut walk = Walk::new(query, self.cursor_key, now)?;
        let floor: Key = Arc::new((partition.clone(), Vec::new()));
        let mut heads: Vec<Ordered<'_>> = self
            .query_sets(query, partition)
            .into_iter()
            .map(|members| ordered(members, walk.start(), direction, &floor))
            .collect();
        while let Some(time) = next_time(&mut heads, direction) {
            let rows = taken(&mut heads, time)
                .into_iter()
                .filter_map(|key| self.row(key))
                .collect();
            if !walk.offer(rows) {
                break;
            }
        }
        Ok(walk.finish())
    }

    /// The history of the queried Flow, oldest first, as event rows.
    fn query_events(&self, query: &Query, partition: &Partition) -> Vec<run::Event> {
        let defaults = self.defaults();
        let Some(id) = query.run_id() else {
            return Vec::new();
        };
        let Some((key, entry)) = self.seen_kv(&(partition.clone(), id.to_vec())) else {
            return Vec::new();
        };
        let meta = entry.meta.as_deref().unwrap_or(&defaults);
        entry
            .history
            .events()
            .into_iter()
            .filter_map(|event| {
                let order = (event.at, event.version);
                let Term::List(parts) = history::event_term(&event, &entry.record, &key.0, meta)
                else {
                    return None;
                };
                let mut parts = parts.into_iter();
                let (Some(Term::Bin(event_id)), Some(fields)) = (parts.next(), parts.next()) else {
                    return None;
                };
                Some(run::Event {
                    order,
                    row: run::event_row(event_id, fields),
                })
            })
            .collect()
    }
}
