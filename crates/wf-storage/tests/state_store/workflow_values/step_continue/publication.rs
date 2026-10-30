use super::{fixtures as f, *};
use wf_domain::history::EventKind;
use wf_domain::value::DomainError;
use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::IndexRejection;
use wf_ports::blob::WorkflowValueBindingKnowledge;

#[test]
fn start_and_claim_fixture_has_one_attempt_before_history_upgrade() -> TestResult {
    let directory = common::directory("step-start-attempt-control")?;
    let path = directory.join("state");
    let mut store = values::fresh(&path)??;
    f::seed(&mut store)?;
    let current = f::record(&mut store)?;
    assert_eq!(
        (
            current.record.version(),
            current.record.fence().get(),
            current.record.attempts()
        ),
        (1, 1, 1)
    );
    drop(store);
    assert!(values::raw_manifest(&path)?.starts_with(b"FWSS\0\x07"));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn durable_step_publishes_rotated_core_full_history_refs_and_original_reply() -> TestResult {
    let directory = common::directory("step-durable")?;
    let path = directory.join("state");
    let mut store = f::fresh(&path)?;
    f::seed(&mut store)?;
    let original = f::record(&mut store)?;
    let frame = f::frame(2, Some(b"\0\xff"), b"w:0:1")?;
    let outcome = f::retained(f::apply(&mut store, &frame, 1)?)?;
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert_eq!(
        (outcome.intent().version(), outcome.response_version()),
        (10, 9)
    );
    assert_eq!(outcome.original_position(), common::log(1));
    assert_eq!(
        (outcome.original_time().get(), outcome.expires_at().get()),
        (2, 102)
    );
    assert_eq!(
        step_continue_retained_summary(
            outcome.intent(),
            9,
            outcome.response(),
            common::log(1),
            TimestampMillis::new(2),
            f::limits().record_batch.codec.admission()
        )?,
        (RetainedStatus::Accepted, Some(1))
    );
    let current = f::record(&mut store)?;
    assert_eq!(
        (
            current.record.version(),
            current.record.fence().get(),
            current.record.attempts()
        ),
        (2, 2, 1)
    );
    assert_eq!(current.record.run_state(), Some(b"charge".as_slice()));
    assert_eq!(current.record.payload(), Some(b"\0\xff".as_slice()));
    let lease = current.record.lease().ok_or("rotated lease")?;
    assert_eq!(lease.token.as_bytes(), b"w:2:2");
    assert_eq!(lease.deadline.get(), 30_002);
    assert_eq!(current.ordering.state_enter_seq, 1_u128 << 32);
    assert_eq!(original.record.version(), 1);
    let page = f::history(&store)?;
    assert_eq!(page.entries.len(), 2);
    let row = &page.entries[1];
    assert_eq!(
        (row.event.event_id.as_str(), row.event.kind),
        ("2-2", EventKind::StepContinued)
    );
    let WorkflowValueBindingKnowledge::Known(Some(binding)) = &row.values else {
        return Err("captured history binding".into());
    };
    assert_eq!(binding.lifecycle.birth.retention_ttl_ms(), 7000);
    assert_eq!(binding.lifecycle.effective_ttl_ms, None);
    assert_eq!(binding.lifecycle.terminal_deadline, None);
    assert_eq!(binding.lifecycle.incarnation, 0);
    drop(store);
    assert_disk_snapshot(&path, &current, &row.event)?;
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn assert_disk_snapshot(
    path: &std::path::Path,
    current: &StoredWorkflowRecord,
    event: &wf_domain::history::HistoryEntry,
) -> TestResult {
    let rows = common::raw_rows(path)?;
    assert!(
        rows.iter()
            .filter(|(key, _)| key.get(6) == Some(&1))
            .all(|(_, bytes)| bytes.starts_with(b"FWSR\0\x02"))
    );
    let full: Vec<_> = rows
        .iter()
        .filter(|(key, _)| key.get(6) == Some(&5))
        .collect();
    assert_eq!(full.len(), 2);
    for (_, bytes) in full {
        assert!(bytes.starts_with(b"FWHS\0\x01"));
    }
    let physical = key::encode_history_key(
        &common::scope()?,
        &current.partition,
        current.record.id(),
        event,
        common::MIB,
    )?;
    let snapshot = value::decode_history_snapshot(
        rows.get(&physical).ok_or("Step physical history")?,
        &common::scope()?,
        &current.partition,
        current.record.id(),
        f::snapshot_limits()?,
    )?;
    assert_eq!(&snapshot.postimage, current);
    assert_eq!(&snapshot.event, event);
    let mut reopened = common::open(path)?;
    assert_eq!(&f::record(&mut reopened)?, current);
    assert_eq!(&f::history(&reopened)?.entries[1].event, event);
    drop(reopened);
    assert_eq!(common::raw_rows(path)?, rows);
    Ok(())
}

#[test]
fn durable_step_payload_presence_and_stale_rejection_preserve_atomic_units() -> TestResult {
    for payload in [None, Some(b"".as_slice()), Some(b"new".as_slice())] {
        let directory = common::directory("step-presence")?;
        let path = directory.join("state");
        let mut store = f::fresh(&path)?;
        f::seed(&mut store)?;
        let frame = f::frame(2, payload, b"w:0:1")?;
        assert_eq!(
            f::retained(f::apply(&mut store, &frame, 1)?)?.status(),
            RetainedStatus::Accepted
        );
        let current = f::record(&mut store)?;
        assert_eq!(current.record.payload(), Some(payload.unwrap_or(b"old")));
        let stale = f::frame_named(b"stale", 3, Some(b"unpublished"), b"w:0:1")?;
        let rejected = f::retained(f::apply(&mut store, &stale, 2)?)?;
        assert_eq!(
            rejected.status(),
            RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
                DomainError::StaleLease
            )))
        );
        assert_eq!(f::record(&mut store)?, current);
        let page = f::history(&store)?;
        assert_eq!(page.entries.len(), 2);
        let WorkflowValueBindingKnowledge::Known(Some(binding)) = &page.current_values else {
            return Err("current binding".into());
        };
        let wf_domain::workflow::active::ActiveValueRef::Versioned { version, .. } =
            binding.refs.payload.as_ref().ok_or("payload ref")?
        else {
            return Err("versioned payload".into());
        };
        assert_eq!(*version, if payload.is_some() { 2 } else { 1 });
        assert_eq!(binding.lifecycle.effective_ttl_ms, None);
        drop(store);
        let reopened = common::open(&path)?;
        assert_eq!(f::history(&reopened)?.entries.len(), 2);
        drop(reopened);
        std::fs::remove_dir_all(directory)?;
    }
    Ok(())
}

#[test]
fn durable_step_fifo_refusal_retains_only_outcome_and_control() -> TestResult {
    use wf_domain::policy::definition::{
        FieldPatch, PolicyPatch, StateChange, StateMode, StatePolicyPatch,
    };
    let directory = common::directory("step-fifo-refusal")?;
    let path = directory.join("state");
    let mut store = f::fresh(&path)?;
    f::seed(&mut store)?;
    let mut policy = PolicyPatch::default();
    policy.states.insert(
        b"charge".to_vec(),
        StateChange::Patch(StatePolicyPatch {
            mode: FieldPatch::Set(StateMode::Fifo),
            ..StatePolicyPatch::default()
        }),
    );
    values::install_complete(&mut store, &common::scope()?, policy, None, 1, 1)?;
    let before = f::record(&mut store)?;
    let outcome = f::retained(f::apply(&mut store, &f::frame(2, None, b"w:0:1")?, 2)?)?;
    assert_eq!(
        outcome.status(),
        RetainedStatus::IndexedRejected(IndexRejection::DirectRunningFifoEntry)
    );
    assert_eq!(f::record(&mut store)?, before);
    assert_eq!(f::history(&store)?.entries.len(), 1);
    drop(store);
    drop(common::open(&path)?);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
