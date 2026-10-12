//! Local deterministic single-record scenario helper; no runtime dependency.

use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::lease::LeaseGuard;
use wf_domain::workflow::{CommittedCommand, Operation, Outcome, WorkflowRecord, reduce};
use wf_types::version::TimestampMillis;

pub(crate) type TestError = Box<dyn std::error::Error>;
pub(crate) type TestResult = Result<(), TestError>;

pub(crate) struct Scenario {
    pub(crate) record: WorkflowRecord,
    pub(crate) history: Vec<HistoryEntry>,
}

impl Scenario {
    pub(crate) fn guard(&self) -> Result<LeaseGuard, TestError> {
        Ok(LeaseGuard {
            token: self
                .record
                .lease()
                .ok_or("record has no lease")?
                .token
                .clone(),
            fence: self.record.fence(),
        })
    }

    pub(crate) fn command(&self, now: u64, operation: Operation) -> CommittedCommand {
        CommittedCommand {
            id: self.record.id().clone(),
            now: TimestampMillis::new(now),
            operation,
        }
    }

    pub(crate) fn apply(&mut self, now: u64, operation: Operation) -> Result<Outcome, TestError> {
        let command = self.command(now, operation);
        let input = self.record.clone();
        let first = reduce(Some(&self.record), &command)?;
        assert_eq!(
            first,
            reduce(Some(&self.record), &command)?,
            "deterministic replay"
        );
        assert_eq!(self.record, input, "borrowed authoritative input mutated");
        let (outcome, write, event) = first.into_parts();
        match (write, event) {
            (Some(write), Some(event)) => {
                assert_eq!(write.expected_version, Some(self.record.version()));
                assert_eq!(write.record.version(), event.version);
                self.record = write.record;
                self.history.push(event);
            }
            (None, None) => {}
            _ => return Err("record/history write cardinality differs".into()),
        }
        Ok(outcome)
    }

    pub(crate) fn count(&self, kind: EventKind) -> usize {
        self.history
            .iter()
            .filter(|entry| entry.kind == kind)
            .count()
    }
}
