//! Ordered intermediate histories, actual workspace stages and owner lifetimes.

use std::mem::size_of;
use std::ptr;

use wf_domain::workflow::batch::BatchRejection;
use wf_domain::workflow::indexed::{IndependentIndexedDecision, IndexRejection, IndexedDecision};
use wf_domain::workflow::{Cancel, Create, DomainError, Operation, Transition};
use wf_ports::blob::{
    RecordBatchValueEffect, RecordBatchValueEffects, RecordBatchValueEffectsError,
    inspect_record_batch_value_effects,
};
use wf_ports::log::{RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchInspection};
use wf_types::version::Fence;

use crate::fixtures::{
    TestResult, WORKSPACE, batch_frame, decide, inspection, item, limits, prepare,
};

#[test]
fn value_effects_preserve_independent_histories_and_source_index_holes() -> TestResult {
    let transition = Transition {
        from: b"q".to_vec(),
        to: b"next".to_vec(),
        fence: Fence::new(0),
        lease_token: None,
        run_at: None,
        payload: Some(b"replacement".to_vec()),
    };
    let mut invalid = transition.clone();
    invalid.from = b"wrong".to_vec();
    let cancel = Operation::Cancel(Cancel {
        fence: Fence::new(0),
        lease_token: None,
        expected_state: None,
        expected_version: None,
        error: Some(b"stop".to_vec()),
    });
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"initial".to_vec()),
    });
    let frame = batch_frame(
        vec![
            item(b"i", create)?,
            item(b"i", Operation::Transition(invalid))?,
            item(b"i", Operation::Transition(transition))?,
            item(b"i", cancel.clone())?,
            item(b"i", cancel)?,
        ],
        true,
    )?;
    let plan = decide(6, &[], &[b"i"], frame.as_bytes(), WORKSPACE, 0)??;
    let RecordBatchApplyDecision::IndependentFresh {
        operation: IndependentIndexedDecision::Accepted(batch),
        ..
    } = plan.decision()
    else {
        return Err("independent producer rejected the premise".into());
    };
    assert_eq!(batch.results().len(), 5);
    // Cancel rejects a terminal duplicate; it is not a source legacy no-op.
    for (index, error) in [(1, DomainError::WrongState), (4, DomainError::Terminal)] {
        assert_eq!(
            batch.results()[index],
            Err(IndexRejection::Batch(BatchRejection::Domain(error)))
        );
    }
    assert_eq!(batch.history_plans().count(), 3);
    assert_eq!(batch.final_indices().collect::<Vec<_>>(), [3]);
    for (index, version) in [(0, 1), (2, 2), (3, 3)] {
        let step = batch.results()[index]
            .as_ref()
            .map_err(|_| "history result rejected")?;
        assert!(step.plan().history().is_some());
        let write = step.plan().write().ok_or("intermediate write absent")?;
        assert_eq!(write.record.version(), version);
    }
    let decoded = prepare(6, frame.as_bytes(), WORKSPACE)??;
    let inspected = inspect_record_batch_value_effects(&decoded, &plan, WORKSPACE, WORKSPACE);
    assert!(inspected.is_ok(), "{:?}", inspected.as_ref().err());
    let Ok(Some(effects)) = inspected else {
        return Err("intermediate histories lack provenance".into());
    };
    assert_eq!(effects.steps().len(), 3);
    for (row, (index, version, mask, terminal)) in effects.steps().iter().zip([
        (0, 1, [true, false, false], false),
        (2, 2, [true, false, false], false),
        (3, 3, [false, false, true], true),
    ]) {
        assert_eq!(row.item_index(), index);
        assert_eq!(row.version(), version);
        assert_eq!(row.supplied(), mask);
        assert_eq!(row.terminal(), terminal);
    }
    Ok(())
}

#[test]
fn value_effects_admit_continuous_work_and_descriptor_coexistence() -> TestResult {
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"v".to_vec()),
    });
    let frame = batch_frame(vec![item(b"i", create)?], false)?;
    let plan = decide(5, &[], &[b"i"], frame.as_bytes(), WORKSPACE, 0)??;
    let RecordBatchApplyDecision::Fresh {
        operation: IndexedDecision::Accepted(batch),
        ..
    } = plan.decision()
    else {
        return Err("workspace premise was not accepted".into());
    };
    assert_eq!(batch.batch().items().len(), 1);
    assert!(batch.batch().items()[0].plan().history().is_some());
    let decoded = prepare(5, frame.as_bytes(), WORKSPACE)??;
    let row_bytes = size_of::<RecordBatchValueEffect>();
    let field_bytes = size_of::<Option<RecordBatchValueEffects>>();
    assert!(row_bytes > 0 && field_bytes > 0);
    let inspected = inspect_record_batch_value_effects(&decoded, &plan, row_bytes, WORKSPACE);
    assert!(inspected.is_ok(), "{:?}", inspected.as_ref().err());
    let Ok(Some(effects)) = inspected else {
        return Err("workspace history lacks provenance".into());
    };
    assert_eq!(effects.steps().len(), 1);
    let too_small = inspect_record_batch_value_effects(&decoded, &plan, row_bytes - 1, WORKSPACE);
    assert!(matches!(
        too_small,
        Err(RecordBatchValueEffectsError::Limit)
    ));
    for work in [0, 1] {
        let refused = inspect_record_batch_value_effects(&decoded, &plan, row_bytes, work);
        assert!(matches!(refused, Err(RecordBatchValueEffectsError::Limit)));
    }
    let encoded = frame.as_bytes().len();
    let decoder_bound = 4 * encoded + limits().admission().decision_bytes() + 1_024 + 2_048;
    let early_bound = decoder_bound + 192 + 256 + limits().admission().decision_bytes();
    let owned_bound = early_bound + field_bytes + row_bytes;
    for maximum in [early_bound, owned_bound - 1] {
        let refused = decide(5, &[], &[b"i"], frame.as_bytes(), maximum, 0)?;
        assert!(matches!(
            refused,
            Err(RecordBatchApplyError::Workspace { attempted, maximum: actual })
                if attempted == owned_bound && actual == maximum
        ));
    }
    assert!(decide(5, &[], &[b"i"], frame.as_bytes(), owned_bound, 0)?.is_ok());
    let complete_bound = decoder_bound + field_bytes;
    let next = plan.control().next();
    let refused = inspection(5, frame.as_bytes(), complete_bound - 1, next, None, 1)?;
    assert!(matches!(
        refused,
        Err(RecordBatchApplyError::Workspace { attempted, maximum })
            if attempted == complete_bound && maximum == complete_bound - 1
    ));
    let replay = inspection(5, frame.as_bytes(), complete_bound, next, None, 1)??;
    let RecordBatchInspection::Complete(replay) = replay else {
        return Err("log replay dispatched Fresh".into());
    };
    assert!(matches!(
        replay.decision(),
        RecordBatchApplyDecision::LogReplay
    ));
    assert!(matches!(
        inspect_record_batch_value_effects(&decoded, &replay, 0, WORKSPACE),
        Ok(None)
    ));
    assert!(matches!(
        inspect_record_batch_value_effects(&decoded, &replay, 0, 0),
        Err(RecordBatchValueEffectsError::Limit)
    ));
    Ok(())
}

#[test]
fn value_effects_borrow_only_the_plan_after_decoded_verification_drops() -> TestResult {
    let create = Operation::Create(Create {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        run_at: None,
        payload: Some(b"v".to_vec()),
    });
    let frame = batch_frame(vec![item(b"i", create)?], false)?;
    let plan = decide(5, &[], &[b"i"], frame.as_bytes(), WORKSPACE, 0)??;
    let other_plan = decide(5, &[], &[b"i"], frame.as_bytes(), WORKSPACE, 0)??;
    for current in [&plan, &other_plan] {
        let RecordBatchApplyDecision::Fresh {
            operation: IndexedDecision::Accepted(batch),
            ..
        } = current.decision()
        else {
            return Err("owner premise was not accepted".into());
        };
        assert_eq!(batch.batch().items().len(), 1);
        assert!(batch.batch().items()[0].plan().history().is_some());
    }
    let (first, repeated) = {
        let decoded = prepare(5, frame.as_bytes(), WORKSPACE)??;
        let first = inspect_record_batch_value_effects(&decoded, &plan, WORKSPACE, WORKSPACE);
        assert!(first.is_ok(), "{:?}", first.as_ref().err());
        let Ok(Some(first)) = first else {
            return Err("first owner absent".into());
        };
        let repeated = inspect_record_batch_value_effects(&decoded, &plan, WORKSPACE, WORKSPACE);
        assert!(repeated.is_ok(), "{:?}", repeated.as_ref().err());
        let Ok(Some(repeated)) = repeated else {
            return Err("repeated owner absent".into());
        };
        assert_eq!(first.steps().len(), 1);
        assert_eq!(repeated.steps().len(), 1);
        (first, repeated)
    };
    assert_eq!(first.steps()[0].item_index(), 0);
    assert_eq!(first.steps()[0].version(), 1);
    assert_eq!(first.steps()[0].supplied(), [true, false, false]);
    assert!(!first.steps()[0].terminal());
    assert!(ptr::eq(first, repeated));
    assert!(ptr::eq(first.steps().as_ptr(), repeated.steps().as_ptr()));
    let other = {
        let decoded = prepare(5, frame.as_bytes(), WORKSPACE)??;
        let inspected =
            inspect_record_batch_value_effects(&decoded, &other_plan, WORKSPACE, WORKSPACE);
        assert!(inspected.is_ok(), "{:?}", inspected.as_ref().err());
        let Ok(Some(other)) = inspected else {
            return Err("cross-plan owner absent".into());
        };
        assert_eq!(other.steps().len(), 1);
        other
    };
    assert_eq!(other.steps(), first.steps());
    assert!(!ptr::eq(first, other));
    assert!(!ptr::eq(first.steps().as_ptr(), other.steps().as_ptr()));
    Ok(())
}
