//! Local deterministic single-record scenario helper; no runtime dependency.

use wf_domain::claim::Claim;
use wf_domain::history::{EventKind, HistoryEntry};
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::workflow::{CommittedCommand, Create, Operation, Outcome, WorkflowRecord, reduce};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

pub(crate) type TestError = Box<dyn std::error::Error>;
pub(crate) type TestResult = Result<(), TestError>;

pub(crate) struct Scenario {
    pub(crate) record: WorkflowRecord,
    pub(crate) history: Vec<HistoryEntry>,
}

impl Scenario {
    pub(crate) fn claimed(id: &[u8], now: u64) -> Result<Self, TestError> {
        let id = RecordId::new(id.to_vec())?;
        let create = CommittedCommand {
            id: id.clone(),
            now: TimestampMillis::new(now),
            operation: Operation::Create(Create {
                workflow_type: b"fencing-type".to_vec(),
                state: b"queued".to_vec(),
                payload: Some(b"payload".to_vec()),
                run_at: Some(TimestampMillis::new(now)),
            }),
        };
        let (_, write, event) = reduce(None, &create)?.into_parts();
        let mut scenario = Self {
            record: write.ok_or("create did not emit a record")?.record,
            history: vec![event.ok_or("create did not emit history")?],
        };
        scenario.apply(
            now,
            Operation::Claim(Claim {
                expected_state: b"queued".to_vec(),
                worker: b"fencing-worker".to_vec(),
                token: LeaseToken::new(b"opaque-committed-token".to_vec())?,
                lease_ms: 30_000,
            }),
        )?;
        Ok(scenario)
    }

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
