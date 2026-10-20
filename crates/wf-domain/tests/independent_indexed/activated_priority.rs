//! Entry restrictions depend on the actual changed operation after activation.

use super::*;

#[test]
fn activated_priority_rejects_changed_entry_commands_and_preserves_legacy_noop() -> TestResult {
    for priority in [1, 2] {
        assert_changed_fifo_entries(priority)?;
        assert_activated_legacy_noops(priority)?;
        assert_activated_terminal_mutations(priority)?;
    }
    Ok(())
}

fn assert_changed_fifo_entries(priority: u8) -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let policy = policy(&[(b"queued", StateMode::Fifo), (b"review", StateMode::Fifo)])?;
    let queued = initial(b"i", b"queued")?;
    let running = claimed(b"i")?;
    let mut same_state = transition(b"i", b"running", b"queued")?;
    let Operation::Transition(value) = &mut same_state.operation else {
        return Err("fixture is not a transition".into());
    };
    value.run_at = Some(TimestampMillis::new(3_000));
    value.payload = Some(vec![0, 255]);
    value.fence = running.fence();
    value.lease_token = Some(guard(&running)?.token);
    let policy_retry = command(
        b"i",
        Operation::RetryWithPolicy(PolicyRetry {
            guard: guard(&running)?,
            run_at: None,
            policy_override: RetryOverride::default(),
            jitter: JitterSample::new(0),
            payload: None,
            error: None,
        }),
    )?;
    let commands = [
        (None, create(b"new", b"queued")?),
        (Some(&queued), transition(b"i", b"queued", b"review")?),
        (Some(&running), same_state),
        (Some(&running), retry(&running, Some(vec![0, 255]))?),
        (Some(&running), policy_retry),
    ];
    for (current, command) in commands {
        let rows = current
            .map(|record| entry(&partition, record, priority))
            .into_iter()
            .collect::<Vec<_>>();
        let plan = accepted(decide(
            &rows,
            &[(&partition, &command, priority)],
            Some(policy.policy()),
            None,
            AdmissionLimits::default(),
        )?)?;
        assert_eq!(plan.results().len(), 1);
        assert_eq!(plan.results()[0], Err(IndexRejection::FifoPriority));
        assert_eq!(plan.final_indices().len(), 0);
        assert_eq!(plan.history_plans().count(), 0);
        assert_eq!(plan.consumed_ordinals(), 0);
        if let Some(current) = current {
            assert_eq!(
                current.version(),
                if current.state() == b"running" { 2 } else { 1 }
            );
        }
    }
    Ok(())
}

fn assert_activated_legacy_noops(priority: u8) -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let policy = policy(&[(b"queued", StateMode::Fifo)])?;
    let queued = initial(b"i", b"queued")?;
    let same_state = transition(b"i", b"queued", b"queued")?;
    let running = claimed(b"i")?;
    let retry_command = retry(&running, None)?;
    let retried = reduce(Some(&running), &retry_command)?
        .into_parts()
        .1
        .ok_or("retry fixture absent")?
        .record;
    let complete_command = complete(&running)?;
    let completed = reduce(Some(&running), &complete_command)?
        .into_parts()
        .1
        .ok_or("terminal fixture absent")?
        .record;
    for (record, command) in [
        (&retried, &retry_command),
        (&completed, &complete_command),
        (&queued, &same_state),
    ] {
        let plan = accepted(decide(
            &[entry(&partition, record, priority)],
            &[(&partition, command, priority)],
            Some(policy.policy()),
            None,
            AdmissionLimits::default(),
        )?)?;
        let noop = success(plan.results(), 0)?.plan();
        assert_eq!(noop.outcome(), Outcome::LegacyNoop);
        assert!(noop.write().is_none() && noop.history().is_none());
        assert_eq!(plan.final_indices().len(), 0);
        assert_eq!(plan.consumed_ordinals(), 0);
    }
    Ok(())
}

fn assert_activated_terminal_mutations(priority: u8) -> TestResult {
    let partition = PartitionId::new(b"p".to_vec())?;
    let policy = policy(&[(b"queued", StateMode::Fifo)])?;
    let running = claimed(b"i")?;
    let commands = [
        complete(&running)?,
        command(
            b"i",
            Operation::Fail(Terminal {
                guard: guard(&running)?,
                payload: None,
                value: Some(vec![255]),
            }),
        )?,
        command(
            b"i",
            Operation::Cancel(Cancel {
                fence: running.fence(),
                lease_token: Some(guard(&running)?.token),
                expected_state: Some(b"running".to_vec()),
                expected_version: Some(2),
                error: None,
            }),
        )?,
    ];
    for (index, command) in commands.iter().enumerate() {
        let plan = accepted(decide(
            &[entry(&partition, &running, priority)],
            &[(&partition, command, priority)],
            Some(policy.policy()),
            None,
            AdmissionLimits::default(),
        )?)?;
        let write = write(success(plan.results(), 0)?)?;
        assert_eq!(
            write.record.state(),
            [b"completed".as_slice(), b"failed", b"cancelled"][index]
        );
        assert_eq!(
            (
                write.record.version(),
                write.record.fence().get(),
                write.record.attempts()
            ),
            (3, 1, 0)
        );
        let metadata = plan.metadata().next().ok_or("terminal ordering absent")?;
        assert_eq!(
            (metadata.priority, metadata.state_enter_seq),
            (priority, 99)
        );
        let delta = plan.deltas().next().ok_or("terminal delta absent")??;
        assert!(matches!(delta.before, Some(IndexRows::Fifo { .. })) && delta.after.is_none());
        assert_eq!(plan.history_plans().count(), 1);
        assert_eq!(plan.consumed_ordinals(), 0);
    }
    Ok(())
}
