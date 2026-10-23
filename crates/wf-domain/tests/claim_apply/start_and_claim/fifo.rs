//! Direct FIFO running entry is rejected without reserving a workflow identity.

use super::*;
use wf_domain::claim::{SelectionAction, SelectionRequest, StateFilter, select};

fn claim_start_source(
    scope: (&[u8], &[u8]),
    rows: &mut Records,
    fifo: &mut FifoMembers,
    parallel: &mut ParallelMembers,
    policy: &PolicySnapshot,
    step: (u64, u64),
) -> Result<(usize, Option<WorkflowRecord>), TestError> {
    let (now, index) = step;
    let partitions = [scope.1];
    let states = [b"queued".as_slice()];
    let request = SelectionRequest {
        partitions: PartitionFilter::Only(&partitions),
        states: StateFilter::Only(&states),
        limit: 1,
        reclaim_expired: true,
        ..SelectionRequest::new(b"g", scope.0, TimestampMillis::new(now))
    };
    let (count, selected) =
        with_published(scope, rows, fifo, parallel, policy, index - 1, |_, view| {
            let selection = select(&view, request, None)?;
            let count = selection.records().len();
            if count != 1 {
                return Ok((count, None));
            }
            let selected = selection.records()[0];
            if selected.partition_key != scope.1
                || selected.record.state() != b"queued"
                || selected.action != SelectionAction::Claim
            {
                return Err("source FIFO selected wrong physical scope/action".into());
            }
            let fence = selected
                .record
                .fence()
                .checked_next()
                .ok_or("claim fence overflow")?;
            Ok((count, Some((selected.record.id().clone(), fence))))
        })?;
    let Some((id, fence)) = selected else {
        return Ok((count, None));
    };
    let command = CommittedCommand {
        id,
        now: TimestampMillis::new(now),
        operation: Operation::Claim(Claim {
            expected_state: b"queued".to_vec(),
            worker: b"fifo-worker".to_vec(),
            token: LeaseToken::new(format!("fifo-worker:{now}:{}", fence.get()).into_bytes())?,
            lease_ms: 30_000,
        }),
    };
    let record = publish_start_source(scope, rows, fifo, parallel, policy, &command, index)?;
    if record.id() != &command.id || record.fence() != fence {
        return Err("source FIFO claim published a different identity/fence".into());
    }
    Ok((count, Some(record)))
}

#[test]
fn source_fifo_rejects_atomic_start_without_reserving_identity() -> TestResult {
    // invariant_test.exs1053–1100, declaration945df5266fddc8cb; policy1059 is fixture-only.
    let workflow_type = b"flow-invariant-fifo-direct-type:2".as_slice();
    let scope = (workflow_type, b"tenant:fifo-direct:1".as_slice());
    let id = b"fifo-direct:1".as_slice();
    let installed = source_start_policy(workflow_type, true)?;
    let mut rows = BTreeMap::new();
    let mut fifo = BTreeSet::new();
    let mut parallel = BTreeSet::new();
    let start = source_start_command(
        id,
        workflow_type,
        b"charge",
        b"fifo-worker",
        1_000,
        30_000,
        None,
    )?;
    let refused = reject_start_source(
        scope,
        &rows,
        &fifo,
        &parallel,
        installed.policy(),
        &start,
        1,
    )?;
    assert_eq!(refused, IndexRejection::DirectRunningFifoEntry);
    assert!(rows.is_empty() && fifo.is_empty() && parallel.is_empty());
    let create = CommittedCommand {
        id: start.id.clone(),
        now: TimestampMillis::new(1_000),
        operation: Operation::Create(Create {
            workflow_type: workflow_type.to_vec(),
            state: b"queued".to_vec(),
            run_at: Some(TimestampMillis::new(1_000)),
            payload: Some(b"payload".to_vec()),
        }),
    };
    let created = publish_start_source(
        scope,
        &mut rows,
        &mut fifo,
        &mut parallel,
        installed.policy(),
        &create,
        2,
    );
    assert!(created.is_ok(), "{created:?}");
    let created = created?;
    assert_eq!(created.id().as_bytes(), id);
    assert_eq!(rows[id].1.state_enter_seq, 2_u128 << 32);
    let (count, claimed) = claim_start_source(
        scope,
        &mut rows,
        &mut fifo,
        &mut parallel,
        installed.policy(),
        (1_000, 3),
    )?;
    assert_eq!(count, 1);
    let claimed = claimed.ok_or("source singleton claim has no record")?;
    assert_eq!(claimed.id().as_bytes(), id);
    let lease = claimed.lease().ok_or("source claimed lease absent")?;
    assert_eq!(lease.worker, b"fifo-worker");
    assert_eq!(lease.deadline.get(), 31_000);
    assert_eq!(claimed.payload(), Some(b"payload".as_slice()));
    assert_eq!(rows[id].1.state_enter_seq, 2_u128 << 32);
    let retained = (rows.clone(), fifo.clone(), parallel.clone());
    let step = CommittedCommand {
        id: claimed.id().clone(),
        now: TimestampMillis::new(1_100),
        operation: Operation::StepContinue(StepContinue {
            guard: LeaseGuard {
                token: lease.token.clone(),
                fence: claimed.fence(),
            },
            from: b"queued".to_vec(),
            to: b"charge".to_vec(),
            worker: Some(b"fifo-worker".to_vec()),
            lease_ms: 30_000,
            payload: None,
        }),
    };
    let refused =
        reject_start_source(scope, &rows, &fifo, &parallel, installed.policy(), &step, 4)?;
    assert_eq!(refused, IndexRejection::DirectRunningFifoEntry);
    assert_eq!(
        (&rows, &fifo, &parallel),
        (&retained.0, &retained.1, &retained.2)
    );
    assert_eq!(rows[id].1.state_enter_seq, 2_u128 << 32);
    Ok(())
}
