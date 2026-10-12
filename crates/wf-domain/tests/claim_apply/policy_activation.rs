//! Source-derived FIFO activation drain; proposed in-memory writes are carried.
use super::*;

#[test]
fn source_fifo_activation_waits_for_already_leased_lane_to_drain() -> TestResult {
    // Pinned invariant_test.exs:802–848, declaration be9fd6690bf99980.
    let kind = b"flow-invariant-fifo-activation-drain-type".as_slice();
    let partition = PartitionId::new(b"tenant:fifo-activation-drain".to_vec())?;
    let ids = [
        b"fifo-activation-drain-first".as_slice(),
        b"fifo-activation-drain-second".as_slice(),
    ];
    let mut rows = [
        test_record(ids[0], kind, b"queued", Some(ids[0].to_vec()), 1_000, 5_000)?,
        test_record(ids[1], kind, b"queued", Some(ids[1].to_vec()), 1_001, 2_000)?,
    ];
    let scope = test_scope()?;
    let (mut control, mut selected, stop) = activation_drain_claim(
        &rows,
        &partition,
        &BTreeMap::new(),
        &scope,
        test_control()?,
        (1, 2_000, b"parallel-worker"),
    )?;
    assert_eq!(selected.len(), 1);
    let running = selected
        .pop()
        .ok_or("source parallel claim has no record")?;
    assert_eq!(running.id().as_bytes(), ids[1]);
    assert_eq!(stop, SelectionStop::ClaimLimit);
    rows[1] = running;
    let patch = PolicyPatch {
        states: BTreeMap::from([(
            b"queued".to_vec(),
            StateChange::Patch(StatePolicyPatch {
                mode: FieldPatch::Set(StateMode::Fifo),
                ..StatePolicyPatch::default()
            }),
        )]),
        ..PolicyPatch::default()
    };
    let command = InstallCommand::low_level(kind.to_vec(), patch);
    let policy = prepare_install(None, 0, &command, PolicyLimits::default())?;
    let policies = BTreeMap::from([(kind, policy.policy())]);
    let (next, blocked, stop) = activation_drain_claim(
        &rows,
        &partition,
        &policies,
        &scope,
        control,
        (2, 5_000, b"fifo-worker"),
    )?;
    assert_eq!((blocked.len(), stop), (0, SelectionStop::Complete));
    control = next;
    let completion = plan_control(
        control,
        test_identity(3),
        EntryStamp::Stamped(test_stamp(5_100, 5_100)?),
    )?;
    let lease = rows[1]
        .lease()
        .ok_or("source running record has no lease")?;
    let terminal = CommittedCommand {
        id: rows[1].id().clone(),
        now: completion.execution_time(),
        operation: Operation::Complete(Terminal {
            guard: LeaseGuard {
                token: lease.token.clone(),
                fence: rows[1].fence(),
            },
            payload: None,
            value: None,
        }),
    };
    let (outcome, write, _) = reduce(Some(&rows[1]), &terminal)?.into_parts();
    assert_eq!(outcome, Outcome::Changed);
    rows[1] = write.ok_or("source completion has no record write")?.record;
    control = completion.next();
    let (_, selected, stop) = activation_drain_claim(
        &rows,
        &partition,
        &policies,
        &scope,
        control,
        (4, 5_101, b"fifo-worker"),
    )?;
    let first = selected
        .first()
        .ok_or("source drained FIFO claim is empty")?;
    assert_eq!((selected.len(), first.id().as_bytes()), (1, ids[0]));
    assert_eq!(
        (rows[1].state(), stop),
        (b"completed".as_slice(), SelectionStop::Complete)
    );
    Ok(())
}

fn activation_drain_claim(
    rows: &[WorkflowRecord; 2],
    partition: &PartitionId,
    policies: &BTreeMap<&[u8], &PolicySnapshot>,
    scope: &RequestScope,
    control: ControlState,
    step: (u64, u64, &[u8]),
) -> Result<(ControlState, Vec<WorkflowRecord>, SelectionStop), TestError> {
    let (index, now, worker) = step;
    let request = RequestId::new(index.to_be_bytes().to_vec())?;
    let outcomes = BTreeMap::new();
    let states = [b"queued".as_slice()];
    let partitions = [partition.clone()];
    let filter = [partition.as_bytes()];
    let entries = rows
        .iter()
        .enumerate()
        .map(|(sequence, record)| ClaimRecord {
            record,
            partition_key: partition.as_bytes(),
            priority: 0,
            state_enter_seq: 1 + sequence as u128,
        })
        .collect::<Vec<_>>();
    let query = ClaimQuery {
        partitions: PartitionFilter::Only(&filter),
        states: ClaimStates::Only(&states),
        worker,
        lease_ms: 30_000,
        ..test_query(rows[0].workflow_type(), 1)
    };
    with_claim_view(
        &entries,
        &partitions,
        GateView {
            scope,
            control,
            outcomes: &outcomes,
        },
        policies,
        index,
        |view| {
            let plan = decide_claim_request(
                view,
                &test_envelope(test_header(scope, &request, index, now)?, query),
                AdmissionLimits::default(),
                ClaimWorkLimits::default(),
            )?;
            let DispatchDecision::Fresh(ClaimDecision::Accepted(claim)) = plan.decision() else {
                return Err("source FIFO drain claim was not fresh acceptance".into());
            };
            let mut selected = Vec::new();
            if let Some(batch) = claim.batch() {
                for item in batch.batch().items() {
                    selected.push(
                        item.plan()
                            .write()
                            .ok_or("source claim has no write")?
                            .record
                            .clone(),
                    );
                }
            }
            Ok((plan.control().next(), selected, claim.stop()))
        },
    )
}
