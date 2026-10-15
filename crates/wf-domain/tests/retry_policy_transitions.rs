//! Source-derived single-record retry schedules, exhaustion and history.

#![forbid(unsafe_code)]

mod retry_policy_support;

use std::collections::BTreeMap;

use retry_policy_support::{Scenario, TestResult, claim, configured};
use wf_domain::history::EventKind;
use wf_domain::policy::{Backoff, BackoffKind, InstalledRetryPolicy, RetryOverride};
use wf_domain::retry::RetryDecision;
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_domain::workflow::{
    CommittedCommand, DomainError, MAX_EXACT_MILLIS, Operation, Outcome, Transition,
    reduce_with_policy,
};
use wf_types::version::TimestampMillis;

// Frozen declaration ab0be7e9b8c01099, policy section lines 396-427.
#[test]
fn stored_state_retry_policy_drives_retry_exhaustion_without_command_override() -> TestResult {
    let installed = InstalledRetryPolicy::new(
        b"payment".to_vec(),
        RetryOverride {
            max_retries: Some(10),
            ..RetryOverride::default()
        },
        BTreeMap::from([(
            b"charge_card".to_vec(),
            RetryOverride {
                max_retries: Some(0),
                exhausted_to: Some(b"payment_failed".to_vec()),
                ..RetryOverride::default()
            },
        )]),
    )?;
    let mut scenario = Scenario::claimed_by(b"payment", b"charge_card", b"worker-policy")?;
    let retry = scenario.retry(RetryOverride::default())?;
    scenario.apply(2_000, Operation::RetryWithPolicy(retry), Some(&installed))?;
    assert_eq!(scenario.record.state(), b"payment_failed");
    assert_eq!(
        scenario.record.next_run_at(),
        Some(TimestampMillis::new(2_000))
    );
    assert_eq!(scenario.record.attempts(), 1);
    assert_eq!(scenario.record.lease(), None);
    assert_eq!(scenario.record.run_state(), None);
    Ok(())
}

// Frozen declaration 579ae5ad18ea20b4, policy section lines 429-464.
#[test]
fn command_retry_policy_overrides_stored_state_policy() -> TestResult {
    let installed = InstalledRetryPolicy::new(
        b"payment".to_vec(),
        RetryOverride::default(),
        BTreeMap::from([(
            b"charge_card".to_vec(),
            RetryOverride {
                max_retries: Some(0),
                exhausted_to: Some(b"payment_failed".to_vec()),
                ..RetryOverride::default()
            },
        )]),
    )?;
    let mut scenario = Scenario::claimed_by(b"payment", b"charge_card", b"worker-policy-override")?;
    let retry = scenario.retry(configured(3, BackoffKind::Fixed, 5_000, b"needs_review"))?;
    scenario.apply(2_000, Operation::RetryWithPolicy(retry), Some(&installed))?;
    assert_eq!(scenario.record.state(), b"charge_card");
    assert_eq!(
        scenario.record.next_run_at(),
        Some(TimestampMillis::new(7_000))
    );
    Ok(())
}

// Frozen declaration e1a8cf7f619f136a, priority section lines 588-659.
#[test]
fn flow_retry_returns_to_claimed_run_state_with_computed_backoff() -> TestResult {
    let mut scenario = Scenario::claimed(b"payment", b"charge_card")?;
    assert_eq!(scenario.record.state(), b"running");
    assert_eq!(scenario.record.run_state(), Some(b"charge_card".as_slice()));
    let retry = scenario.retry(RetryOverride {
        max_retries: Some(3),
        exhausted_to: Some(b"payment_failed".to_vec()),
        backoff: Some(Backoff {
            kind: BackoffKind::Exponential,
            base_ms: 1_000,
            max_ms: 30_000,
            jitter_pct: 0,
        }),
    })?;
    scenario.apply(2_000, Operation::RetryWithPolicy(retry), None)?;
    assert_eq!(scenario.record.state(), b"charge_card");
    assert_eq!(scenario.record.attempts(), 1);
    assert_eq!(
        scenario.record.next_run_at(),
        Some(TimestampMillis::new(3_000))
    );
    assert_eq!(scenario.record.lease(), None);
    let entry = scenario.history.last().ok_or("retry history absent")?;
    assert_eq!(entry.kind, EventKind::Retry);
    let meta = entry.retry.as_ref().ok_or("retry metadata absent")?;
    assert_eq!(meta.decision.as_str(), "scheduled");
    assert_eq!(meta.run_state, b"charge_card");
    assert_eq!(meta.next_run_at, Some(TimestampMillis::new(3_000)));
    assert_eq!(meta.policy.max_retries, 3);
    assert_eq!(meta.policy.backoff.kind.as_str(), "exponential");
    assert_eq!(meta.policy.backoff.base_ms, 1_000);
    assert_eq!(meta.policy.backoff.max_ms, 30_000);
    assert_eq!(meta.policy.backoff.jitter_pct, 0);
    assert_eq!(meta.policy.exhausted_to, b"payment_failed");
    assert_eq!(
        scenario.apply(2_999, Operation::Claim(claim(b"charge_card")?), None)?,
        Outcome::NotEligible
    );
    let mut next_claim = claim(b"charge_card")?;
    next_claim.worker = b"worker-charge-b".to_vec();
    assert_eq!(
        scenario.apply(3_000, Operation::Claim(next_claim), None)?,
        Outcome::Changed
    );
    assert_eq!(scenario.record.id().as_bytes(), b"policy-fixture");
    assert_eq!(scenario.record.run_state(), Some(b"charge_card".as_slice()));
    Ok(())
}

// Frozen declaration 78839ca2df54b42f, priority section lines 661-704.
#[test]
fn flow_retry_exhausts_to_configured_active_state() -> TestResult {
    let mut scenario = Scenario::claimed(b"payment-exhaust", b"charge_card")?;
    let retry = scenario.retry(configured(0, BackoffKind::Fixed, 10_000, b"payment_failed"))?;
    scenario.apply(2_000, Operation::RetryWithPolicy(retry), None)?;
    assert_eq!(scenario.record.state(), b"payment_failed");
    assert_eq!(scenario.record.attempts(), 1);
    assert_eq!(
        scenario.record.next_run_at(),
        Some(TimestampMillis::new(2_000))
    );
    assert_eq!(scenario.record.lease(), None);
    let mut manual = claim(b"payment_failed")?;
    manual.worker = b"worker-manual".to_vec();
    assert_eq!(
        scenario.apply(2_000, Operation::Claim(manual), None)?,
        Outcome::Changed
    );
    assert_eq!(scenario.record.id().as_bytes(), b"policy-fixture");
    assert_eq!(scenario.record.state(), b"running");
    assert_eq!(
        scenario.record.run_state(),
        Some(b"payment_failed".as_slice())
    );
    assert_eq!(
        scenario
            .record
            .lease()
            .ok_or("manual claim lease absent")?
            .worker,
        b"worker-manual"
    );
    Ok(())
}

// Frozen declaration 537f7338ecf1c648, priority section lines 706-746.
#[test]
fn flow_retry_terminal_exhaustion_keeps_stable_audit_metadata() -> TestResult {
    let mut scenario = Scenario::claimed(b"payment-terminal-exhaust", b"charge_card")?;
    let retry = scenario.retry(configured(0, BackoffKind::Fixed, 10_000, b"failed"))?;
    scenario.apply(2_000, Operation::RetryWithPolicy(retry), None)?;
    assert_eq!(scenario.record.state(), b"failed");
    assert_eq!(scenario.record.next_run_at(), None);
    let entry = scenario.history.last().ok_or("retry history absent")?;
    let meta = entry.retry.as_ref().ok_or("retry metadata absent")?;
    assert_eq!(meta.decision.as_str(), "exhausted");
    assert_eq!(meta.next_run_at, None);
    assert_eq!(meta.policy.exhausted_to, b"failed");
    Ok(())
}

#[test]
fn retry_limit_is_inclusive_and_replay_mints_no_extra_attempts_or_events() -> TestResult {
    let mut scenario = Scenario::claimed(b"payment", b"queued")?;
    let policy = configured(1, BackoffKind::Fixed, 100, b"completed");
    let retry = scenario.retry(policy.clone())?;
    scenario.apply(2_000, Operation::RetryWithPolicy(retry.clone()), None)?;
    assert_eq!(scenario.record.state(), b"queued");
    assert_eq!(scenario.record.attempts(), 1);
    let first = scenario.record.clone();
    let count = scenario.history.len();
    assert_eq!(
        scenario.apply(2_050, Operation::RetryWithPolicy(retry), None)?,
        Outcome::LegacyNoop
    );
    assert_eq!(scenario.record, first);
    assert_eq!(scenario.history.len(), count);
    scenario.apply(2_100, Operation::Claim(claim(b"queued")?), None)?;
    let retry = scenario.retry(policy)?;
    scenario.apply(2_200, Operation::RetryWithPolicy(retry.clone()), None)?;
    assert_eq!(scenario.record.state(), b"completed");
    assert_eq!(scenario.record.attempts(), 2);
    assert_eq!(scenario.record.next_run_at(), None);
    let terminal = scenario.record.clone();
    let count = scenario.history.len();
    assert_eq!(
        scenario.apply(2_300, Operation::RetryWithPolicy(retry), None)?,
        Outcome::LegacyNoop
    );
    assert_eq!(scenario.record, terminal);
    assert_eq!(scenario.history.len(), count);
    Ok(())
}

#[test]
fn explicit_time_bypasses_backoff_but_cannot_bypass_exhaustion_or_admission() -> TestResult {
    for target in [b"failed".as_slice(), b"completed", b"cancelled", b"manual"] {
        let mut scenario = Scenario::claimed(b"payment", b"charge_card")?;
        let mut retry = scenario.retry(configured(0, BackoffKind::Fixed, 100, target))?;
        retry.run_at = Some(TimestampMillis::new(9_000));
        scenario.apply(2_000, Operation::RetryWithPolicy(retry), None)?;
        let due = if target == b"manual" {
            Some(TimestampMillis::new(2_000))
        } else {
            None
        };
        assert_eq!(scenario.record.state(), target);
        assert_eq!(scenario.record.next_run_at(), due);
    }
    let mut scenario = Scenario::claimed(b"payment", b"charge_card")?;
    let mut retry = scenario.retry(RetryOverride::default())?;
    retry.run_at = Some(TimestampMillis::new(0));
    scenario.apply(2_000, Operation::RetryWithPolicy(retry.clone()), None)?;
    assert_eq!(scenario.record.next_run_at(), Some(TimestampMillis::new(0)));
    retry.run_at = Some(TimestampMillis::new(MAX_EXACT_MILLIS + 1));
    let command = CommittedCommand {
        id: scenario.record.id().clone(),
        now: TimestampMillis::new(2_100),
        operation: Operation::RetryWithPolicy(retry),
    };
    assert_eq!(
        reduce_with_policy(Some(&scenario.record), &command, None),
        Err(DomainError::RunAtTooLarge)
    );
    Ok(())
}

#[test]
fn noop_precedes_unused_policy_lookup_and_changed_intent_but_intervention_rejects() -> TestResult {
    let mut scenario = Scenario::claimed(b"payment", b"queued")?;
    let mut retry = scenario.retry(configured(3, BackoffKind::Fixed, 100, b"failed"))?;
    retry.payload = Some(b"first".to_vec());
    retry.error = Some(b"first-error".to_vec());
    scenario.apply(2_000, Operation::RetryWithPolicy(retry.clone()), None)?;
    let original = scenario.record.clone();
    let foreign =
        InstalledRetryPolicy::new(b"other".to_vec(), RetryOverride::default(), BTreeMap::new())?;
    retry.payload = Some(b"different".to_vec());
    retry.error = None;
    retry.policy_override.max_retries = Some(0);
    assert_eq!(
        scenario.apply(
            2_100,
            Operation::RetryWithPolicy(retry.clone()),
            Some(&foreign)
        )?,
        Outcome::LegacyNoop
    );
    assert_eq!(scenario.record, original);
    scenario.apply(
        2_100,
        Operation::Transition(Transition {
            from: b"queued".to_vec(),
            to: b"waiting".to_vec(),
            fence: scenario.record.fence(),
            lease_token: None,
            run_at: None,
            payload: None,
        }),
        None,
    )?;
    let command = CommittedCommand {
        id: scenario.record.id().clone(),
        now: TimestampMillis::new(2_200),
        operation: Operation::RetryWithPolicy(retry),
    };
    assert_eq!(
        reduce_with_policy(Some(&scenario.record), &command, None),
        Err(DomainError::StaleLease)
    );
    Ok(())
}

#[test]
fn retry_replaces_error_retains_payload_and_clears_run_state_and_lease() -> TestResult {
    let mut scenario = Scenario::claimed(b"payment", b"charge_card")?;
    let mut retry = scenario.retry(configured(3, BackoffKind::None, 0, b"failed"))?;
    retry.error = Some(Vec::new());
    let fence = scenario.record.fence();
    scenario.apply(2_000, Operation::RetryWithPolicy(retry), None)?;
    assert_eq!(scenario.record.payload(), Some(b"payload".as_slice()));
    assert_eq!(scenario.record.error(), Some(b"".as_slice()));
    assert_eq!(scenario.record.fence(), fence);
    assert_eq!(scenario.record.run_state(), None);
    assert_eq!(scenario.record.lease_deadline().get(), 0);
    scenario.apply(2_000, Operation::Claim(claim(b"charge_card")?), None)?;
    let retry = scenario.retry(configured(3, BackoffKind::None, 0, b"failed"))?;
    scenario.apply(2_001, Operation::RetryWithPolicy(retry), None)?;
    assert_eq!(scenario.record.error(), None);
    assert_eq!(scenario.record.result(), None);
    let meta = scenario
        .history
        .last()
        .and_then(|entry| entry.retry.as_ref())
        .ok_or("retry metadata absent")?;
    assert_eq!(meta.decision, RetryDecision::Scheduled);
    Ok(())
}

#[test]
fn policy_retry_admission_precedes_missing_record_and_noop_and_foreign_view_rejects_fresh()
-> TestResult {
    let mut scenario = Scenario::claimed(b"payment", b"queued")?;
    let foreign =
        InstalledRetryPolicy::new(b"other".to_vec(), RetryOverride::default(), BTreeMap::new())?;
    let retry = scenario.retry(configured(3, BackoffKind::Fixed, 100, b"failed"))?;
    let command = CommittedCommand {
        id: scenario.record.id().clone(),
        now: TimestampMillis::new(2_000),
        operation: Operation::RetryWithPolicy(retry.clone()),
    };
    assert_eq!(
        reduce_with_policy(Some(&scenario.record), &command, Some(&foreign)),
        Err(DomainError::PolicyTypeMismatch)
    );
    scenario.apply(2_000, Operation::RetryWithPolicy(retry.clone()), None)?;
    let mut invalid = retry;
    invalid.policy_override.max_retries = Some(1_001);
    let command = CommittedCommand {
        id: scenario.record.id().clone(),
        now: TimestampMillis::new(2_100),
        operation: Operation::RetryWithPolicy(invalid),
    };
    for record in [None, Some(&scenario.record)] {
        let error = reduce_with_policy(record, &command, None)
            .err()
            .ok_or("invalid input bypassed admission")?;
        assert_eq!(
            error.to_string(),
            "ERR flow retry max_retries must be between 0 and 1000"
        );
    }
    Ok(())
}

#[test]
fn generated_policy_deadline_can_exceed_the_explicit_input_ceiling() -> TestResult {
    let mut scenario = Scenario::claimed(b"payment", b"queued")?;
    let retry = scenario.retry(configured(3, BackoffKind::Fixed, 100, b"failed"))?;
    scenario.apply(MAX_EXACT_MILLIS, Operation::RetryWithPolicy(retry), None)?;
    assert_eq!(
        scenario.record.next_run_at(),
        Some(TimestampMillis::new(MAX_EXACT_MILLIS + 100))
    );
    Ok(())
}

#[test]
fn generated_policy_retry_exhaustion_matches_independent_attempt_model() -> TestResult {
    for limit in [0_u16, 1, 3, 8] {
        for target in [b"manual".as_slice(), b"failed", b"completed", b"cancelled"] {
            let mut scenario = Scenario::claimed(b"model-payment", b"queued")?;
            let policy = configured(limit, BackoffKind::Fixed, 7, target);
            let (mut now, mut version, mut fence, mut events) = (2_000, 2, 1, 2);
            for attempt in 1..=u64::from(limit) + 1 {
                let exhausted = attempt > u64::from(limit);
                let terminal = exhausted && target != b"manual";
                let state = if exhausted { target } else { b"queued" };
                let due = (!terminal).then_some(now + if exhausted { 0 } else { 7 });
                let retry = scenario.retry(policy.clone())?;
                assert_eq!(
                    scenario.apply(now, Operation::RetryWithPolicy(retry.clone()), None)?,
                    Outcome::Changed
                );
                version += 1;
                events += 1;
                assert_eq!(scenario.record.state(), state);
                assert_eq!(scenario.record.attempts(), attempt);
                assert_eq!(scenario.record.version(), version);
                assert_eq!(scenario.record.fence().get(), fence);
                assert_eq!(scenario.record.next_run_at(), due.map(TimestampMillis::new));
                assert_eq!(scenario.record.lease(), None);
                assert_eq!(scenario.record.run_state(), None);
                assert_eq!(scenario.history.len(), events);
                let event = scenario.history.last().ok_or("model retry event absent")?;
                assert_eq!(event.kind, EventKind::Retry);
                assert_eq!(event.occurred_at, TimestampMillis::new(now));
                assert_eq!(event.version, version);
                assert_eq!(event.fence.get(), fence);
                assert_eq!(event.attempts, attempt);
                assert_eq!(event.state, state);
                assert_eq!(event.next_run_at, due.map(TimestampMillis::new));
                let metadata = event.retry.as_ref().ok_or("model retry metadata absent")?;
                assert_eq!(
                    metadata.decision,
                    if exhausted {
                        RetryDecision::Exhausted
                    } else {
                        RetryDecision::Scheduled
                    }
                );
                assert_eq!(metadata.run_state, b"queued");
                assert_eq!(metadata.next_run_at, due.map(TimestampMillis::new));
                let previous = scenario.record.clone();
                let history = scenario.history.clone();
                assert_eq!(
                    scenario.apply(now + 1, Operation::RetryWithPolicy(retry), None)?,
                    Outcome::LegacyNoop
                );
                assert_eq!(scenario.record, previous);
                assert_eq!(scenario.history, history);
                if !exhausted {
                    assert_eq!(
                        scenario.apply(now + 6, Operation::Claim(claim(b"queued")?), None)?,
                        Outcome::NotEligible
                    );
                    assert_eq!(scenario.record, previous);
                    assert_eq!(scenario.history, history);
                    assert_eq!(
                        scenario.apply(now + 7, Operation::Claim(claim(b"queued")?), None)?,
                        Outcome::Changed
                    );
                    version += 1;
                    fence += 1;
                    events += 1;
                    assert_eq!(scenario.record.state(), b"running");
                    assert_eq!(scenario.record.version(), version);
                    assert_eq!(scenario.record.fence().get(), fence);
                    assert_eq!(scenario.record.attempts(), attempt);
                    assert_eq!(scenario.history.len(), events);
                    now += 18;
                }
            }
        }
    }
    Ok(())
}

#[test]
fn generated_retry_deadline_survives_core_record_restoration() -> TestResult {
    let mut scenario = Scenario::claimed(b"restored-payment", b"queued")?;
    let retry = scenario.retry(configured(3, BackoffKind::Fixed, 100, b"failed"))?;
    scenario.apply(MAX_EXACT_MILLIS, Operation::RetryWithPolicy(retry), None)?;
    let record = &scenario.record;
    assert_eq!(
        record.next_run_at(),
        Some(TimestampMillis::new(MAX_EXACT_MILLIS + 100))
    );
    let event = scenario.history.last().ok_or("retry witness absent")?;
    let restored = restore_record(
        WorkflowRecordData {
            id: record.id().clone(),
            workflow_type: record.workflow_type().to_vec(),
            state: record.state().to_vec(),
            version: record.version(),
            fence: record.fence(),
            attempts: record.attempts(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
            next_run_at: record.next_run_at(),
            run_state: record.run_state().map(<[u8]>::to_vec),
            lease: record.lease().cloned(),
            payload: record.payload().map(<[u8]>::to_vec),
            result: record.result().map(<[u8]>::to_vec),
            error: record.error().map(<[u8]>::to_vec),
            latest_kind: event.kind,
            latest_version: event.version,
            latest_fence: event.fence,
        },
        wf_domain::workflow::admission::AdmissionLimits::default(),
    )?;
    assert_eq!(restored, scenario.record);
    Ok(())
}
