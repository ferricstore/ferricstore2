//! Native order (`order: native` in a plan). A collection of runs whose
//! order, once the fields its equalities fix are dropped, is update time
//! alone (Elixir's `order_mode/3` over its `flow_runs_tenant_*_updated`
//! indexes) is answered in the order the shard's catalog keeps: the shard
//! offers the rows of one update time at a time, in the walk's direction,
//! and stops once the page is full. A page reads its own rows and the next
//! page's first, and a cursor seeks to its row's update time.
//!
//! Rows keep the sort key [`super::run::runs`] gives them (update time,
//! then the SHA-256 of the id), so a page is the one the bounded top-k
//! would answer and the cursors are alike.

use super::super::term::Term;
use super::Query;
use super::cursor::Key;
use super::error::Failure;
use super::field::Field;
use super::fql::{Direction, Predicate, Shape};
use super::run::{self, Keyed};

/// The direction a shard walks its catalog in to answer `query` in order,
/// or `None` when the query needs the bounded top-k.
#[must_use]
pub fn native(query: &Query) -> Option<Direction> {
    // A collection reads runs (an event collection is history, ordered by
    // event id); points and counts have no order.
    if query.shape != Shape::Collection {
        return None;
    }
    let mut remaining = query
        .order_by
        .iter()
        .filter(|(field, _)| !fixed(query, field));
    match (remaining.next(), remaining.next()) {
        (Some((Field::UpdatedAtMs, direction)), None) => Some(*direction),
        _ => None,
    }
}

/// Whether an equality (one value, `IS NULL` or `IS MISSING`) fixes
/// `field`, so ordering by it orders nothing.
fn fixed(query: &Query, field: &Field) -> bool {
    query.predicates.iter().any(|predicate| match predicate {
        Predicate::Eq(found, _) | Predicate::IsNull(found) | Predicate::IsMissing(found) => {
            found == field
        }
        Predicate::In(found, values) => found == field && values.len() == 1,
        Predicate::Range(..) | Predicate::TimeWindow(..) => false,
    })
}

/// The update time in a cursor's sort key, decoded from where `order_by`
/// puts it: every order field is an integer, so each component is a tag
/// (`0x30` and eight bytes, or one byte for nil and absent), flipped when
/// descending.
fn update_time(after: &[u8], order_by: &[(Field, Direction)]) -> Option<u64> {
    let mut rest = after;
    for (field, direction) in order_by {
        let flip = |byte: u8| {
            if *direction == Direction::Desc {
                !byte
            } else {
                byte
            }
        };
        let (tag, tail) = rest.split_first()?;
        if flip(*tag) != 0x30 {
            if *field == Field::UpdatedAtMs {
                return None;
            }
            rest = tail;
            continue;
        }
        let (bytes, tail) = tail.split_first_chunk::<8>()?;
        if *field == Field::UpdatedAtMs {
            let biased = u64::from_be_bytes(bytes.map(flip));
            return u64::try_from((biased ^ (1 << 63)).cast_signed()).ok();
        }
        rest = tail;
    }
    None
}

/// One page of a query answered in native order.
pub struct Walk<'a> {
    query: &'a Query,
    key: Key,
    now_ms: u64,
    after: Option<Vec<u8>>,
    start: Option<u64>,
    kept: Vec<Keyed>,
    scanned: usize,
}

impl<'a> Walk<'a> {
    /// A walk for `query`, its cursor opened under the node's `key`.
    ///
    /// # Errors
    /// An invalid or expired cursor.
    pub fn new(query: &'a Query, key: Key, now_ms: u64) -> Result<Self, Failure> {
        let after = run::opened(query, &key, now_ms)?;
        let start = after
            .as_deref()
            .and_then(|after| update_time(after, &query.order_by));
        Ok(Self {
            query,
            key,
            now_ms,
            after,
            start,
            kept: Vec::new(),
            scanned: 0,
        })
    }

    /// The update time to start from: the cursor row's, or `None` for the
    /// first page.
    #[must_use]
    pub const fn start(&self) -> Option<u64> {
        self.start
    }

    /// Takes the rows that share the next update time; `false` once the
    /// page is full and the walk can stop.
    pub fn offer(&mut self, rows: Vec<Term>) -> bool {
        self.scanned = self.scanned.saturating_add(rows.len());
        let mut keyed: Vec<Keyed> = rows
            .into_iter()
            .filter(|row| run::satisfies(self.query, row))
            .map(|row| (run::sort_key(&row, &self.query.order_by), row))
            .filter(|(key, _)| self.after.as_ref().is_none_or(|after| key > after))
            .collect();
        keyed.sort_by(|left, right| left.0.cmp(&right.0));
        let wanted = self.query.limit.unwrap_or(1).saturating_add(1);
        keyed.truncate(wanted.saturating_sub(self.kept.len()));
        self.kept.extend(keyed);
        self.kept.len() < wanted
    }

    /// The page the offered rows make.
    #[must_use]
    pub fn finish(self) -> Term {
        run::page(
            self.query,
            self.kept,
            (&self.key, self.now_ms),
            self.scanned,
        )
    }
}
