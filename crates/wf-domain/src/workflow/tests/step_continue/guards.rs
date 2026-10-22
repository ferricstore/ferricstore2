//! Runtime rejection and independent generated-token admission boundaries.

use super::*;
use crate::claim::ScopedRecordId;
use crate::value::InputField;
use crate::workflow::admission::{AdmissionError, AdmissionLimits};
use crate::workflow::batch::{
    BatchDecision, BatchItem, BatchRejection, GroupView, decide_group_batch,
};
use std::collections::BTreeMap;
use wf_types::identity::{GroupId, PartitionId};

#[test]
fn step_continue_rejects_guards_without_a_plan() -> TestResult {
    for case in 0..14 {
        let mut record = step_running()?;
        let mut command = step_command(&record)?;
        let Operation::StepContinue(step) = &mut command.operation else {
            return Err("step fixture absent".into());
        };
        let expected = match case {
            0 => {
                record.lease = None;
                DomainError::StaleLease
            }
            1 => {
                record.state = b"queued".to_vec();
                DomainError::StaleLease
            }
            2 => {
                step.guard.token = LeaseToken::new(b"wrong".to_vec())?;
                DomainError::StaleLease
            }
            3 => {
                step.guard.fence = Fence::new(0);
                DomainError::StaleLease
            }
            4 => {
                step.from = b"wrong".to_vec();
                step.worker = Some(b"other".to_vec());
                DomainError::WrongState
            }
            5 => {
                record
                    .lease
                    .as_mut()
                    .ok_or("fixture lease absent")?
                    .worker
                    .clear();
                DomainError::StaleLease
            }
            6 => {
                step.worker = Some(Vec::new());
                DomainError::StaleLease
            }
            7 => {
                step.worker = Some(b"other".to_vec());
                step.to = b"completed".to_vec();
                DomainError::StaleLease
            }
            8 => {
                step.to = b"completed".to_vec();
                DomainError::TerminalTransition
            }
            9 => {
                step.from.clear();
                DomainError::EmptyValue(InputField::From)
            }
            10 => {
                step.to.clear();
                DomainError::EmptyValue(InputField::To)
            }
            11 => {
                step.to = b"running".to_vec();
                DomainError::RunningTransition
            }
            12 => {
                step.guard.token = LeaseToken::new(b"wrong".to_vec())?;
                step.from = b"wrong".to_vec();
                DomainError::StaleLease
            }
            _ => {
                step.guard.token = LeaseToken::new(b"wrong".to_vec())?;
                step.to = b"running".to_vec();
                DomainError::RunningTransition
            }
        };
        let before = record.clone();
        assert_eq!(
            reduce(Some(&record), &command),
            Err(expected),
            "guard case {case}"
        );
        assert_eq!(record, before);
    }
    let record = step_running()?;
    assert_eq!(
        reduce(None, &step_command(&record)?),
        Err(DomainError::NotFound)
    );
    Ok(())
}

#[test]
fn step_continue_checks_numeric_boundaries() -> TestResult {
    let record = step_running()?;
    let before = record.clone();
    for (now, duration, fence, expected) in [
        (MAX_EXACT_MILLIS + 1, 1, 1, DomainError::NowTooLarge),
        (0, 0, 1, DomainError::ZeroLease),
        (0, MAX_EXACT_MILLIS + 1, 1, DomainError::LeaseTooLarge),
        (MAX_EXACT_MILLIS, 1, 1, DomainError::LeaseDeadlineTooLarge),
        (0, 30_000, MAX_EXACT_MILLIS + 1, DomainError::FenceTooLarge),
        (0, 30_000, u64::MAX, DomainError::FenceTooLarge),
    ] {
        let mut command = step_command(&record)?;
        command.now = TimestampMillis::new(now);
        let Operation::StepContinue(step) = &mut command.operation else {
            return Err("step fixture absent".into());
        };
        step.lease_ms = duration;
        step.guard.fence = Fence::new(fence);
        assert_eq!(reduce(Some(&record), &command), Err(expected));
        assert_eq!(record, before);
    }
    let mut extreme = record.clone();
    let command = step_command(&extreme)?;
    extreme.version = u64::MAX;
    let unchanged = extreme.clone();
    assert_eq!(
        reduce(Some(&extreme), &command),
        Err(DomainError::CounterOverflow(Counter::Version))
    );
    assert_eq!(extreme, unchanged);
    for (now, fence, logical, token) in [
        (0, 1, None, b"worker:0:2".as_slice()),
        (9, 9, Some(Vec::new()), b"worker:9:10".as_slice()),
        (
            10,
            99,
            Some(b"queued".to_vec()),
            b"worker:10:100".as_slice(),
        ),
    ] {
        let mut current = record.clone();
        current.fence = Fence::new(fence);
        current.run_state = logical;
        let mut command = step_command(&current)?;
        command.now = TimestampMillis::new(now);
        let Operation::StepContinue(step) = &mut command.operation else {
            return Err("step fixture absent".into());
        };
        step.from = b"queued".to_vec();
        let snapshot = current.clone();
        let plan = reduce(Some(&current), &command)?;
        let next = &plan.write().ok_or("numeric step write absent")?.record;
        assert_eq!(
            next.lease()
                .ok_or("numeric step lease absent")?
                .token
                .as_bytes(),
            token
        );
        assert_eq!(next.fence().get(), fence + 1);
        assert_eq!(current, snapshot);
    }
    Ok(())
}

#[test]
fn step_continue_admits_generated_token_before_owned_plan() -> TestResult {
    let mut record = step_running()?;
    let lease = record.lease.as_mut().ok_or("fixture lease absent")?;
    lease.worker = b"0123456789".to_vec();
    lease.token = LeaseToken::new(b"t".to_vec())?;
    let mut command = step_command(&record)?;
    command.now = TimestampMillis::new(2_100);
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let id = ScopedRecordId {
        partition_key: b"p",
        id: record.id().as_bytes(),
    };
    let canonical = BTreeMap::from([(id, &record)]);
    let view = GroupView {
        group: &group,
        placements: &placements,
        records: &canonical,
    };
    let items = [BatchItem::new(&partition, &command, b"counter-boundary")];
    let before = record.clone();
    let input = command.clone();
    // Actual input29 + current44 + key16 + partition1 + generated17 =107; charge1452.
    assert_eq!(
        (record.id().as_bytes().len(), record.workflow_type().len()),
        (16, 4)
    );
    for (inline, aggregate, expected) in [
        (
            16,
            4_096,
            Some(AdmissionError::ValueTooLarge {
                bytes: 17,
                maximum: 16,
            }),
        ),
        (17, 4_096, None),
        (17, 1_451, Some(AdmissionError::DecisionBytes)),
        (17, 1_452, None),
    ] {
        let limits = AdmissionLimits::new(inline, aggregate, 1)?;
        match (decide_group_batch(view, &items, b"", limits)?, expected) {
            (BatchDecision::Rejected(actual), Some(error)) => {
                assert_eq!(actual, BatchRejection::Admission(error));
            }
            (BatchDecision::Accepted(plan), None) => {
                assert_eq!(plan.items().len(), 1);
                let changed = plan.items()[0].plan();
                assert_eq!(changed.outcome(), Outcome::Changed);
                let next = &changed.write().ok_or("admitted step write absent")?.record;
                assert_eq!(
                    next.lease()
                        .ok_or("admitted lease absent")?
                        .token
                        .as_bytes(),
                    b"0123456789:2100:2"
                );
                assert_eq!(
                    changed.history().ok_or("admitted history absent")?.kind,
                    EventKind::StepContinued
                );
            }
            _ => return Err("generated-token admission returned unexpected decision".into()),
        }
        assert_eq!(record, before);
        assert_eq!(command, input);
    }
    Ok(())
}
