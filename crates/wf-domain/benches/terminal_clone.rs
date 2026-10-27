//! Bounded terminal-decision smoke and explicit release-only measurements.
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::error::Error;
use std::hint::black_box;
use std::time::{Duration, Instant};

use wf_domain::claim::ScopedRecordId;
use wf_domain::history::EventKind;
use wf_domain::lease::{Lease, LeaseGuard, LeaseToken};
use wf_domain::value::{Counter, DomainError};
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::{
    BatchDecision, BatchItem, BatchRejection, GroupView, decide_group_batch,
};
use wf_domain::workflow::restoration::{WorkflowRecordData, restore_record};
use wf_domain::workflow::{CommittedCommand, Operation, Outcome, Terminal, WorkflowRecord, reduce};
use wf_types::identity::{GroupId, PartitionId, RecordId};
use wf_types::version::{Fence, TimestampMillis};

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let measure = match (arguments.next(), arguments.next()) {
        (None, None) => false,
        (Some(flag), None) if flag == "--test" || flag == "--bench" => false,
        (Some(flag), None) if flag == "--measure" => {
            if cfg!(debug_assertions) {
                return Err("measurement requires the recorded optimized bench build".into());
            }
            true
        }
        _ => return Err("usage: terminal_clone [--test | --bench | --measure]".into()),
    };
    run_case(8, false)?;
    run_case(9, false)?;
    if measure {
        println!("terminal_clone: measurement requires recorded bench-profile provenance");
    }
    for profile in 0..8 {
        run_case(profile, measure)?;
    }
    if !measure {
        println!("terminal_clone smoke: ten profiles validated twice");
    }
    Ok(())
}

fn with_case(
    profile: usize,
    run: impl for<'view> FnOnce(
        GroupView<'view>,
        &'view [BatchItem<'view>],
        &'view WorkflowRecord,
        &'view CommittedCommand,
    ) -> Result<(), Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    if profile > 9 {
        return Err("unsupported bounded terminal profile".into());
    }
    let fail = matches!(profile, 1 | 9);
    let small = profile == 3;
    let version = if profile == 7 { u64::MAX } else { 2 };
    let mut current = restore_record(
        WorkflowRecordData {
            id: RecordId::new(b"terminal-bench".to_vec())?,
            workflow_type: b"job".to_vec(),
            state: b"running".to_vec(),
            version,
            fence: Fence::new(1),
            attempts: 0,
            created_at: TimestampMillis::new(1_000),
            updated_at: TimestampMillis::new(2_000),
            next_run_at: Some(TimestampMillis::new(2_100)),
            run_state: Some(b"queued".to_vec()),
            lease: Some(Lease {
                worker: vec![0xFE; 256],
                token: LeaseToken::new(vec![0xFD; 256])?,
                deadline: TimestampMillis::new(2_100),
            }),
            payload: (!small).then(|| vec![0xA1; 65_536]),
            result: if profile == 9 {
                Some(vec![0xE7; 32])
            } else if !fail && !small {
                Some(vec![0xA2; 65_536])
            } else {
                None
            },
            error: if profile == 8 {
                Some(vec![0xEC; 32])
            } else if fail {
                Some(vec![0xA2; 65_536])
            } else {
                None
            },
            latest_kind: EventKind::Claimed,
            latest_version: version,
            latest_fence: Fence::new(1),
        },
        AdmissionLimits::default(),
    )?;
    let command = make_command(profile, &current)?;
    if profile == 6 {
        // One untimed setup reduction produces the real duplicate predecessor.
        let (_, write, _) = reduce(Some(&current), &command)?.into_parts();
        current = write
            .ok_or("legacy setup produced no completed record")?
            .record;
    }
    let group = GroupId::new(b"g".to_vec())?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition.clone(), group.clone())]);
    let records = BTreeMap::from([(
        ScopedRecordId {
            partition_key: partition.as_bytes(),
            id: current.id().as_bytes(),
        },
        &current,
    )]);
    let items = [BatchItem::new(&partition, &command, command.id.as_bytes())];
    run(
        GroupView {
            group: &group,
            placements: &placements,
            records: &records,
        },
        &items,
        &current,
        &command,
    )
}

fn make_command(
    profile: usize,
    current: &WorkflowRecord,
) -> Result<CommittedCommand, Box<dyn Error>> {
    let lease = current.lease().ok_or("terminal fixture has no lease")?;
    let terminal = Terminal {
        guard: LeaseGuard {
            token: if profile == 4 {
                LeaseToken::new(vec![0xFC; 256])?
            } else {
                lease.token.clone()
            },
            fence: current.fence(),
        },
        payload: match profile {
            2 => None,
            3 => Some(Vec::new()),
            _ => Some(vec![0xB1; 16]),
        },
        value: match profile {
            2 => None,
            3 => Some(Vec::new()),
            _ => Some(vec![0xB2; 16]),
        },
    };
    Ok(CommittedCommand {
        id: current.id().clone(),
        now: TimestampMillis::new(2_050),
        operation: if matches!(profile, 1 | 9) {
            Operation::Fail(terminal)
        } else {
            Operation::Complete(terminal)
        },
    })
}

fn validate_decision(
    decision: &BatchDecision,
    current: &WorkflowRecord,
    command: &CommittedCommand,
    profile: usize,
) -> Result<(), Box<dyn Error>> {
    let version_error = DomainError::CounterOverflow(Counter::Version);
    let rejection = match profile {
        4 => Some(BatchRejection::Domain(DomainError::StaleLease)),
        5 => Some(BatchRejection::Admission(AdmissionError::DecisionBytes)),
        7 => Some(BatchRejection::Domain(version_error)),
        _ => None,
    };
    if let Some(reason) = rejection {
        assert_eq!(decision, &BatchDecision::Rejected(reason));
        return Ok(());
    }
    let BatchDecision::Accepted(plan) = decision else {
        return Err("accepted terminal profile was rejected".into());
    };
    assert_eq!(plan.items().len(), 1);
    let item = &plan.items()[0];
    assert_eq!(item.partition().as_bytes(), b"p");
    assert_eq!(current.id(), &command.id);
    assert_eq!(command.now.get(), 2_050);
    let fail = matches!(profile, 1 | 9);
    let (kind, state) = if fail {
        (EventKind::Failed, b"failed".as_slice())
    } else {
        (EventKind::Completed, b"completed".as_slice())
    };
    let record = if profile == 6 {
        assert_eq!(item.plan().outcome(), Outcome::LegacyNoop);
        assert!(item.plan().write().is_none() && item.plan().history().is_none());
        current
    } else {
        assert_eq!(item.plan().outcome(), Outcome::Changed);
        let write = item.plan().write().ok_or("terminal write absent")?;
        assert_eq!(write.expected_version, Some(2));
        let event = item.plan().history().ok_or("terminal history absent")?;
        assert_eq!(event.event_id, "2050-3");
        assert_eq!(event.kind, kind);
        assert_eq!(event.occurred_at.get(), 2_050);
        let scalars = (event.version, event.fence.get(), event.attempts);
        assert_eq!(scalars, (3, 1, 0));
        assert_eq!(event.state.as_slice(), state);
        assert_eq!((event.next_run_at, event.lease_deadline.get()), (None, 0));
        assert!(event.worker.is_none() && event.retry.is_none());
        &write.record
    };
    validate_terminal_record(record, kind, state);
    validate_values(record, profile, fail)
}

fn validate_values(
    record: &WorkflowRecord,
    profile: usize,
    fail: bool,
) -> Result<(), Box<dyn Error>> {
    let payload = record.payload().ok_or("terminal payload presence lost")?;
    let (length, byte) = match profile {
        2 => (65_536, 0xA1),
        3 => (0, 0),
        _ => (16, 0xB1),
    };
    assert_eq!(payload.len(), length);
    assert!(payload.iter().all(|actual| *actual == byte));
    let (target, opposite) = if fail {
        (record.error(), record.result())
    } else {
        (record.result(), record.error())
    };
    if profile == 2 {
        assert_eq!(target, None);
    } else {
        let value = target.ok_or("terminal target presence lost")?;
        assert_eq!(value.len(), if profile == 3 { 0 } else { 16 });
        assert!(value.iter().all(|actual| *actual == 0xB2));
    }
    if matches!(profile, 8 | 9) {
        let value = opposite.ok_or("opposite terminal value lost")?;
        assert_eq!(value.len(), 32);
        let byte = if fail { 0xE7 } else { 0xEC };
        assert!(value.iter().all(|actual| *actual == byte));
    } else {
        assert_eq!(opposite, None);
    }
    Ok(())
}

fn sample_decisions(
    view: GroupView<'_>,
    items: &[BatchItem<'_>],
    limits: AdmissionLimits,
    profile: usize,
    iterations: usize,
) -> Result<Duration, Box<dyn Error>> {
    if profile > 7 || iterations == 0 {
        return Err("invalid timed terminal profile or iteration count".into());
    }
    let version_error = DomainError::CounterOverflow(Counter::Version);
    let rejection = match profile {
        4 => Some(BatchRejection::Domain(DomainError::StaleLease)),
        5 => Some(BatchRejection::Admission(AdmissionError::DecisionBytes)),
        7 => Some(BatchRejection::Domain(version_error)),
        _ => None,
    };
    let outcome = if profile == 6 {
        Outcome::LegacyNoop
    } else {
        Outcome::Changed
    };
    let started = Instant::now();
    for _ in 0..iterations {
        let decision = decide_group_batch(
            black_box(view),
            black_box(items),
            black_box(b"terminal-bench-entry".as_slice()),
            black_box(limits),
        )?;
        let expected = match (&decision, rejection) {
            (BatchDecision::Rejected(actual), Some(reason)) => *actual == reason,
            (BatchDecision::Accepted(plan), None) => {
                plan.items().len() == 1
                    && plan
                        .items()
                        .first()
                        .is_some_and(|item| item.plan().outcome() == outcome)
            }
            _ => false,
        };
        if !expected {
            return Err("unexpected timed terminal decision; discard this measurement".into());
        }
        drop(black_box(decision));
    }
    Ok(started.elapsed())
}

fn run_case(profile: usize, measure: bool) -> Result<(), Box<dyn Error>> {
    if profile > 9 || (measure && profile > 7) {
        return Err("invalid terminal profile measurement".into());
    }
    let names = [
        "complete_replace",
        "fail_replace",
        "complete_preserve",
        "complete_empty",
        "stale_token",
        "decision_bytes",
        "legacy_duplicate",
        "version_overflow",
        "complete_opposite",
        "fail_opposite",
    ];
    let defaults = AdmissionLimits::default();
    let limits = if profile == 5 {
        // Record131614 + command/key/partition317 gives 4V+1024 = 528748.
        AdmissionLimits::new(defaults.inline_bytes(), 528_747, defaults.batch_items())?
    } else {
        defaults
    };
    with_case(profile, |view, items, current, command| {
        let current_before = current.clone();
        let command_before = command.clone();
        let before = decide_group_batch(view, items, b"terminal-bench-entry", limits)?;
        validate_decision(&before, current, command, profile)?;
        drop(before);
        if measure {
            let iterations = [512, 512, 512, 4_096, 4_096, 4_096, 4_096, 512][profile];
            let _warmup = sample_decisions(view, items, limits, profile, iterations)?;
            for sample in 0..16 {
                let elapsed = sample_decisions(view, items, limits, profile, iterations)?;
                print_sample(names[profile], sample, iterations, elapsed)?;
            }
        }
        let after = decide_group_batch(view, items, b"terminal-bench-entry", limits)?;
        validate_decision(&after, current, command, profile)?;
        drop(after);
        assert_eq!(current, &current_before);
        assert_eq!(command, &command_before);
        Ok(())
    })
}

fn print_sample(
    name: &str,
    sample: usize,
    iterations: usize,
    elapsed: Duration,
) -> Result<(), Box<dyn Error>> {
    if elapsed.is_zero() {
        return Err("zero-duration terminal sample; discard this measurement".into());
    }
    println!(
        "profile={} sample={sample} iterations={iterations} elapsed_ns={}",
        name,
        elapsed.as_nanos()
    );
    Ok(())
}

fn validate_terminal_record(record: &WorkflowRecord, kind: EventKind, state: &[u8]) {
    assert_eq!(record.id().as_bytes(), b"terminal-bench");
    assert_eq!(record.workflow_type(), b"job");
    assert_eq!(record.state(), state);
    let scalars = (record.version(), record.fence().get(), record.attempts());
    assert_eq!(scalars, (3, 1, 0));
    let times = (record.created_at().get(), record.updated_at().get());
    assert_eq!(times, (1_000, 2_050));
    assert_eq!(record.next_run_at(), None);
    assert_eq!(record.run_state(), Some(b"queued".as_slice()));
    assert!(record.lease().is_none());
    assert_eq!(record.latest_event_witness(), (kind, 3, Fence::new(1)));
}
