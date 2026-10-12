//! Existing default claimed fixture for the guard/retry integration suites.

use super::support::{Scenario, TestError};
use wf_domain::claim::Claim;
use wf_domain::lease::LeaseToken;
use wf_domain::workflow::{CommittedCommand, Create, Operation, reduce};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

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
}
