//! Actual mode, field, mixed-history and retained identity contracts.

use wf_domain::lease::LeaseGuard;
use wf_domain::workflow::request::RetainedStatus;
use wf_domain::workflow::{Cancel, Terminal, Transition};
use wf_ports::blob::OriginalValueRefKind::{Error, Payload, Result as ResultValue};
use wf_ports::log::{encode_workflow_stored_value_batch, encode_workflow_value_batch};
use wf_storage::transaction::{
    WorkflowValueTransactionError, apply_workflow_value_batch_transaction,
};
use wf_types::version::Fence;

use super::*;

#[test]
fn stored_publication_preserves_exact_values_in_all_modes() -> TestResult {
    let directory = crate::fixtures::directory("stored-publish-modes")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let values = [
        b"raw".as_slice(),
        b"FSV2\x01FSV2raw",
        b"FSV2\x02\x83\x61\x07",
        b"FSV2\x02bad-etf",
        b"",
    ];
    let mut at = 0;
    for mode in [1, 5, 6, 7] {
        for (index, value) in values.into_iter().enumerate() {
            let id = format!("mode-{mode}-{index}");
            let request = create(mode, id.as_bytes(), 100, Some(value))?;
            let applied = apply(&mut store, &request, at);
            assert!(
                applied.is_ok(),
                "healthy stored publication: {:?}",
                applied.as_ref().err()
            );
            let outcome = retained(applied?)?;
            assert_eq!(outcome.intent().version(), 9);
            assert_eq!(outcome.status(), RetainedStatus::Accepted);
            assert_eq!(
                outcome.response_version(),
                if mode == 1 { 8 } else { u16::from(mode) }
            );
            assert_value(&store, id.as_bytes(), Payload, 1, value)?;
            at += 1;
        }
    }
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    for family in [2, 5, 8, 9, 10, 11] {
        assert_eq!(
            rows.keys()
                .filter(|key| key.get(6) == Some(&family))
                .count(),
            20
        );
    }
    assert!(
        rows.iter()
            .filter(|(key, _)| key.get(6) == Some(&9))
            .all(|(_, value)| value.starts_with(b"FWVA\0\x02"))
    );
    let store = open(&path)?;
    for mode in [1, 5, 6, 7] {
        for (index, value) in values.into_iter().enumerate() {
            assert_value(
                &store,
                format!("mode-{mode}-{index}").as_bytes(),
                Payload,
                1,
                value,
            )?;
        }
    }
    drop(store);
    assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn transition(from: &[u8], to: &[u8], payload: Option<&[u8]>) -> Operation {
    Operation::Transition(Transition {
        from: from.to_vec(),
        to: to.to_vec(),
        fence: Fence::new(0),
        lease_token: None,
        run_at: None,
        payload: payload.map(<[u8]>::to_vec),
    })
}

fn mixed_batch(typed: &[u8]) -> TestResult<PreparedWorkflowValueBatch> {
    batch(
        6,
        b"mixed",
        101,
        vec![
            f::item(
                b"j",
                101,
                Operation::Create(Create {
                    workflow_type: b"r".to_vec(),
                    state: b"q".to_vec(),
                    run_at: None,
                    payload: Some(typed.to_vec()),
                }),
            )?,
            f::item(
                b"i",
                101,
                Operation::Create(Create {
                    workflow_type: b"r".to_vec(),
                    state: b"q".to_vec(),
                    run_at: None,
                    payload: Some(b"must-not-publish".to_vec()),
                }),
            )?,
            f::item(b"i", 101, transition(b"q", b"next", Some(typed)))?,
            f::item(b"i", 101, transition(b"next", b"later", None))?,
        ],
    )
}

#[test]
fn stored_publication_mixed_aliases_and_every_history_survive_reopen() -> TestResult {
    let directory = crate::fixtures::directory("stored-publish-mixed")?;
    let path = directory.join("state");
    let scope = crate::fixtures::scope()?;
    let mut store = fresh(&path)?;
    let old = create(5, b"i", 100, Some(b"FSV2\x02legacy"))?;
    f::apply_value(&mut store, &scope, &old, 0)?;
    let typed = b"FSV2\x02\x83\x61\x07";
    let request = mixed_batch(typed)?;
    let applied = apply(&mut store, &request, 1);
    assert!(
        applied.is_ok(),
        "healthy stored publication: {:?}",
        applied.as_ref().err()
    );
    assert_eq!(retained(applied?)?.status(), RetainedStatus::Accepted);
    let binary = batch(
        5,
        b"binary-carry",
        102,
        vec![f::item(b"i", 102, transition(b"later", b"last", None))?],
    )?;
    f::apply_value(&mut store, &scope, &binary, 2)?;
    let cancel = batch(
        5,
        b"stored-cancel",
        103,
        vec![f::item(
            b"i",
            103,
            Operation::Cancel(Cancel {
                fence: Fence::new(0),
                lease_token: None,
                expected_state: Some(b"last".to_vec()),
                expected_version: Some(4),
                error: Some(b"FSV2\x02bad-etf".to_vec()),
            }),
        )?],
    )?;
    assert_eq!(
        retained(apply(&mut store, &cancel, 3)?)?.status(),
        RetainedStatus::Accepted
    );
    for event in [None, Some("101-2"), Some("101-3"), Some("102-4")] {
        let expected = if event.is_none() {
            [Some(2), None, Some(5)]
        } else {
            [Some(2), None, None]
        };
        assert_eq!(binding_versions(&store, b"i", event)?, expected);
    }
    assert_eq!(
        binding_versions(&store, b"i", Some("100-1"))?,
        [Some(1), None, None]
    );
    assert_value(&store, b"i", Payload, 1, b"FSV2\x01FSV2\x02legacy")?;
    assert_value(&store, b"i", Payload, 2, typed)?;
    assert_value(&store, b"i", Error, 5, b"FSV2\x02bad-etf")?;
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    for (family, count) in [(8, 4), (9, 4), (10, 2), (11, 6), (5, 6)] {
        assert_eq!(
            rows.keys()
                .filter(|key| key.get(6) == Some(&family))
                .count(),
            count
        );
    }
    assert_eq!(
        rows.iter()
            .filter(|(key, value)| key.get(6) == Some(&9) && value.starts_with(b"FWVA\0\x01"))
            .count(),
        1
    );
    assert_eq!(
        rows.iter()
            .filter(|(key, value)| key.get(6) == Some(&9) && value.starts_with(b"FWVA\0\x02"))
            .count(),
        3
    );
    let mut reopened = open(&path)?;
    assert_eq!(
        current(&mut reopened, b"i")?.record.payload(),
        Some(typed.as_slice())
    );
    assert_eq!(
        binding_versions(&reopened, b"i", None)?,
        [Some(2), None, Some(5)]
    );
    assert_value(&reopened, b"i", Error, 5, b"FSV2\x02bad-etf")?;
    drop(reopened);
    assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn stored_publication_terminal_values_remain_stored() -> TestResult {
    let directory = crate::fixtures::directory("stored-publish-terminal")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let born = apply(
        &mut store,
        &create(7, b"complete", 100, Some(b"original"))?,
        0,
    );
    assert!(
        born.is_ok(),
        "healthy stored publication: {:?}",
        born.as_ref().err()
    );
    assert_eq!(retained(born?)?.status(), RetainedStatus::Accepted);
    apply(&mut store, &create(7, b"fail", 100, Some(b"original"))?, 1)?;
    let mut items = Vec::new();
    for id in [b"complete".as_slice(), b"fail"] {
        let before = current(&mut store, id)?;
        let terminal = Terminal {
            guard: LeaseGuard {
                token: before.record.lease().ok_or("lease")?.token.clone(),
                fence: before.record.fence(),
            },
            payload: Some(b"FSV2\x02\x83\x61\x01".to_vec()),
            value: Some(b"FSV2\x02bad-etf".to_vec()),
        };
        let operation = if id == b"complete" {
            Operation::Complete(terminal)
        } else {
            Operation::Fail(terminal)
        };
        items.push(f::item(id, 200, operation)?);
    }
    let result = apply(&mut store, &batch(5, b"terminals", 200, items)?, 2)?;
    assert_eq!(retained(result)?.status(), RetainedStatus::Accepted);
    for (id, kind, state) in [
        (b"complete".as_slice(), ResultValue, b"completed".as_slice()),
        (b"fail", Error, b"failed"),
    ] {
        let record = current(&mut store, id)?;
        assert_eq!(record.record.state(), state);
        assert_value(&store, id, Payload, 2, b"FSV2\x02\x83\x61\x01")?;
        assert_value(&store, id, kind, 2, b"FSV2\x02bad-etf")?;
    }
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    assert_eq!(
        rows.iter()
            .filter(|(key, _)| key.get(6) == Some(&9))
            .count(),
        6
    );
    drop(open(&path)?);
    assert_eq!(crate::fixtures::raw_rows(&path)?, rows);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn stored_publication_retains_identity_and_requires_explicit_capability() -> TestResult {
    let directory = crate::fixtures::directory("stored-publish-identity")?;
    let path = directory.join("state");
    let mut store = fresh(&path)?;
    let request = create(5, b"identity", 100, Some(b"FSV2\x02\x83\x61\x01"))?;
    let result = apply(&mut store, &request, 0);
    assert!(
        result.is_ok(),
        "healthy stored publication: {:?}",
        result.as_ref().err()
    );
    let original = retained(result?)?;
    assert_eq!(original.intent().version(), 9);
    assert!(matches!(
        apply(&mut store, &request, 0)?,
        RecordBatchTransactionOutcome::LogReplay(_)
    ));
    assert_eq!(retained(apply(&mut store, &request, 1)?)?, original);
    let conflicting = create(5, b"identity", 100, Some(b"other"))?;
    assert!(matches!(
        apply(&mut store, &conflicting, 2)?,
        RecordBatchTransactionOutcome::IntentConflict(_)
    ));
    assert!(matches!(
        f::apply_value(&mut store, &request.scope, &request, 3)?,
        RecordBatchTransactionOutcome::IntentConflict(_)
    ));
    let new_frame = encode_workflow_stored_value_batch(&request, f::limits().record_batch.codec)?;
    let old_frame = encode_workflow_value_batch(&request, f::limits().record_batch.codec)?;
    assert!(
        apply_workflow_value_batch_transaction(
            &mut store,
            &request.scope,
            new_frame.as_bytes(),
            crate::fixtures::log(4),
            f::limits()
        )
        .is_err()
    );
    assert!(
        apply_workflow_stored_value_batch_transaction(
            &mut store,
            &request.scope,
            old_frame.as_bytes(),
            crate::fixtures::log(4),
            f::limits()
        )
        .is_err()
    );
    drop(store);
    let rows = crate::fixtures::raw_rows(&path)?;
    assert_eq!(rows.keys().filter(|key| key.get(6) == Some(&9)).count(), 1);
    let mut reopened = open(&path)?;
    assert_eq!(retained(apply(&mut reopened, &request, 4)?)?, original);
    drop(reopened);
    let binary_path = directory.join("binary");
    let mut binary = f::fresh(&binary_path)??;
    let refused = apply(&mut binary, &request, 0);
    assert!(matches!(
        refused
            .as_ref()
            .err()
            .and_then(|e| e.downcast_ref::<WorkflowValueTransactionError>()),
        Some(WorkflowValueTransactionError::UnsupportedProfile)
    ));
    let old = f::apply_value(&mut binary, &request.scope, &request, 0)?;
    assert_eq!(retained(old)?.intent().version(), 8);
    drop(binary);
    drop(open(&binary_path)?);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}
