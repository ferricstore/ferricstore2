//! Public scalar Changes validation, not a storage or authorization certificate.

use wf_domain::claim::{Claim, ClaimRecord, DueOrder, LaneKey, ScopedRecordId};
use wf_domain::history::EventKind;
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::{BatchItem, BatchRejection};
use wf_domain::workflow::caller::{MutationFieldValue, MutationInput, normalize_core_mutation};
use wf_domain::workflow::control::{EntryIdentity, EntryStamp, PreparedDigest};
use wf_domain::workflow::indexed::{
    EntryOrder, IndependentIndexedDecision, IndexRejection, IndexedBatchItem,
    TouchedIndexedGroupView, decide_touched_independent_indexed_batch,
};
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::{DomainError, Outcome};
use wf_ports::log::{
    CodecError, RecordBatchApplyError, RecordBatchInspection,
    decide_fresh_independent_record_batch_touched, decide_fresh_public_mutation_touched,
    encode_independent_record_batch, encode_public_mutation,
    inspect_independent_record_batch_point, inspect_public_mutation_point,
    prepare_independent_record_batch, prepare_public_mutation, public_mutation_response_matches,
    public_mutation_retained_status,
};

use super::*;

const WORKSPACE: usize = 64 * 1_024 * 1_024;

fn previous() -> TestResult<ControlState> {
    Ok(ControlState::restored(
        clock(1_000)?,
        EntryIdentity::new(
            LogIdentity::new(1, 7, AppliedPosition::new(1)),
            PreparedDigest::new([7; 32]),
        ),
        EntryStamp::Stamped(clock(1_000)?),
    )?)
}

fn limits() -> CodecLimits {
    CodecLimits::new(AdmissionLimits::default())
}

fn running() -> TestResult<StoredWorkflowRecord> {
    let mut row = sample_record(b"i", b"r", b"q")?;
    let command = CommittedCommand {
        id: row.record.id().clone(),
        now: TimestampMillis::new(1_000),
        operation: Operation::Claim(Claim {
            expected_state: b"q".to_vec(),
            worker: b"w".to_vec(),
            token: LeaseToken::new(b"l".to_vec())?,
            lease_ms: 30_000,
        }),
    };
    row.record = reduce(Some(&row.record), &command)?
        .into_parts()
        .1
        .ok_or("claim write")?
        .record;
    row.ordering.state_enter_seq = 99;
    assert_eq!(row.record.version(), 2);
    Ok(row)
}

fn operation(token: &[u8]) -> TestResult<Operation> {
    Ok(Operation::Complete(Terminal {
        guard: LeaseGuard {
            token: LeaseToken::new(token.to_vec())?,
            fence: Fence::new(1),
        },
        payload: None,
        value: None,
    }))
}

fn encoded(
    scope: &RequestScope,
    token: &[u8],
    codec: CodecLimits,
) -> TestResult<wf_ports::log::PreparedBytes> {
    let fields = [
        ("lease_token", MutationFieldValue::Bytes(token)),
        ("fencing_token", MutationFieldValue::Integer(1)),
    ];
    let caller = normalize_core_mutation(
        MutationInput {
            operation: "complete",
            id: b"i",
            fields: &fields,
        },
        &[],
        Some(b"p"),
        codec.admission(),
        16 * 1_024 * 1_024,
    )?;
    Ok(encode_public_mutation(
        scope,
        &RequestId::new(b"q".to_vec())?,
        &caller,
        clock(2_000)?,
        codec,
        WORKSPACE,
    )?
    .0)
}

fn observed<'current, T>(
    scope: &RequestScope,
    row: Option<&'current StoredWorkflowRecord>,
    cross: bool,
    inspect: impl FnOnce(TouchedIndexedGroupView<'_, 'current>) -> TestResult<T>,
) -> TestResult<T> {
    let placement = if cross {
        GroupId::new(b"other".to_vec())?
    } else {
        scope.group().clone()
    };
    let placements = BTreeMap::from([(PartitionId::new(b"p".to_vec())?, placement)]);
    let entry = row.map(|row| ClaimRecord {
        record: &row.record,
        partition_key: row.partition.as_bytes(),
        priority: row.ordering.priority,
        state_enter_seq: row.ordering.state_enter_seq,
    });
    let current = BTreeMap::from([(
        ScopedRecordId {
            partition_key: b"p",
            id: b"i",
        },
        entry,
    )]);
    let parallel = entry
        .and_then(|row| DueOrder::for_record(&row))
        .map(|due| BTreeMap::from([(due, true)]))
        .unwrap_or_default();
    let heads = entry
        .and_then(|row| LaneKey::for_record(&row))
        .map(|lane| BTreeMap::from([(lane, None)]))
        .unwrap_or_default();
    let members = BTreeMap::new();
    let policies = BTreeMap::from([(b"r".as_slice(), None)]);
    let retry = BTreeMap::from([(b"r".as_slice(), None)]);
    inspect(TouchedIndexedGroupView {
        group: scope.group(),
        placements: &placements,
        current: &current,
        parallel: &parallel,
        fifo_members: &members,
        fifo_heads: &heads,
        policies: &policies,
        retry_policies: &retry,
    })
}

fn planned<'current>(
    scope: &RequestScope,
    row: Option<&'current StoredWorkflowRecord>,
    token: &[u8],
    codec: CodecLimits,
    cross: bool,
) -> TestResult<RecordBatchApplyPlan<'current>> {
    let bytes = encoded(scope, token, codec)?;
    let decoded = prepare_public_mutation(bytes.as_bytes(), codec, WORKSPACE)?;
    let RecordBatchInspection::Fresh(fresh) = inspect_public_mutation_point(
        decoded,
        LogIdentity::new(1, 7, AppliedPosition::new(2)),
        PointGateView {
            scope,
            control: previous()?,
            outcome: None,
            retained_nonempty: false,
        },
        codec,
        WORKSPACE,
    )?
    else {
        return Err("not Fresh".into());
    };
    observed(scope, row, cross, |view| {
        Ok(decide_fresh_public_mutation_touched(fresh, view, 0, 1_000)?)
    })
}

fn item_outcomes() -> TestResult {
    let scope = scope()?;
    let row = running()?;
    for (current, token, response, status, writes) in [
        (
            Some(&row),
            b"l".as_slice(),
            &[0][..],
            RetainedStatus::Accepted,
            1,
        ),
        (
            Some(&row),
            b"wrong".as_slice(),
            &[2, 0, 4][..],
            RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
                DomainError::StaleLease,
            ))),
            0,
        ),
        (
            None,
            b"l".as_slice(),
            &[2, 0, 0][..],
            RetainedStatus::IndexedRejected(IndexRejection::Batch(BatchRejection::Domain(
                DomainError::NotFound,
            ))),
            0,
        ),
    ] {
        let plan = planned(&scope, current, token, limits(), false)?;
        let next = plan.control().next();
        let RecordBatchApplyDecision::IndependentFresh { operation, outcome } = plan.decision()
        else {
            return Err("not independent Fresh".into());
        };
        assert_eq!(outcome.intent().version(), 1);
        assert_eq!(outcome.response_version(), 2);
        assert_eq!(outcome.status(), status);
        assert_eq!(outcome.response(), response);
        assert_eq!(
            public_mutation_retained_status(outcome.intent(), 2, response)?,
            status
        );
        assert!(public_mutation_response_matches(operation, response)?);
        assert!(!public_mutation_response_matches(
            operation,
            if writes == 1 {
                &[2, 0, 0][..]
            } else {
                &[0][..]
            }
        )?);
        let changes = GroupApplyChanges::new(&scope, previous()?, plan)?;
        assert_eq!(changes.expected(), previous()?);
        assert_eq!(changes.control(), Some(next));
        assert!(changes.business().is_none());
        let business = changes
            .independent_business()
            .ok_or("evaluated results absent")?;
        assert_eq!(business.results().len(), 1);
        assert_eq!(business.final_indices().count(), writes);
        assert_eq!(business.history_plans().count(), writes);
        assert_eq!(business.consumed_ordinals(), 0);
        if writes == 1 {
            assert_eq!(business.final_indices().collect::<Vec<_>>(), [0]);
            let original = business
                .originals()
                .next()
                .flatten()
                .ok_or("original absent")?;
            assert!(std::ptr::eq(original.record, &raw const row.record));
            assert_eq!(original.record.version(), 2);
            assert_eq!(original.state_enter_seq, 99);
            let item = business.results()[0]
                .as_ref()
                .map_err(|_| "write rejected")?;
            let write = item.plan().write().ok_or("write absent")?;
            assert_eq!(write.expected_version, Some(2));
            assert_eq!(write.record.version(), 3);
            assert_eq!(write.record.state(), b"completed");
            assert_eq!(
                item.plan().history().ok_or("history absent")?.kind,
                EventKind::Completed
            );
        }
    }
    Ok(())
}

fn whole_origins() -> TestResult {
    let scope = scope()?;
    let row = running()?;
    for (codec, cross, body, reason) in [
        (
            CodecLimits::new(AdmissionLimits::new(256, 2_800, 1)?),
            false,
            [1, 1, 4],
            IndexRejection::Batch(BatchRejection::Admission(AdmissionError::DecisionBytes)),
        ),
        (
            limits(),
            true,
            [1, 3, 0],
            IndexRejection::Batch(BatchRejection::CrossGroup),
        ),
    ] {
        // One running predecessor contributes12 bytes; V=44+12, charge2912.
        // B2800 admits the fixed2688 but refuses the actual cumulative input.
        let plan = planned(&scope, Some(&row), b"l", codec, cross)?;
        let RecordBatchApplyDecision::IndependentFresh { operation, outcome } = plan.decision()
        else {
            return Err("not whole Fresh".into());
        };
        assert_eq!(operation, &IndependentIndexedDecision::Rejected(reason));
        assert_eq!(outcome.response(), body);
        assert_eq!(outcome.status(), RetainedStatus::IndexedRejected(reason));
        assert!(public_mutation_response_matches(operation, &body)?);
        assert!(public_mutation_response_matches(operation, &[2, 1, 4]).is_err());
        assert!(!public_mutation_response_matches(operation, &[0])?);
        let changes = GroupApplyChanges::new(&scope, previous()?, plan)?;
        assert!(changes.independent_business().is_none());
        assert!(changes.business().is_none());
        assert!(changes.control().is_some());
        assert!(changes.outcome().is_some());
    }
    Ok(())
}

fn matcher_and_count() -> TestResult {
    let scope = scope()?;
    let plan = planned(&scope, None, b"l", limits(), false)?;
    let RecordBatchApplyDecision::IndependentFresh { operation, .. } = plan.decision() else {
        return Err("not Fresh".into());
    };
    for bad in [
        &[][..],
        &[0, 0],
        &[2, 0, 0, 0],
        &[1, 0, 0],
        &[2, 1, 4],
        &[2, 0, 0, 0, 1, 0],
    ] {
        assert!(public_mutation_response_matches(operation, bad).is_err());
    }
    let command = CommittedCommand {
        id: RecordId::new(b"i".to_vec())?,
        now: TimestampMillis::new(2_000),
        operation: operation_for_count()?,
    };
    let partition = PartitionId::new(b"p".to_vec())?;
    let item = IndexedBatchItem::new(BatchItem::new(&partition, &command, b"k"));
    let two = observed(&scope, None, false, |view| {
        Ok(decide_touched_independent_indexed_batch(
            view,
            &[item, item],
            EntryOrder {
                identity: LogIdentity::new(1, 7, AppliedPosition::new(2)),
                first_ordinal: 0,
            },
            b"two",
            AdmissionLimits::default(),
        )?)
    })?;
    assert!(matches!(&two, IndependentIndexedDecision::Accepted(plan) if plan.results().len()==2));
    assert!(!matches!(
        public_mutation_response_matches(&two, &[2, 0, 0]),
        Ok(true)
    ));
    Ok(())
}

fn operation_for_count() -> TestResult<Operation> {
    operation(b"l")
}

fn invalid_changes() -> TestResult {
    let scope = scope()?;
    let row = running()?;
    for foreign in [
        RequestScope::new(
            TenantId::new(b"other".to_vec())?,
            scope.group().clone(),
            scope.recovery().clone(),
        ),
        RequestScope::new(
            scope.tenant().clone(),
            GroupId::new(b"other".to_vec())?,
            scope.recovery().clone(),
        ),
        RequestScope::new(
            scope.tenant().clone(),
            scope.group().clone(),
            RecoveryId::new(b"other".to_vec())?,
        ),
    ] {
        assert!(matches!(
            GroupApplyChanges::new(
                &foreign,
                previous()?,
                planned(&scope, Some(&row), b"l", limits(), false)?
            ),
            Err(StateTransactionError::InvalidChanges)
        ));
    }
    for applied in [false, true] {
        let plan = planned(&scope, Some(&row), b"l", limits(), false)?;
        let wrong = if applied {
            plan.control().next()
        } else {
            ControlState::initial(clock(0)?)
        };
        assert!(matches!(
            GroupApplyChanges::new(&scope, wrong, plan),
            Err(StateTransactionError::InvalidChanges)
        ));
    }
    Ok(())
}

fn profile_crossing(public: bool) -> TestResult {
    let scope = scope()?;
    let codec = limits();
    let frame = if public {
        encoded(&scope, b"l", codec)?
    } else {
        encode_independent_record_batch(
            PreparedRequest {
                scope: scope.clone(),
                request: RequestId::new(b"q".to_vec())?,
                intent: ClientIntent::new(1, [0; 32])?,
                stamp: clock(2_000)?,
                operation: PreparedOperation::Batch(vec![PreparedBatchItem {
                    partition: PartitionId::new(b"p".to_vec())?,
                    command: CommittedCommand {
                        id: RecordId::new(b"i".to_vec())?,
                        now: TimestampMillis::new(2_000),
                        operation: operation(b"l")?,
                    },
                    create_priority: 0,
                }]),
            },
            codec,
            WORKSPACE,
        )?
    };
    let point = PointGateView {
        scope: &scope,
        control: previous()?,
        outcome: None,
        retained_nonempty: false,
    };
    let at = LogIdentity::new(1, 7, AppliedPosition::new(2));
    let inspection = if public {
        inspect_public_mutation_point(
            prepare_public_mutation(frame.as_bytes(), codec, WORKSPACE)?,
            at,
            point,
            codec,
            WORKSPACE,
        )?
    } else {
        inspect_independent_record_batch_point(
            prepare_independent_record_batch(frame.as_bytes(), codec, WORKSPACE)?,
            at,
            point,
            codec,
            WORKSPACE,
        )?
    };
    let RecordBatchInspection::Fresh(fresh) = inspection else {
        return Err("not Fresh".into());
    };
    observed(&scope, None, false, |view| {
        let result = if public {
            decide_fresh_independent_record_batch_touched(fresh, view, 0, 1_000)
        } else {
            decide_fresh_public_mutation_touched(fresh, view, 0, 1_000)
        };
        assert!(matches!(
            result,
            Err(RecordBatchApplyError::Encoding(
                CodecError::UnsupportedVersion
            ))
        ));
        Ok(())
    })
}

fn owns_result_after_donors() -> TestResult {
    let scope = scope()?;
    let row = running()?;
    let plan = planned(&scope, Some(&row), b"l", limits(), false)?;
    let changes = GroupApplyChanges::new(&scope, previous()?, plan)?;
    let RecordBatchApplyDecision::IndependentFresh {
        operation: IndependentIndexedDecision::Accepted(business),
        outcome,
    } = changes.into_plan().into_parts().1
    else {
        return Err("owned public result absent".into());
    };
    let results = business.into_results();
    drop(row);
    drop(scope);
    assert_eq!(
        results[0].as_ref().map(|item| item.plan().outcome()),
        Ok(Outcome::Changed)
    );
    let write = results[0]
        .as_ref()
        .map_err(|_| "write error")?
        .plan()
        .write()
        .ok_or("write absent")?;
    assert_eq!(write.record.version(), 3);
    assert_eq!(write.record.state(), b"completed");
    assert_eq!(outcome.intent().version(), 1);
    assert_eq!(outcome.response_version(), 2);
    assert_eq!(outcome.original_time(), TimestampMillis::new(2_000));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(3_000));
    assert_eq!(outcome.response(), [0]);
    Ok(())
}

#[test]
fn public_item_changes_bind_scalar_status_and_original() -> TestResult {
    item_outcomes()?;
    whole_origins()?;
    matcher_and_count()?;
    invalid_changes()?;
    profile_crossing(false)?;
    profile_crossing(true)?;
    owns_result_after_donors()
}
