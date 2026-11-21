//! `FLOW.QUERY` on the shard that owns the queried partition (ADR-014 step
//! 12). Candidates come from the catalog, narrowed by the type and state
//! the query fixes, as record rows (the GET map's allowlisted fields); the
//! application evaluates, sorts, pages and answers. A point read takes the
//! one record; an event query takes the Flow's history.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};

use sha2::{Digest, Sha256};
use wf_application::commands::flow::history;
use wf_application::commands::flow::query::field::Field;
use wf_application::commands::flow::query::fql::{Shape, Source};
use wf_application::commands::flow::query::{Query, cursor, run};
use wf_application::commands::flow::record::Include;
use wf_application::commands::flow::term::Term;
use wf_application::commands::queue::{Partition, ServiceError};

use super::{Key, ShardState};

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
            Source::Runs => run::runs(
                query,
                run::Runs {
                    rows: self.query_rows(query, partition),
                    key: self.cursor_key,
                    now_ms: now,
                },
            ),
        };
        answered.map_err(|failure| ServiceError::Query(Box::new(failure)))
    }

    fn row(&self, key: &Key) -> Option<Term> {
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
            .flatten()
            .filter_map(|(_, id)| self.row(&(partition.clone(), id.clone())))
            .collect()
    }

    /// The history of the queried Flow, oldest first, as event rows.
    fn query_events(&self, query: &Query, partition: &Partition) -> Vec<run::Event> {
        let defaults = self.defaults();
        let Some(id) = query.run_id() else {
            return Vec::new();
        };
        let Some((key, entry)) = self
            .records
            .get_key_value(&(partition.clone(), id.to_vec()))
        else {
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
