//! The end of a shard round for finished Flows (ADR-016): images go to the
//! cold writer, and its answers turn Flows into stubs.

use std::time::Duration;

use wf_application::commands::flow::schedule::SCHEDULE_TYPE;
use wf_storage::cold::{ColdBatch, Location};

use super::super::{Key, ShardState};
use super::{Answer, HOUR_MS, Slot, values};

/// A durable image of a Flow that finished, as the writer thread answers.
pub(in super::super) struct Ack {
    key: Key,
    version: u64,
    location: Location,
    removal: u64,
}

impl ShardState {
    /// Ends a round: Flows loaded and left unchanged go back to their
    /// stubs, Flows whose images are durable become stubs, Flows that
    /// turned terminal go to the cold writer, and so do the kept versions
    /// of running Flows over the budget (ADR-016 phase D).
    pub(crate) fn cool(&mut self) {
        for key in std::mem::take(&mut self.cold.warmed) {
            if let Some(Slot::Warm(_, stub)) = self.records.get(&*key) {
                let stub = stub.clone();
                self.records.insert(key, Slot::Cold(stub));
            }
        }
        while let Ok(answer) = self.cold.acks.1.try_recv() {
            self.cold.in_flight = self.cold.in_flight.saturating_sub(1);
            self.answered(answer);
        }
        self.spill();
        let demoted = std::mem::take(&mut self.cold.demote);
        let mut images = Vec::new();
        let mut pending = Vec::new();
        for key in demoted {
            if let Some((hour_image, version, removal)) = self.encode_cold(&key) {
                images.push(hour_image);
                pending.push((key, version, removal));
            }
        }
        if images.is_empty() {
            return;
        }
        let sender = self.cold.acks.0.clone();
        self.cold.in_flight = self.cold.in_flight.saturating_add(1);
        self.cold.queue.send(ColdBatch {
            images,
            done: Box::new(move |result| {
                let acks = match result {
                    Ok(locations) => pending
                        .into_iter()
                        .zip(locations)
                        .map(|((key, version, removal), location)| Ack {
                            key,
                            version,
                            location,
                            removal,
                        })
                        .collect(),
                    // Not written: the Flows stay in memory.
                    Err(_) => Vec::new(),
                };
                // A stopped shard no longer needs its answer.
                drop(sender.send(Answer::Cold(acks)));
            }),
        });
    }

    /// Waits until every batch sent is answered, for at most `limit`, and
    /// applies the answers; replay ends with this so its restart sees the
    /// same split as before.
    pub(crate) fn settle_cold(&mut self, limit: Duration) {
        let deadline = std::time::Instant::now().checked_add(limit);
        while self.cold.in_flight > 0 {
            let left = deadline.map_or(Duration::ZERO, |deadline| {
                deadline.saturating_duration_since(std::time::Instant::now())
            });
            match self.cold.acks.1.recv_timeout(left) {
                Ok(answer) => {
                    self.cold.in_flight = self.cold.in_flight.saturating_sub(1);
                    self.answered(answer);
                }
                Err(_) => break,
            }
        }
    }

    fn answered(&mut self, answer: Answer) {
        match answer {
            Answer::Cold(acks) => self.settle(acks),
            Answer::Kept(moved) => self.settle_kept(moved),
        }
    }

    fn encode_cold(&self, key: &Key) -> Option<((u64, Vec<u8>), u64, u64)> {
        let Some(Slot::Hot(entry)) = self.records.get(&**key) else {
            return None;
        };
        // Schedules stay in memory: the poll reads them every second.
        if entry.record.workflow_type() == SCHEDULE_TYPE.as_bytes() {
            return None;
        }
        let (removal, _) = self.removal_slot(entry)?;
        let bytes = super::encode(key, entry, &self.cold.store)?;
        Some(((removal / HOUR_MS, bytes), entry.record.version(), removal))
    }

    /// Turns Flows whose images are durable into stubs; a Flow changed
    /// since keeps memory and its image is dropped.
    fn settle(&mut self, acks: Vec<Ack>) {
        for ack in acks {
            let fields = match self.records.get(&*ack.key) {
                Some(Slot::Hot(entry))
                    if entry.record.version() == ack.version
                        && self.removal_slot(entry).map(|(until, _)| until)
                            == Some(ack.removal) =>
                {
                    Some((
                        entry.record.workflow_type().to_vec(),
                        entry.record.state().to_vec(),
                        (
                            entry.record.updated_at().get(),
                            entry.sequence,
                            entry.priority,
                        ),
                        (u64::from(entry.history.len()), u64::from(values(entry))),
                    ))
                }
                _ => None,
            };
            let Some((flow_type, state, (updated, sequence, priority), counts)) = fields else {
                drop(self.cold.store.release(ack.location.file, 1));
                continue;
            };
            let stub = self.stub(
                ack.location,
                (&flow_type, &state),
                (updated, sequence, priority, ack.removal),
                counts,
            );
            self.records.insert(ack.key, Slot::Cold(stub));
        }
    }
}
