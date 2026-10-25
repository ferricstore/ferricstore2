//! Presence and source-index expectations belong to these tests, not fixtures.

use wf_domain::lease::{ExtendLease, LeaseGuard, LeaseToken};
use wf_domain::retry::{Retry, RetryTime};
use wf_domain::workflow::caller::MutationFieldValue;
use wf_domain::workflow::control::EntryIdentity;
use wf_domain::workflow::indexed::{IndependentIndexedDecision, IndexedDecision};
use wf_domain::workflow::{
    Cancel, CommittedCommand, Create, Operation, Outcome, Terminal, Transition, reduce,
};
use wf_ports::blob::{RecordBatchValueEffectsError, inspect_record_batch_value_effects};
use wf_ports::log::{
    CodecError, RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchInspection,
    encode_independent_record_batch, encode_record_batch,
};
use wf_types::version::{Fence, TimestampMillis};

use crate::fixtures::{
    TestResult, WORKSPACE, atomic_frame, batch_frame, claimed, decide, inspection, item, limits,
    outcome, prepare, public_frame,
};

#[test]
fn value_effects_bind_exact_prepared_entry_and_operation_kinds() -> TestResult {
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"v".to_vec()),
    });
    let fields = [
        ("type", MutationFieldValue::Bytes(b"r")),
        ("state", MutationFieldValue::Bytes(b"q")),
        ("payload", MutationFieldValue::Bytes(b"v")),
    ];
    let frames = [
        (5, batch_frame(vec![item(b"i", create.clone())?], false)?),
        (6, batch_frame(vec![item(b"i", create)?], true)?),
        (1, public_frame("create", b"i", &fields)?),
        (7, atomic_frame(Some(b"v"))?),
    ];
    for (profile, frame) in &frames {
        let plan = decide(*profile, &[], &[b"i"], frame.as_bytes(), WORKSPACE, 0)??;
        let step = match plan.decision() {
            RecordBatchApplyDecision::Fresh {
                operation: IndexedDecision::Accepted(batch),
                ..
            } => {
                assert_eq!(batch.batch().items().len(), 1);
                &batch.batch().items()[0]
            }
            RecordBatchApplyDecision::IndependentFresh {
                operation: IndependentIndexedDecision::Accepted(batch),
                ..
            } => {
                assert_eq!(batch.results().len(), 1);
                batch.results()[0]
                    .as_ref()
                    .map_err(|_| "producer rejected creation")?
            }
            _ => return Err("producer did not accept the actual profile".into()),
        };
        assert!(step.plan().history().is_some());
        let record = &step.plan().write().ok_or("creation write absent")?.record;
        assert_eq!(record.version(), 1);
        assert_eq!(record.payload(), Some(b"v".as_slice()));
        let decoded = prepare(*profile, frame.as_bytes(), WORKSPACE)??;
        assert_eq!(
            plan.control()
                .next()
                .last_entry()
                .map(EntryIdentity::prepared),
            Some(decoded.digest())
        );
        let inspected = inspect_record_batch_value_effects(&decoded, &plan, WORKSPACE, WORKSPACE);
        assert!(
            inspected.is_ok(),
            "profile {profile}: {:?}",
            inspected.as_ref().err()
        );
        let Ok(Some(effects)) = inspected else {
            return Err("real history has no effect owner".into());
        };
        assert_eq!(effects.steps().len(), 1);
        assert_eq!(effects.steps()[0].item_index(), 0);
        assert_eq!(effects.steps()[0].version(), 1);
        assert_eq!(effects.steps()[0].supplied(), [true, false, false]);
        assert!(!effects.steps()[0].terminal());
        if *profile == 7 {
            for encode in [encode_record_batch, encode_independent_record_batch] {
                let refused = encode(decoded.request().clone(), limits(), WORKSPACE);
                assert!(matches!(
                    refused,
                    Err(RecordBatchApplyError::Encoding(CodecError::UnsupportedTag))
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn value_effects_distinguish_omitted_empty_and_equal_replacements() -> TestResult {
    let records = [claimed(b"i", Some(b"v"))?];
    for (replacement, expected_payload, mask) in [
        (None, b"v".as_slice(), [false, false, false]),
        (Some(b"".as_slice()), b"".as_slice(), [true, false, false]),
        (Some(b"v".as_slice()), b"v".as_slice(), [true, false, false]),
    ] {
        let command = item(
            b"i",
            Operation::Transition(Transition {
                from: b"running".to_vec(),
                to: b"q".to_vec(),
                fence: Fence::new(1),
                lease_token: Some(LeaseToken::new(b"k".to_vec())?),
                run_at: None,
                payload: replacement.map(<[u8]>::to_vec),
            }),
        )?;
        let frame = batch_frame(vec![command], false)?;
        let plan = decide(5, &records, &[b"i"], frame.as_bytes(), WORKSPACE, 0)??;
        let RecordBatchApplyDecision::Fresh {
            operation: IndexedDecision::Accepted(batch),
            ..
        } = plan.decision()
        else {
            return Err("transition producer rejected the premise".into());
        };
        let step = &batch.batch().items()[0];
        assert!(step.plan().history().is_some());
        let write = step.plan().write().ok_or("transition has no write")?;
        assert_eq!(write.record.version(), 3);
        assert_eq!(write.record.payload(), Some(expected_payload));
        let decoded = prepare(5, frame.as_bytes(), WORKSPACE)??;
        assert_eq!(decoded.value_bytes(0), Some([replacement, None]));
        let inspected = inspect_record_batch_value_effects(&decoded, &plan, WORKSPACE, WORKSPACE);
        assert!(inspected.is_ok(), "{:?}", inspected.as_ref().err());
        let Ok(Some(effects)) = inspected else {
            return Err("replacement history lacks provenance".into());
        };
        assert_eq!(effects.steps().len(), 1);
        assert_eq!(effects.steps()[0].item_index(), 0);
        assert_eq!(effects.steps()[0].version(), 3);
        assert_eq!(effects.steps()[0].supplied(), mask);
        assert!(!effects.steps()[0].terminal());
        assert_eq!(records[0].version(), 2);
        assert_eq!(records[0].state(), b"running");
        assert_eq!(records[0].payload(), Some(b"v".as_slice()));
    }
    Ok(())
}

#[test]
fn value_effects_preserve_atomic_input_indices_and_kinds() -> TestResult {
    let records = [
        claimed(b"c", None)?,
        claimed(b"f", None)?,
        claimed(b"x", None)?,
        claimed(b"r", Some(b"p"))?,
    ];
    let guard = LeaseGuard {
        token: LeaseToken::new(b"k".to_vec())?,
        fence: Fence::new(1),
    };
    let terminal = Terminal {
        guard: guard.clone(),
        payload: None,
        value: Some(b"value".to_vec()),
    };
    let cancel = Operation::Cancel(Cancel {
        fence: Fence::new(1),
        lease_token: Some(guard.token.clone()),
        expected_state: None,
        expected_version: None,
        error: Some(b"cancel".to_vec()),
    });
    let retry = Operation::Retry(Retry {
        guard,
        run_at: RetryTime::Explicit(TimestampMillis::new(9)),
        payload: None,
        error: Some(b"retry".to_vec()),
    });
    let frame = batch_frame(
        vec![
            item(b"c", Operation::Complete(terminal.clone()))?,
            item(b"f", Operation::Fail(terminal))?,
            item(b"x", cancel)?,
            item(b"r", retry)?,
        ],
        false,
    )?;
    let ids = [b"c".as_slice(), b"f", b"x", b"r"];
    let plan = decide(5, &records, &ids, frame.as_bytes(), WORKSPACE, 0)??;
    let RecordBatchApplyDecision::Fresh {
        operation: IndexedDecision::Accepted(batch),
        ..
    } = plan.decision()
    else {
        return Err("atomic producer rejected the field-kind premises".into());
    };
    assert_eq!(batch.batch().items().len(), 4);
    for step in batch.batch().items() {
        assert!(step.plan().history().is_some());
        assert_eq!(
            step.plan()
                .write()
                .ok_or("atomic write absent")?
                .record
                .version(),
            3
        );
    }
    let decoded = prepare(5, frame.as_bytes(), WORKSPACE)??;
    assert_eq!(
        decoded.value_bytes(0),
        Some([None, Some(b"value".as_slice())])
    );
    assert_eq!(
        decoded.value_bytes(2),
        Some([Some(b"cancel".as_slice()), None])
    );
    let inspected = inspect_record_batch_value_effects(&decoded, &plan, WORKSPACE, WORKSPACE);
    assert!(inspected.is_ok(), "{:?}", inspected.as_ref().err());
    let Ok(Some(effects)) = inspected else {
        return Err("atomic histories lack provenance".into());
    };
    assert_eq!(effects.steps().len(), 4);
    for (row, (index, mask, terminal)) in effects.steps().iter().zip([
        (0, [false, true, false], true),
        (1, [false, false, true], true),
        (2, [false, false, true], true),
        (3, [false, false, true], false),
    ]) {
        assert_eq!(row.item_index(), index);
        assert_eq!(row.version(), 3);
        assert_eq!(row.supplied(), mask);
        assert_eq!(row.terminal(), terminal);
    }
    let retry = &batch.batch().items()[3]
        .plan()
        .write()
        .ok_or("retry write absent")?
        .record;
    assert_eq!(retry.payload(), Some(b"p".as_slice()));
    assert_eq!(retry.error(), Some(b"retry".as_slice()));
    assert_eq!(records[3].payload(), Some(b"p".as_slice()));
    Ok(())
}

#[test]
fn value_effects_emit_carry_rows_for_real_no_value_histories() -> TestResult {
    let records = [claimed(b"i", Some(b"v"))?];
    let guard = LeaseGuard {
        token: LeaseToken::new(b"k".to_vec())?,
        fence: Fence::new(1),
    };
    let frame = batch_frame(
        vec![item(
            b"i",
            Operation::ExtendLease(ExtendLease {
                guard: guard.clone(),
                lease_ms: 2_000,
            }),
        )?],
        false,
    )?;
    let plan = decide(5, &records, &[b"i"], frame.as_bytes(), WORKSPACE, 0)??;
    let RecordBatchApplyDecision::Fresh {
        operation: IndexedDecision::Accepted(batch),
        ..
    } = plan.decision()
    else {
        return Err("ExtendLease producer rejected the carry premise".into());
    };
    let step = &batch.batch().items()[0];
    assert!(step.plan().history().is_some());
    assert_eq!(
        step.plan()
            .write()
            .ok_or("extend write absent")?
            .record
            .version(),
        3
    );
    let decoded = prepare(5, frame.as_bytes(), WORKSPACE)??;
    let inspected = inspect_record_batch_value_effects(&decoded, &plan, WORKSPACE, WORKSPACE);
    assert!(inspected.is_ok(), "{:?}", inspected.as_ref().err());
    let Ok(Some(effects)) = inspected else {
        return Err("zero-mask history was omitted".into());
    };
    assert_eq!(effects.steps().len(), 1);
    assert_eq!(effects.steps()[0].item_index(), 0);
    assert_eq!(effects.steps()[0].version(), 3);
    assert_eq!(effects.steps()[0].supplied(), [false, false, false]);
    assert!(!effects.steps()[0].terminal());
    let rejected = decide(5, &[], &[b"i"], frame.as_bytes(), WORKSPACE, 0)??;
    assert!(matches!(
        rejected.decision(),
        RecordBatchApplyDecision::Fresh {
            operation: IndexedDecision::Rejected(_),
            ..
        }
    ));
    let terminal = Operation::Complete(Terminal {
        guard,
        payload: None,
        value: None,
    });
    let completed_plan = reduce(
        Some(&records[0]),
        &CommittedCommand {
            id: records[0].id().clone(),
            now: TimestampMillis::new(2),
            operation: terminal.clone(),
        },
    )?;
    let (_, write, _) = completed_plan.into_parts();
    let completed = [write.ok_or("completed fixture write absent")?.record];
    let noop_frame = batch_frame(vec![item(b"i", terminal)?], false)?;
    let bytes = noop_frame.as_bytes();
    let no_history = decide(5, &completed, &[b"i"], bytes, WORKSPACE, 0)??;
    let RecordBatchApplyDecision::Fresh {
        operation: IndexedDecision::Accepted(noop),
        ..
    } = no_history.decision()
    else {
        return Err("duplicate Complete was rejected".into());
    };
    assert_eq!(
        noop.batch().items()[0].plan().outcome(),
        Outcome::LegacyNoop
    );
    assert!(noop.batch().items()[0].plan().history().is_none());
    let noop_decoded = prepare(5, bytes, WORKSPACE)??;
    for (verified, empty_plan) in [(&decoded, &rejected), (&noop_decoded, &no_history)] {
        assert!(matches!(
            inspect_record_batch_value_effects(verified, empty_plan, 0, WORKSPACE),
            Ok(None)
        ));
    }
    Ok(())
}

#[test]
fn value_effects_refuse_crossed_profiles_and_do_not_invent_expiry() -> TestResult {
    let records = [claimed(b"i", None)?];
    let terminal = Terminal {
        guard: LeaseGuard {
            token: LeaseToken::new(b"k".to_vec())?,
            fence: Fence::new(1),
        },
        payload: None,
        value: Some(b"done".to_vec()),
    };
    let frame = batch_frame(
        vec![item(b"i", Operation::Complete(terminal.clone()))?],
        false,
    )?;
    let plan = decide(5, &records, &[b"i"], frame.as_bytes(), WORKSPACE, 0)??;
    let RecordBatchApplyDecision::Fresh {
        operation: IndexedDecision::Accepted(batch),
        ..
    } = plan.decision()
    else {
        return Err("terminal producer rejected the premise".into());
    };
    let write = batch.batch().items()[0]
        .plan()
        .write()
        .ok_or("terminal write absent")?;
    assert_eq!(write.record.state(), b"completed");
    assert_eq!(write.record.version(), 3);
    assert!(batch.batch().items()[0].plan().history().is_some());
    let decoded = prepare(5, frame.as_bytes(), WORKSPACE)??;
    let inspected = inspect_record_batch_value_effects(&decoded, &plan, WORKSPACE, WORKSPACE);
    assert!(inspected.is_ok(), "{:?}", inspected.as_ref().err());
    let Ok(Some(effects)) = inspected else {
        return Err("terminal history lacks provenance".into());
    };
    assert_eq!(effects.steps().len(), 1);
    assert!(effects.steps()[0].terminal());
    // Outcome retention is a separate request fact, never a value TTL witness.
    assert_eq!(outcome(&plan)?.expires_at(), TimestampMillis::new(1_002));
    for profile in [1, 6, 7] {
        assert!(matches!(
            prepare(profile, frame.as_bytes(), WORKSPACE)?,
            Err(RecordBatchApplyError::Encoding(
                CodecError::UnsupportedVersion
            ))
        ));
    }
    let mut other_terminal = terminal;
    other_terminal.value = Some(b"different".to_vec());
    let other = batch_frame(
        vec![item(b"i", Operation::Complete(other_terminal))?],
        false,
    )?;
    let crossed = prepare(5, other.as_bytes(), WORKSPACE)??;
    assert_ne!(crossed.digest(), decoded.digest());
    assert!(matches!(
        inspect_record_batch_value_effects(&crossed, &plan, WORKSPACE, WORKSPACE),
        Err(RecordBatchValueEffectsError::InvalidChanges)
    ));
    let next = plan.control().next();
    let original = Some(outcome(&plan)?);
    let conflict = inspection(5, other.as_bytes(), WORKSPACE, next, original, 2)??;
    let RecordBatchInspection::Complete(conflict) = conflict else {
        return Err("intent conflict dispatched Fresh".into());
    };
    assert!(matches!(
        conflict.decision(),
        RecordBatchApplyDecision::IntentConflict
    ));
    assert!(matches!(
        inspect_record_batch_value_effects(&crossed, &conflict, 0, WORKSPACE),
        Ok(None)
    ));
    let replay = inspection(5, frame.as_bytes(), WORKSPACE, next, original, 2)??;
    let RecordBatchInspection::Complete(replay) = replay else {
        return Err("replay dispatched Fresh".into());
    };
    assert!(matches!(
        replay.decision(),
        RecordBatchApplyDecision::Replay(_)
    ));
    assert!(matches!(
        inspect_record_batch_value_effects(&decoded, &replay, 0, WORKSPACE),
        Ok(None)
    ));
    Ok(())
}
