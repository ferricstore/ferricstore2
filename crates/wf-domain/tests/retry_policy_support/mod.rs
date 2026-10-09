//! Deterministic source-shaped policy fixtures; no persistence is simulated.

use wf_domain::claim::Claim;
use wf_domain::history::HistoryEntry;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::{Backoff, BackoffKind, InstalledRetryPolicy, RetryOverride};
use wf_domain::retry::{JitterSample, PolicyRetry};
use wf_domain::workflow::{
    CommittedCommand, Create, Operation, Outcome, WorkflowRecord, reduce, reduce_with_policy,
};
use wf_types::identity::RecordId;
use wf_types::version::TimestampMillis;

pub(crate) type TestError = Box<dyn std::error::Error>;
pub(crate) type TestResult = Result<(), TestError>;

pub(crate) struct Scenario {
    pub(crate) record: WorkflowRecord,
    pub(crate) history: Vec<HistoryEntry>,
}

impl Scenario {
    pub(crate) fn claimed(workflow_type: &[u8], state: &[u8]) -> Result<Self, TestError> {
        Self::claimed_by(workflow_type, state, b"worker-charge")
    }

    pub(crate) fn claimed_by(
        workflow_type: &[u8],
        state: &[u8],
        worker: &[u8],
    ) -> Result<Self, TestError> {
        let create = CommittedCommand {
            id: RecordId::new(b"policy-fixture".to_vec())?,
            now: TimestampMillis::new(1_000),
            operation: Operation::Create(Create {
                workflow_type: workflow_type.to_vec(),
                state: state.to_vec(),
                run_at: Some(TimestampMillis::new(1_000)),
                payload: Some(b"payload".to_vec()),
            }),
        };
        let (_, write, event) = reduce(None, &create)?.into_parts();
        let mut scenario = Self {
            record: write.ok_or("fixture create missing")?.record,
            history: vec![event.ok_or("fixture created event missing")?],
        };
        let mut first_claim = claim(state)?;
        first_claim.worker = worker.to_vec();
        assert_eq!(
            scenario.apply(1_000, Operation::Claim(first_claim), None)?,
            Outcome::Changed
        );
        Ok(scenario)
    }

    pub(crate) fn guard(&self) -> Result<LeaseGuard, TestError> {
        Ok(LeaseGuard {
            token: self
                .record
                .lease()
                .ok_or("fixture lease missing")?
                .token
                .clone(),
            fence: self.record.fence(),
        })
    }

    pub(crate) fn retry(&self, policy_override: RetryOverride) -> Result<PolicyRetry, TestError> {
        Ok(PolicyRetry {
            guard: self.guard()?,
            run_at: None,
            policy_override,
            jitter: JitterSample::new(42_731_729),
            payload: None,
            error: None,
        })
    }

    pub(crate) fn apply(
        &mut self,
        now: u64,
        operation: Operation,
        policy: Option<&InstalledRetryPolicy>,
    ) -> Result<Outcome, TestError> {
        let command = CommittedCommand {
            id: self.record.id().clone(),
            now: TimestampMillis::new(now),
            operation,
        };
        let before_record = self.record.clone();
        let before_command = command.clone();
        let before_policy = policy.cloned();
        let plan = reduce_with_policy(Some(&self.record), &command, policy)?;
        assert_eq!(
            plan,
            reduce_with_policy(Some(&self.record), &command, policy)?
        );
        assert_eq!(self.record, before_record);
        assert_eq!(command, before_command);
        assert_eq!(policy, before_policy.as_ref());
        let (outcome, write, event) = plan.into_parts();
        match (write, event) {
            (Some(write), Some(event)) => {
                assert_eq!(write.expected_version, Some(self.record.version()));
                assert_eq!(write.record.version(), event.version);
                self.record = write.record;
                self.history.push(event);
            }
            (None, None) => {}
            _ => return Err("record/history cardinality differs".into()),
        }
        Ok(outcome)
    }
}

pub(crate) fn claim(state: &[u8]) -> Result<Claim, TestError> {
    Ok(Claim {
        expected_state: state.to_vec(),
        worker: b"worker-charge".to_vec(),
        token: LeaseToken::new(b"explicit-policy-token".to_vec())?,
        lease_ms: 30_000,
    })
}

pub(crate) fn configured(
    max_retries: u16,
    kind: BackoffKind,
    delay: u64,
    target: &[u8],
) -> RetryOverride {
    RetryOverride {
        max_retries: Some(max_retries),
        backoff: Some(Backoff {
            kind,
            base_ms: delay,
            max_ms: delay,
            jitter_pct: 0,
        }),
        exhausted_to: Some(target.to_vec()),
    }
}
