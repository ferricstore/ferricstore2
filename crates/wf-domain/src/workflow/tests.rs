//! Internal overflow fixtures exercise unreachable-through-short-runs boundaries.

use super::*;
use crate::claim::Claim;
use crate::lease::{LeaseGuard, LeaseToken};
use crate::policy::RetryOverride;
use crate::retry::{JitterSample, PolicyRetry, Retry, RetryTime};
use crate::value::Counter;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn queued() -> Result<WorkflowRecord, Box<dyn std::error::Error>> {
    let command = CommittedCommand {
        id: RecordId::new(b"counter-boundary".to_vec())?,
        now: TimestampMillis::new(0),
        operation: Operation::Create(Create {
            workflow_type: b"type".to_vec(),
            state: b"queued".to_vec(),
            run_at: None,
            payload: None,
        }),
    };
    let (_, write, _) = reduce(None, &command)?.into_parts();
    Ok(write.ok_or("fixture create missing")?.record)
}

fn claim_command(record: &WorkflowRecord) -> Result<CommittedCommand, DomainError> {
    Ok(CommittedCommand {
        id: record.id.clone(),
        now: TimestampMillis::new(0),
        operation: Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: b"worker".to_vec(),
            token: LeaseToken::new(b"token".to_vec())?,
            lease_ms: 1,
        }),
    })
}

#[test]
fn version_overflow_rejects_without_changing_the_authoritative_input() -> TestResult {
    let mut record = queued()?;
    record.version = u64::MAX;
    let command = claim_command(&record)?;
    let before = record.clone();
    assert_eq!(
        reduce(Some(&record), &command),
        Err(DomainError::CounterOverflow(Counter::Version))
    );
    assert_eq!(record, before);
    Ok(())
}

#[test]
fn fence_overflow_rejects_instead_of_reusing_a_generation() -> TestResult {
    let mut record = queued()?;
    record.fence = Fence::new(u64::MAX);
    let command = claim_command(&record)?;
    let before = record.clone();
    assert_eq!(
        reduce(Some(&record), &command),
        Err(DomainError::CounterOverflow(Counter::Fence))
    );
    assert_eq!(record, before);
    Ok(())
}

#[test]
fn attempts_overflow_cannot_release_a_running_lease_or_append_history() -> TestResult {
    let record = queued()?;
    let (_, write, _) = reduce(Some(&record), &claim_command(&record)?)?.into_parts();
    let mut running = write.ok_or("fixture claim missing")?.record;
    running.attempts = u64::MAX;
    let guard = LeaseGuard {
        token: running
            .lease
            .as_ref()
            .ok_or("fixture lease missing")?
            .token
            .clone(),
        fence: running.fence,
    };
    let command = CommittedCommand {
        id: running.id.clone(),
        now: TimestampMillis::new(0),
        operation: Operation::Retry(Retry {
            guard,
            run_at: RetryTime::Explicit(TimestampMillis::new(0)),
            payload: None,
            error: None,
        }),
    };
    let before = running.clone();
    assert_eq!(
        reduce(Some(&running), &command),
        Err(DomainError::CounterOverflow(Counter::Attempts))
    );
    assert_eq!(running, before);
    Ok(())
}

#[test]
fn legacy_duplicates_cannot_bypass_the_source_fence_input_limit() -> TestResult {
    let mut record = queued()?;
    let fence = Fence::new(MAX_EXACT_MILLIS + 1);
    record.fence = fence;
    record.attempts = 1;
    let guard = LeaseGuard {
        token: LeaseToken::new(b"old-token".to_vec())?,
        fence,
    };
    let cases = [
        (
            b"completed".as_slice(),
            Operation::Complete(Terminal {
                guard: guard.clone(),
                payload: None,
                value: None,
            }),
        ),
        (
            b"failed".as_slice(),
            Operation::Fail(Terminal {
                guard: guard.clone(),
                payload: None,
                value: None,
            }),
        ),
        (
            b"waiting".as_slice(),
            Operation::Transition(Transition {
                from: b"running".to_vec(),
                to: b"waiting".to_vec(),
                fence,
                lease_token: Some(guard.token.clone()),
                run_at: None,
                payload: None,
            }),
        ),
        (
            b"failed".as_slice(),
            Operation::RetryWithPolicy(PolicyRetry {
                guard: guard.clone(),
                run_at: None,
                policy_override: RetryOverride::default(),
                jitter: JitterSample::new(0),
                payload: None,
                error: None,
            }),
        ),
        (
            b"queued".as_slice(),
            Operation::Retry(Retry {
                guard,
                run_at: RetryTime::PolicyResolved(TimestampMillis::new(0)),
                payload: None,
                error: None,
            }),
        ),
    ];
    for (state, operation) in cases {
        record.state = state.to_vec();
        let before = record.clone();
        let command = CommittedCommand {
            id: record.id.clone(),
            now: TimestampMillis::new(0),
            operation,
        };
        let error = reduce(Some(&record), &command)
            .err()
            .ok_or("duplicate bypassed numeric guard admission")?;
        assert_eq!(
            error.to_string(),
            "ERR flow fencing_token exceeds maximum 9007199254740991"
        );
        assert_eq!(record, before);
    }
    Ok(())
}

#[test]
fn installed_retry_overflow_and_restored_result_preservation_are_atomic() -> TestResult {
    let record = queued()?;
    let (_, write, _) = reduce(Some(&record), &claim_command(&record)?)?.into_parts();
    let mut running = write.ok_or("fixture claim missing")?.record;
    running.result = Some(b"retained-result".to_vec());
    running.error = Some(b"old-error".to_vec());
    let retry = PolicyRetry {
        guard: LeaseGuard {
            token: running
                .lease
                .as_ref()
                .ok_or("fixture lease missing")?
                .token
                .clone(),
            fence: running.fence,
        },
        run_at: Some(TimestampMillis::new(0)),
        policy_override: RetryOverride::default(),
        jitter: JitterSample::new(0),
        payload: None,
        error: None,
    };
    let command = CommittedCommand {
        id: running.id.clone(),
        now: TimestampMillis::new(0),
        operation: Operation::RetryWithPolicy(retry),
    };
    for (version, attempts, expected) in [
        (u64::MAX, 0, Counter::Version),
        (2, u64::MAX, Counter::Attempts),
    ] {
        running.version = version;
        running.attempts = attempts;
        let before = running.clone();
        assert_eq!(
            reduce(Some(&running), &command),
            Err(DomainError::CounterOverflow(expected))
        );
        assert_eq!(running, before);
    }
    running.version = 2;
    running.attempts = 0;
    let plan = reduce(Some(&running), &command)?;
    let next = &plan.write().ok_or("retry write missing")?.record;
    assert_eq!(next.result(), Some(b"retained-result".as_slice()));
    assert_eq!(next.error(), None);
    assert_eq!(next.lease(), None);
    Ok(())
}

#[test]
fn claim_phase_checks_counter_extremes_without_partial_plans() -> TestResult {
    use crate::claim::apply::{ClaimApplyView, ClaimEnvelope};
    use crate::claim::apply::{ClaimDecision, ClaimRejection, ClaimWorkLimits};
    for (version, fence, rejection) in [
        (u64::MAX, 0, Some(ClaimRejection::VersionOverflow)),
        (1, u64::MAX, Some(ClaimRejection::FenceOverflow)),
        (u64::MAX, u64::MAX, Some(ClaimRejection::VersionOverflow)),
        (u64::MAX - 1, u64::MAX - 1, None),
    ] {
        let mut record = queued()?;
        record.version = version;
        record.fence = Fence::new(fence);
        let before = record.clone();
        with_counter_claim(
            &record,
            |view: ClaimApplyView<'_>, envelope: &ClaimEnvelope<'_>| {
                let plan = request::decide_claim_request(
                    view,
                    envelope,
                    admission::AdmissionLimits::default(),
                    ClaimWorkLimits::default(),
                )?;
                match (plan.decision(), rejection) {
                    (
                        request::DispatchDecision::Fresh(ClaimDecision::Rejected(actual)),
                        Some(expected),
                    ) => {
                        assert_eq!(*actual, expected);
                    }
                    (request::DispatchDecision::Fresh(ClaimDecision::Accepted(claim)), None) => {
                        let batch = claim.batch().ok_or("missing extreme plan")?;
                        let item = &batch.batch().items()[0];
                        let next = &item.plan().write().ok_or("missing extreme write")?.record;
                        assert_eq!((next.version(), next.fence().get()), (u64::MAX, u64::MAX));
                        assert_eq!(
                            next.lease().ok_or("no extreme lease")?.token.as_bytes(),
                            b"worker:0:18446744073709551615"
                        );
                        let history = item.plan().history().ok_or("no extreme history")?;
                        assert_eq!(history.event_id, "0-18446744073709551615");
                        assert_eq!((history.version, history.fence.get()), (u64::MAX, u64::MAX));
                        assert_eq!(
                            (batch.consumed_ordinals(), batch.next_ordinal()),
                            (0, Some(u32::MAX))
                        );
                    }
                    _ => return Err("unexpected claim counter decision".into()),
                }
                Ok(())
            },
        )?;
        assert_eq!(record, before);
    }
    Ok(())
}

fn with_counter_claim(
    record: &WorkflowRecord,
    run: impl for<'view> FnOnce(
        crate::claim::apply::ClaimApplyView<'view>,
        &crate::claim::apply::ClaimEnvelope<'view>,
    ) -> TestResult,
) -> TestResult {
    use crate::claim::apply::{
        ClaimApplyView, ClaimEncoding, ClaimEnvelope, ClaimQuery, ClaimStates,
    };
    use crate::claim::{ClaimRecord, DueOrder, PartitionFilter, ScopedRecordId};
    use std::collections::BTreeMap;
    use wf_types::identity::{GroupId, PartitionId, RecoveryId, RequestId, TenantId};
    use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, LogIdentity};
    let scope = request::RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    );
    let partition = PartitionId::new(b"p".to_vec())?;
    let id = RequestId::new(b"extremes".to_vec())?;
    let time = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    let identity = |index| {
        control::EntryIdentity::new(
            LogIdentity::new(1, 7, AppliedPosition::new(index)),
            control::PreparedDigest::new([1; 32]),
        )
    };
    let control = control::plan_control(
        control::ControlState::initial(time),
        identity(0),
        control::EntryStamp::Unstamped,
    )?
    .next();
    let entry = ClaimRecord {
        record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 11,
    };
    let scoped = ScopedRecordId::for_record(&entry);
    let placements = BTreeMap::from([(partition.clone(), scope.group().clone())]);
    let records = BTreeMap::from([(scoped, entry)]);
    let canonical = BTreeMap::from([(scoped, record)]);
    let parallel = BTreeMap::from([(DueOrder::for_record(&entry).ok_or("no due")?, entry)]);
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let encodings = BTreeMap::from([(
        scoped,
        ClaimEncoding {
            partition: &partition,
            state_key: b"f:{p}:s:counter-boundary",
        },
    )]);
    let view = ClaimApplyView {
        gate: request::gate::GateView {
            scope: &scope,
            control,
            outcomes: &outcomes,
        },
        group: indexed::IndexedGroupView {
            batch: batch::GroupView {
                group: scope.group(),
                placements: &placements,
                records: &canonical,
            },
            records: &records,
            parallel: &parallel,
            fifo: &fifo,
            policies: &policies,
        },
        revision: 1,
        encodings: &encodings,
    };
    let envelope = ClaimEnvelope {
        header: request::gate::CommittedHeader {
            format_version: 1,
            scope: &scope,
            request: &id,
            intent: request::ClientIntent::new(1, [1; 32])?,
            identity: identity(1),
            stamp: time,
            encoded_entry: b"entry",
            encoded_isolation_scope: b"scope",
        },
        query: ClaimQuery {
            workflow_type: b"type",
            partitions: PartitionFilter::Any,
            states: ClaimStates::Any,
            exclude_states: &[],
            priority: None,
            worker: b"worker",
            lease_ms: 1,
            limit: 1,
        },
        token_version: 1,
        first_ordinal: u32::MAX,
        scan_budget: 1,
        head_budget: 1,
        resume: None,
    };
    run(view, &envelope)
}

mod claim_ownership;
mod step_continue;
