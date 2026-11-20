//! Rebuilding shard state from the log after an optional snapshot (ADR-013).
//!
//! Without a snapshot every logged change is applied. With one, a change
//! whose owning shard (under the snapshot's shard count) has not logged its
//! marker yet is already in the snapshot and is skipped. Every shard of the
//! snapshot must have logged its marker, or the log does not match the
//! snapshot and the node refuses to open.

use wf_application::commands::queue::Partition;
use wf_domain::workflow::Outcome;

use crate::supervision::shard::command::Logged;
use crate::supervision::shard::state::ShardState;

use super::NodeError;
use super::router::shard_index;

/// The loaded snapshot, as replay sees it.
pub(crate) struct Cut {
    pub(crate) id: u64,
    pub(crate) base: u64,
    shards: u16,
    seen: Vec<bool>,
}

impl Cut {
    pub(crate) fn new(id: u64, base: u64, shards: u16) -> Self {
        Self {
            id,
            base,
            shards,
            seen: vec![false; usize::from(shards)],
        }
    }

    /// Whether a change to `partition` at this point of the log is already
    /// in the snapshot.
    fn holds(&self, partition: &Partition) -> bool {
        shard_index(partition, self.shards)
            .and_then(|index| self.seen.get(index))
            .is_some_and(|seen| !seen)
    }
}

/// Applies every logged change the snapshot does not hold; returns the
/// highest snapshot id named by a marker, or 0.
pub(crate) fn replay(
    states: &mut [ShardState],
    records: &[Vec<u8>],
    shards: u16,
    mut cut: Option<&mut Cut>,
) -> Result<u64, NodeError> {
    let mut newest_marker = 0;
    for (position, record) in records.iter().enumerate() {
        let failed = NodeError::Replay { record: position };
        let logged = Logged::decode(record).ok_or(failed)?;
        if let Logged::Marker { snapshot, shard } = logged {
            newest_marker = newest_marker.max(snapshot);
            if let Some(cut) = cut.as_deref_mut()
                && cut.id == snapshot
            {
                *cut.seen.get_mut(usize::from(shard)).ok_or(failed)? = true;
            }
            continue;
        }
        let logged = match cut.as_deref() {
            Some(cut) => unheld(logged, cut),
            None => Some(logged),
        };
        let Some(logged) = logged else {
            continue;
        };
        for (index, change) in by_shard(logged, shards).ok_or(failed)? {
            let state = states.get_mut(index).ok_or(failed)?;
            if state.apply(&change) != Ok(Outcome::Changed) {
                return Err(failed);
            }
        }
    }
    if let Some(cut) = cut
        && let Some(shard) = cut.seen.iter().position(|seen| !seen)
    {
        return Err(NodeError::MissingMarker {
            shard: u16::try_from(shard).unwrap_or(u16::MAX),
        });
    }
    // Replay is done: later changes are live.
    for state in states.iter_mut() {
        state.go_live();
    }
    Ok(newest_marker)
}

/// The part of a change the snapshot does not hold, if any.
fn unheld(logged: Logged, cut: &Cut) -> Option<Logged> {
    match logged {
        Logged::Expire { now, keys } => {
            let keys: Vec<_> = keys
                .into_iter()
                .filter(|(partition, _)| !cut.holds(partition))
                .collect();
            (!keys.is_empty()).then_some(Logged::Expire { now, keys })
        }
        Logged::Group(changes) => {
            let changes: Vec<Logged> = changes
                .into_iter()
                .filter_map(|change| unheld(change, cut))
                .collect();
            (!changes.is_empty()).then_some(Logged::Group(changes))
        }
        other => match other.partition() {
            Some(partition) if cut.holds(partition) => None,
            _ => Some(other),
        },
    }
}

/// Routes a logged change to the shards owning its Flows; a sweep is split
/// into one sweep per shard.
fn by_shard(logged: Logged, shards: u16) -> Option<Vec<(usize, Logged)>> {
    if let Logged::Group(changes) = logged {
        // Each shard's part of a group stays one change.
        let mut parts: Vec<(usize, Vec<Logged>)> = Vec::new();
        for change in changes {
            let index = shard_index(change.partition()?, shards)?;
            match parts.iter_mut().find(|(owner, _)| *owner == index) {
                Some((_, part)) => part.push(change),
                None => parts.push((index, vec![change])),
            }
        }
        return Some(
            parts
                .into_iter()
                .map(|(index, part)| (index, Logged::Group(part)))
                .collect(),
        );
    }
    let Logged::Expire { now, keys } = logged else {
        let index = shard_index(logged.partition()?, shards)?;
        return Some(vec![(index, logged)]);
    };
    let mut parts: Vec<(usize, Logged)> = Vec::new();
    for key in keys {
        let index = shard_index(&key.0, shards)?;
        match parts.iter_mut().find(|(owner, _)| *owner == index) {
            Some((_, Logged::Expire { keys, .. })) => keys.push(key),
            _ => parts.push((
                index,
                Logged::Expire {
                    now,
                    keys: vec![key],
                },
            )),
        }
    }
    Some(parts)
}
