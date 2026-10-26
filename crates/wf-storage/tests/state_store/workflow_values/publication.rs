//! Same consumed transaction publishes current and every intermediate owner.

use wf_domain::lease::LeaseGuard;
use wf_domain::retention::RetentionOverride;
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::active::{LifecycleOverrides, MaxActiveOverride};
use wf_domain::workflow::caller::PublicMutationEvidence;
use wf_domain::workflow::request::{RequestScope, RetainedStatus};
use wf_domain::workflow::{Cancel, Create, Operation, Transition};
use wf_ports::blob::{
    OriginalValuePartition, OriginalValueRefKind, WorkflowValueReadBatch, render_original_value_ref,
};
use wf_ports::consensus::PreparedOperation;
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
use wf_storage::state::RedbStateStore;
use wf_storage::transaction::RecordBatchTransactionOutcome;
use wf_types::identity::{PartitionId, RecordId, RequestId};
use wf_types::version::{Fence, HlcTimestamp, TimestampMillis};

use super::{TestResult, fixtures as f};

#[test]
fn value_binary_commit_publishes_catalog_current_and_intermediate_history() -> TestResult {
    let directory = crate::fixtures::directory("value-every-history")?;
    let path = directory.join("state");
    let fresh = f::fresh(&path)?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let Ok(mut store) = fresh else {
        return Err("expected fresh value capability".into());
    };
    let scope = crate::fixtures::scope()?;
    let mut items = Vec::new();
    for (id, payload) in [
        (b"i".as_slice(), b"A".as_slice()),
        (b"i", b"rejected"),
        (b"j", b"C"),
    ] {
        let create = Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: None,
            payload: Some(payload.to_vec()),
        });
        items.push(f::item(id, 100, create)?);
    }
    let transition = Operation::Transition(Transition {
        from: b"q".to_vec(),
        to: b"next".to_vec(),
        fence: Fence::new(0),
        lease_token: None,
        run_at: None,
        payload: Some(b"B".to_vec()),
    });
    items.insert(2, f::item(b"i", 100, transition)?);
    let inputs = (0..4).map(|index| f::input(index, 0)).collect();
    let prepared = f::batch(
        6,
        b"intermediates",
        100,
        PreparedOperation::Batch(items),
        inputs,
        vec![f::witness(b"r", None)?],
    )?;
    let applied = f::apply_value(&mut store, &scope, &prepared, 0)?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = applied else {
        return Err("expected consumed retained result".into());
    };
    assert_eq!(outcome.status(), RetainedStatus::Accepted);
    assert!(matches!(
        f::apply_value(&mut store, &scope, &prepared, 0)?,
        RecordBatchTransactionOutcome::LogReplay(_)
    ));
    let partition = PartitionId::new(b"p".to_vec())?;
    for (id, event, incarnation, version) in [
        (b"i".as_slice(), None, 0, 2),
        (b"i", Some("100-1"), 0, 1),
        (b"i", Some("100-2"), 0, 2),
        (b"j", None, 2, 1),
        (b"j", Some("100-1"), 2, 1),
    ] {
        let id = RecordId::new(id.to_vec())?;
        let binding =
            store.workflow_value_binding(&scope, &partition, &id, event, f::read_limits());
        assert!(binding.is_ok(), "{:?}", binding.as_ref().err());
        f::assert_binding(
            &binding?,
            (b"p", id.as_bytes()),
            incarnation,
            [Some(version), None, None],
            (None, 7_000, 2, 10, Some(7_000), None),
            &prepared.policies[0],
        )?;
    }
    let values = read_intermediate_values(&store, &scope)?;
    drop((prepared, outcome, store));
    f::assert_read(
        &values,
        &[b"A", b"B", b"C"],
        &[Some(0), Some(1), Some(2), Some(0)],
    )?;
    let rows = crate::fixtures::raw_rows(&path)?;
    for (family, expected) in [(8, 3), (9, 3), (10, 2), (11, 3)] {
        let mut prefix = b"FWPK\0\x01".to_vec();
        prefix.push(family);
        assert_eq!(
            rows.keys().filter(|key| key.starts_with(&prefix)).count(),
            expected,
        );
    }
    assert_ne!(f::raw_manifest(&path)?, Vec::<u8>::new());
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn value_binary_binding_actions_preserve_equal_empty_and_omitted_values() -> TestResult {
    let directory = crate::fixtures::directory("value-presence-actions")?;
    let path = directory.join("state");
    let fresh = f::fresh(&path)?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let Ok(mut store) = fresh else {
        return Err("expected fresh value capability".into());
    };
    let scope = crate::fixtures::scope()?;
    for case in 0..4 {
        assert_action(&mut store, &scope, case)?;
    }
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    for (family, expected) in [(8, 7), (9, 7), (10, 4), (11, 8)] {
        let mut prefix = b"FWPK\0\x01".to_vec();
        prefix.push(family);
        assert_eq!(
            rows.keys().filter(|key| key.starts_with(&prefix)).count(),
            expected,
        );
    }
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn value_binary_birth_and_terminal_ttl_overrides_survive_replay() -> TestResult {
    let directory = crate::fixtures::directory("value-captured-overrides")?;
    let path = directory.join("state");
    let fresh = f::fresh(&path)?;
    assert!(fresh.is_ok(), "{:?}", fresh.as_ref().err());
    let Ok(mut store) = fresh else {
        return Err("expected fresh value capability".into());
    };
    let scope = crate::fixtures::scope()?;
    let original = f::original_policy(1)?;
    f::install_original(&mut store, &scope, original.encoded(), None, 0, 10)?;
    let partition = PartitionId::new(b"p".to_vec())?;
    for case in 0..4 {
        let id = [b"inherit".as_slice(), b"command", b"zero", b"negative"][case];
        let state = if case == 2 { b"other".as_slice() } else { b"q" };
        let at = [1, 3, 5, 7][case];
        let donor = f::load_policy(&mut store, &scope)?.ok_or("installed policy missing")?;
        let mut input = f::input(0, 0);
        input.birth = (case == 1).then_some(LifecycleOverrides {
            active: MaxActiveOverride::Infinite,
            retention: RetentionOverride {
                ttl_ms: Some(44_000),
                history_max_events: Some(50),
            },
            history_hot_max_events: Some(0),
        });
        let create = Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: state.to_vec(),
            run_at: None,
            payload: Some(b"birth".to_vec()),
        });
        let mut request = f::batch(
            5,
            format!("birth-{case}").as_bytes(),
            200,
            PreparedOperation::Batch(vec![f::item(id, 200, create)?]),
            vec![input],
            vec![f::witness(b"r", Some(&donor))?],
        )?;
        f::apply_value(&mut store, &scope, &request, at)?;
        let cancel = Operation::Cancel(Cancel {
            fence: Fence::new(0),
            lease_token: None,
            expected_state: Some(state.to_vec()),
            expected_version: Some(1),
            error: Some(Vec::new()),
        });
        request.operation = PreparedOperation::PublicMutation {
            item: Box::new(f::item(id, 150, cancel)?),
            evidence: PublicMutationEvidence {
                requested_now_ms: Some(150),
                auto_partition: false,
                retry_override_present: false,
            },
        };
        request.profile = 1;
        request.request = RequestId::new(format!("terminal-{case}").into_bytes())?;
        request.inputs[0].birth = None;
        request.inputs[0].requested_at = Some(TimestampMillis::new(150));
        request.inputs[0].terminal_ttl_ms = [None, Some(33_000), Some(0), Some(-1)][case];
        f::apply_value(&mut store, &scope, &request, at + 1)?;
        assert!(matches!(
            f::apply_value(&mut store, &scope, &request, at + 1)?,
            RecordBatchTransactionOutcome::LogReplay(_)
        ));
        let expected = match case {
            0 => (Some(5_000), 22_000, 2, 40, Some(22_000), Some(22_200)),
            1 => (None, 44_000, 0, 50, Some(33_000), Some(33_200)),
            2 => (Some(5_000), 11_000, 2, 30, None, None),
            3 => (Some(5_000), 22_000, 2, 40, None, None),
            _ => return Err("unexpected lifecycle scenario".into()),
        };
        let id = RecordId::new(id.to_vec())?;
        let current = store.workflow_value_binding(&scope, &partition, &id, None, f::read_limits());
        assert!(current.is_ok(), "{:?}", current.as_ref().err());
        let current = current?;
        drop((request.operation, donor));
        let incarnation = u128::from(at) << 32;
        f::assert_binding(
            &current,
            (b"p", id.as_bytes()),
            incarnation,
            [Some(1), None, Some(2)],
            expected,
            &request.policies[0],
        )?;
    }
    drop((original, store));
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn read_intermediate_values(
    store: &RedbStateStore,
    scope: &RequestScope,
) -> TestResult<WorkflowValueReadBatch> {
    let partition = OriginalValuePartition::Explicit(b"p");
    let references = [(b"i".as_slice(), 1), (b"i", 2), (b"j", 1)]
        .into_iter()
        .map(|(id, version)| {
            render_original_value_ref(
                id,
                OriginalValueRefKind::Payload,
                version,
                partition,
                65_535,
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("value reference fixture: {error:?}"))?;
    let refs = [
        references[0].as_slice(),
        references[1].as_slice(),
        references[2].as_slice(),
        references[0].as_slice(),
    ];
    let mut clock = f::ValueReadClock {
        hlc: HlcTimestamp::new(100, 0)?,
        wall: TimestampMillis::new(100),
        calls: 0,
        failure: None,
    };
    let read = store.read_workflow_value_batch(&mut clock, scope, &refs, f::read_limits());
    assert!(read.is_ok(), "{:?}", read.as_ref().err());
    assert_eq!(clock.calls, 1);
    Ok(read?)
}

fn running_guard(
    store: &mut RedbStateStore,
    scope: &RequestScope,
    id: &RecordId,
) -> TestResult<LeaseGuard> {
    let limits = f::limits().record_batch;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        limits.codec.admission(),
        limits.maximum_read_steps,
        limits.maximum_read_bytes,
        limits.maximum_adapter_bytes,
    )?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let observed = tx
        .record(scope.tenant(), &partition, id)?
        .ok_or("running donor missing")?;
    assert_eq!(observed.record.state(), b"running");
    assert_eq!(observed.record.fence(), Fence::new(1));
    let token = observed
        .record
        .lease()
        .ok_or("running lease missing")?
        .token
        .clone();
    assert_ne!(token.as_bytes(), b"");
    drop(tx);
    Ok(LeaseGuard {
        token,
        fence: Fence::new(1),
    })
}

fn assert_action(store: &mut RedbStateStore, scope: &RequestScope, case: u8) -> TestResult {
    use wf_domain::workflow::{StartAndClaim, Terminal};
    let replacement_versions = [Some(2), Some(2), None];
    let (id, at, now, versions, deadline) = match case {
        0 => (b"i".as_slice(), 0, 100, replacement_versions, Some(7_101)),
        1 => (b"j".as_slice(), 2, 300, [Some(1), None, None], Some(7_301)),
        2 => (
            b"k".as_slice(),
            4,
            500,
            [Some(1), None, Some(2)],
            Some(7_501),
        ),
        3 => (b"l".as_slice(), 6, 700, [Some(1), None, None], None),
        _ => return Err("unexpected value action scenario".into()),
    };
    let start = Operation::StartAndClaim(StartAndClaim {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        worker: b"w".to_vec(),
        lease_ms: 1_000,
        payload: Some(b"A".to_vec()),
    });
    let prepared = f::batch(
        7,
        format!("start-{case}").as_bytes(),
        now,
        PreparedOperation::Batch(vec![f::item(id, now, start)?]),
        vec![f::input(0, 0)],
        vec![f::witness(b"r", None)?],
    )?;
    f::apply_value(store, scope, &prepared, at)?;
    let id = RecordId::new(id.to_vec())?;
    let guard = running_guard(store, scope, &id)?;
    let terminal = Terminal {
        guard: guard.clone(),
        payload: (case == 0).then(|| b"A".to_vec()),
        value: match case {
            0 => Some(Vec::new()),
            2 => Some(b"E".to_vec()),
            _ => None,
        },
    };
    let action = match case {
        0 | 1 => Operation::Complete(terminal),
        2 => Operation::Fail(terminal),
        3 => Operation::Retry(Retry {
            guard: guard.clone(),
            run_at: RetryTime::Explicit(TimestampMillis::new(now + 10)),
            payload: None,
            error: None,
        }),
        _ => return Err("unexpected action class".into()),
    };
    let mut items = vec![f::item(id.as_bytes(), now + 1, action)?];
    if case < 2 {
        let duplicate = Operation::Complete(Terminal {
            guard,
            payload: None,
            value: None,
        });
        items.push(f::item(id.as_bytes(), now + 1, duplicate)?);
    }
    let inputs = (0..u32::try_from(items.len())?)
        .map(|index| f::input(index, 0))
        .collect();
    let terminal = f::batch(
        6,
        format!("action-{case}").as_bytes(),
        now + 1,
        PreparedOperation::Batch(items),
        inputs,
        vec![f::witness(b"r", None)?],
    )?;
    f::apply_value(store, scope, &terminal, at + 1)?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let start_event = format!("{now}-1");
    let end_event = format!("{}-2", now + 1);
    for (event, expected, deadline) in [
        (None, versions, deadline),
        (Some(start_event.as_str()), [Some(1), None, None], None),
        (Some(end_event.as_str()), versions, deadline),
    ] {
        let binding = store.workflow_value_binding(scope, &partition, &id, event, f::read_limits());
        assert!(binding.is_ok(), "{:?}", binding.as_ref().err());
        f::assert_binding(
            &binding?,
            (b"p", id.as_bytes()),
            u128::from(at) << 32,
            expected,
            (None, 7_000, 2, 10, Some(7_000), deadline),
            &terminal.policies[0],
        )?;
    }
    Ok(())
}
