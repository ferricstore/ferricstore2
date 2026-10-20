//! Independent physical-leaf receipts and one-gate, prospectively paid apply.

mod literals;

use literals::{all_rejections, original_receipt, receipt};

use std::collections::BTreeMap;

use wf_domain::claim::apply::{
    ClaimDecision, ClaimEncoding, ClaimObservedWindow, ClaimRejection, ClaimWindowEnd,
    ClaimWorkLimits,
};
use wf_domain::claim::{ClaimRecord, DueOrder, LaneKey, ScopedRecordId};
use wf_domain::workflow::admission::AdmissionLimits;
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::indexed::TouchedIndexedGroupView;
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::request::{RetainedMetadata, RetainedOutcome, RetainedStatus};
use wf_domain::workflow::{CommittedCommand, Create, Operation, WorkflowRecord, reduce};
use wf_ports::consensus::PreparedOperation;
use wf_ports::log::{
    ClaimApplyDecision, ClaimPhaseInspection, CodecError, CodecLimits, RecordBatchApplyError,
    claim_response_matches, claim_retained_summary, decide_fresh_claim_phase_observed,
    encode_prepared, inspect_claim_phase_point, prepare_claim_phase,
    public_mutation_retained_status, record_batch_retained_status,
};
use wf_types::identity::PartitionId;
use wf_types::version::{AppliedPosition, LogIdentity, TimestampMillis};

use super::{TestResult, bytes, request};

const WORKSPACE: usize = 64 * 1_024 * 1_024;

fn at(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

fn initial_control() -> TestResult<ControlState> {
    Ok(plan_control(
        ControlState::initial(wf_types::version::CommittedTime::new(
            wf_types::version::HlcTimestamp::new(0, 0)?,
            0,
        )?),
        EntryIdentity::new(at(0), PreparedDigest::new([17; 32])),
        EntryStamp::Unstamped,
    )?
    .next())
}

fn summary(body: &[u8]) -> Result<(RetainedStatus, Option<usize>), CodecError> {
    claim_retained_summary(
        wf_domain::workflow::request::ClientIntent::new(1, [0; 32])
            .map_err(|_| CodecError::UnsupportedVersion)?,
        3,
        body,
        TimestampMillis::new(11),
        AdmissionLimits::default(),
    )
}

fn fixture_record() -> TestResult<WorkflowRecord> {
    Ok(reduce(
        None,
        &CommittedCommand {
            id: wf_types::identity::RecordId::new(vec![0, 255])?,
            now: TimestampMillis::new(5),
            operation: Operation::Create(Create {
                workflow_type: vec![b't', 255],
                state: b"waiting".to_vec(),
                run_at: Some(TimestampMillis::new(9)),
                payload: Some(vec![0, 1, 255]),
            }),
        },
    )?
    .into_parts()
    .1
    .ok_or("fixture create had no write")?
    .record)
}

fn with_window<'current, T>(
    record: &'current WorkflowRecord,
    scope: &wf_domain::workflow::request::RequestScope,
    run: impl for<'query> FnOnce(ClaimObservedWindow<'query, 'current>) -> TestResult<T>,
) -> TestResult<T> {
    let partition = PartitionId::new(b"p".to_vec())?;
    let entry = ClaimRecord {
        record,
        partition_key: b"p",
        priority: 2,
        state_enter_seq: 11,
    };
    let id = ScopedRecordId::for_record(&entry);
    let placements = BTreeMap::from([(partition.clone(), scope.group().clone())]);
    let current = BTreeMap::from([(id, Some(entry))]);
    let due = DueOrder::for_record(&entry).ok_or("fixture has no due")?;
    let parallel = BTreeMap::from([(due, true)]);
    let lane = LaneKey::for_record(&entry).ok_or("fixture has no lane")?;
    let heads = BTreeMap::from([(lane, None)]);
    let policies = BTreeMap::from([(record.workflow_type(), None)]);
    let retry = BTreeMap::from([(record.workflow_type(), None)]);
    let members = BTreeMap::new();
    let encodings = BTreeMap::from([(
        id,
        ClaimEncoding {
            partition: &partition,
            state_key: b"f:{p}:s:\0\xFF",
        },
    )]);
    run(ClaimObservedWindow {
        indexed: TouchedIndexedGroupView {
            group: scope.group(),
            placements: &placements,
            current: &current,
            parallel: &parallel,
            fifo_members: &members,
            fifo_heads: &heads,
            policies: &policies,
            retry_policies: &retry,
        },
        encodings: &encodings,
        parallel: &[entry],
        parallel_end: ClaimWindowEnd::RangeEnd,
        head_lanes: &[],
        heads_complete: true,
    })
}

#[test]
fn op3_decode_admits_three_structural_spines_before_owned_materialization() -> TestResult {
    let codec = CodecLimits::default();
    let mut count = 0;
    for line in include_str!("../../../../tests/prepared/claim-phase-v1-vectors.tsv").lines() {
        if line.starts_with('#') {
            continue;
        }
        let mut fields = line.split('\t');
        let name = fields.next().ok_or("missing vector name")?;
        let encoded = bytes(fields.next().ok_or("missing vector bytes")?)?;
        let expected = request(name)?;
        let PreparedOperation::ClaimPhase(phase) = &expected.operation else {
            return Err("wrong original vector operation".into());
        };
        let occurrences = phase.query.states.as_ref().map_or(0, Vec::len)
            + phase.query.exclude_states.len()
            + phase.query.partitions.as_ref().map_or(0, Vec::len);
        let bound = 4 * encoded.len() + 2_048 + 576 * occurrences;
        assert!(matches!(
            prepare_claim_phase(&encoded, codec, bound - 1),
            Err(RecordBatchApplyError::Workspace { attempted, maximum })
                if attempted == bound && maximum == bound - 1
        ));
        let decoded = prepare_claim_phase(&encoded, codec, bound)?;
        assert_eq!(decoded.request(), &expected, "{name}");
        count += 1;
    }
    assert_eq!(count, 21);
    let mut raw = request("claim_any")?;
    let PreparedOperation::ClaimPhase(phase) = &mut raw.operation else {
        return Err("missing structural fixture".into());
    };
    phase.query.states = Some(vec![vec![b's']; 100_000]);
    phase.query.exclude_states = vec![vec![b'x']; 100_000];
    phase.query.partitions = Some(vec![vec![b'p']; 100_000]);
    let encoded = encode_prepared(&raw, codec)?;
    drop(raw);
    let bound = 4 * encoded.as_bytes().len() + 2_048 + 172_800_000;
    assert!(matches!(
        prepare_claim_phase(encoded.as_bytes(), codec, bound - 1),
        Err(RecordBatchApplyError::Workspace { attempted, maximum })
            if attempted == bound && maximum == bound - 1
    ));
    let decoded = prepare_claim_phase(encoded.as_bytes(), codec, bound)?;
    let envelope = decoded.claim_envelope(at(1)).ok_or("missing query")?;
    let wf_domain::claim::apply::ClaimStates::Only(states) = envelope.query.states else {
        return Err("missing represented state list".into());
    };
    let wf_domain::claim::PartitionFilter::Only(partitions) = envelope.query.partitions else {
        return Err("missing represented partition list".into());
    };
    assert_eq!(
        (
            states.len(),
            envelope.query.exclude_states.len(),
            partitions.len()
        ),
        (100_000, 100_000, 100_000)
    );
    Ok(())
}

#[test]
fn leaf_response_independent_full_literals_and_exhaustive_status_dispatch() -> TestResult {
    let body = original_receipt(0)?;
    assert_eq!(body.len(), 141);
    assert_eq!(summary(&body), Ok((RetainedStatus::Accepted, Some(1))));
    for end in 0..body.len() {
        assert!(summary(&body[..end]).is_err(), "truncation {end}");
    }
    let mut trailing = body.clone();
    trailing.push(0);
    assert!(summary(&trailing).is_err());
    for (index, value) in [(5, 2), (6, 2), (7, 3), (8, 1), (131, 2)] {
        let mut bad = body.clone();
        bad[index] = value;
        assert!(summary(&bad).is_err(), "discriminator {index}");
    }
    for (reason, status) in all_rejections().into_iter().enumerate() {
        let mut rejected = b"FWCR\0\x01\x01".to_vec();
        rejected.push(u8::try_from(reason)?);
        match status {
            ClaimRejection::ValueTooLarge { bytes, maximum } => {
                rejected.extend_from_slice(&u64::try_from(bytes)?.to_be_bytes());
                rejected.extend_from_slice(&u64::try_from(maximum)?.to_be_bytes());
            }
            ClaimRejection::WorkBytes { bytes, maximum } => {
                rejected.extend_from_slice(&bytes.to_be_bytes());
                rejected.extend_from_slice(&maximum.to_be_bytes());
            }
            _ => {}
        }
        assert_eq!(
            summary(&rejected),
            Ok((RetainedStatus::ClaimRejected(status), None))
        );
        rejected.push(0);
        assert!(summary(&rejected).is_err());
    }
    assert!(summary(b"FWCR\0\x01\x01\x10").is_err());
    let intent = request("claim_any")?.intent;
    for version in [1, 2, 4, u16::MAX] {
        assert!(
            claim_retained_summary(
                intent,
                version,
                &body,
                TimestampMillis::new(11),
                AdmissionLimits::default()
            )
            .is_err()
        );
    }
    assert_eq!(
        public_mutation_retained_status(intent, 2, &[0])?,
        RetainedStatus::Accepted
    );
    assert_eq!(
        record_batch_retained_status(
            wf_domain::workflow::request::ClientIntent::record_batch([0; 32]),
            1,
            &[0]
        )?,
        RetainedStatus::Accepted
    );
    assert_eq!(
        summary(&original_receipt(u64::MAX)?),
        Ok((RetainedStatus::Accepted, Some(1)))
    );
    let binary = receipt(b"w:\0\xFF", b"w:\0\xFF:11:1", 0)?;
    assert_eq!(summary(&binary), Ok((RetainedStatus::Accepted, Some(1))));
    check_receipt_neighbors(&body)?;
    Ok(())
}

fn check_receipt_neighbors(body: &[u8]) -> TestResult {
    let intent = request("claim_any")?.intent;
    let admission = AdmissionLimits::default();
    for other in [
        wf_domain::workflow::request::ClientIntent::record_batch([0; 32]),
        wf_domain::workflow::request::ClientIntent::independent_record_batch([0; 32]),
    ] {
        assert!(
            claim_retained_summary(other, 3, body, TimestampMillis::new(11), admission).is_err()
        );
    }
    assert!(claim_retained_summary(intent, 3, body, TimestampMillis::new(12), admission).is_err());
    let cap = AdmissionLimits::new(admission.inline_bytes(), body.len() - 1, 1_000)?;
    assert_eq!(
        claim_retained_summary(intent, 3, body, TimestampMillis::new(11), cap),
        Err(CodecError::ByteLimit)
    );
    let mut short_state = body.to_vec();
    short_state.splice(38..49, [0, 0, 0, 1, b'q']);
    assert_eq!(
        summary(&short_state),
        Ok((RetainedStatus::Accepted, Some(1)))
    );
    let token_cap = AdmissionLimits::new(6, admission.decision_bytes(), 1_000)?;
    assert_eq!(
        claim_retained_summary(intent, 3, &short_state, TimestampMillis::new(11), token_cap),
        Err(CodecError::ValueLimit)
    );
    for (stop, scanned, restart) in [(1_u8, 1_u32, 1_u8), (2, 1, 1)] {
        let mut partial = body.to_vec();
        partial[7] = stop;
        partial[8] = restart;
        partial[9..13].copy_from_slice(&scanned.to_be_bytes());
        assert_eq!(summary(&partial), Ok((RetainedStatus::Accepted, Some(1))));
    }
    let mut empty = b"FWCR\0\x01\0\0\0".to_vec();
    empty.extend_from_slice(&[0; 12]);
    assert_eq!(summary(&empty), Ok((RetainedStatus::Accepted, Some(0))));
    let mut duplicate = body.to_vec();
    duplicate[9..13].copy_from_slice(&2_u32.to_be_bytes());
    duplicate[17..21].copy_from_slice(&2_u32.to_be_bytes());
    duplicate.extend_from_slice(&body[21..]);
    assert!(summary(&duplicate).is_err());
    let second_id = 30 + body.len() - 21;
    duplicate[second_id] = 1;
    assert_eq!(summary(&duplicate), Ok((RetainedStatus::Accepted, Some(2))));
    let one = AdmissionLimits::new(admission.inline_bytes(), admission.decision_bytes(), 1)?;
    assert_eq!(
        claim_retained_summary(intent, 3, &duplicate, TimestampMillis::new(11), one),
        Err(CodecError::ItemLimit)
    );
    for index in [36_usize, 89] {
        let mut mixed = duplicate.clone();
        mixed[index + body.len() - 21] ^= 1;
        assert!(summary(&mixed).is_err(), "mixed-phase field {index}");
    }
    let mut mixed_worker = duplicate;
    for index in [53_usize, 59] {
        mixed_worker[index + body.len() - 21] = b'v';
    }
    assert!(summary(&mixed_worker).is_err());
    let mut created_after = body.to_vec();
    created_after[90..98].copy_from_slice(&13_u64.to_be_bytes());
    assert_eq!(
        summary(&created_after),
        Ok((RetainedStatus::Accepted, Some(1)))
    );
    Ok(())
}

#[test]
fn point_gate_precedes_changed_leaf_query_and_current_authority() -> TestResult {
    let original = request("claim_any")?;
    let encoded = encode_prepared(&original, CodecLimits::default())?;
    let control = plan_control(
        initial_control()?,
        EntryIdentity::new(at(1), encoded.digest()),
        EntryStamp::Stamped(original.stamp),
    )?
    .next();
    let retained = RetainedOutcome::new(
        RetainedMetadata {
            scope: original.scope.clone(),
            request: original.request.clone(),
            intent: original.intent,
            original_position: at(1),
            original_time: TimestampMillis::new(11),
            expires_at: TimestampMillis::new(100),
            status: RetainedStatus::Accepted,
        },
        3,
        original_receipt(0)?,
        AdmissionLimits::default(),
    )?;
    let mut retry = original.clone();
    let PreparedOperation::ClaimPhase(phase) = &mut retry.operation else {
        return Err("missing retry phase".into());
    };
    phase.token_version = 0;
    phase.scan_budget = 0;
    phase.head_budget = 0;
    phase.query.worker.clear();
    phase.query.lease_ms = 0;
    let retry_bytes = encode_prepared(&retry, CodecLimits::default())?;
    let decoded = prepare_claim_phase(retry_bytes.as_bytes(), CodecLimits::default(), WORKSPACE)?;
    let inspection = inspect_claim_phase_point(
        decoded,
        at(2),
        PointGateView {
            scope: &original.scope,
            control,
            outcome: Some(&retained),
            retained_nonempty: true,
        },
        CodecLimits::default(),
        WORKSPACE,
        ClaimWorkLimits::new(1)?,
    )?;
    let ClaimPhaseInspection::Complete(plan) = inspection else {
        return Err("retained request entered Fresh".into());
    };
    let ClaimApplyDecision::Replay(actual) = plan.decision() else {
        return Err("changed query did not replay original receipt".into());
    };
    assert_eq!(*actual, &retained);
    assert_eq!(actual.response().as_ptr(), retained.response().as_ptr());
    assert_eq!(actual.original_time().get(), 11);
    assert_eq!(actual.original_position(), at(1));
    assert_eq!(actual.expires_at().get(), 100);
    assert_eq!(plan.control().next().position().get(), 2);
    let invalid = request("claim_priority_max")?;
    let bytes = encode_prepared(&invalid, CodecLimits::default())?;
    let decoded = prepare_claim_phase(bytes.as_bytes(), CodecLimits::default(), WORKSPACE)?;
    let inspection = inspect_claim_phase_point(
        decoded,
        at(1),
        PointGateView {
            scope: &invalid.scope,
            control: initial_control()?,
            outcome: None,
            retained_nonempty: false,
        },
        CodecLimits::default(),
        WORKSPACE,
        ClaimWorkLimits::default(),
    )?;
    let ClaimPhaseInspection::Fresh(fresh) = inspection else {
        return Err("new invalid query did not reach query classification".into());
    };
    assert_eq!(
        fresh.query_preflight(),
        Err(ClaimRejection::InvalidPriority)
    );
    let rejected = decide_fresh_claim_phase_observed(fresh, None, 0, 100)?;
    let ClaimApplyDecision::Fresh { operation, outcome } = rejected.decision() else {
        return Err("missing finalized rejection".into());
    };
    assert!(matches!(
        operation,
        ClaimDecision::Rejected(ClaimRejection::InvalidPriority)
    ));
    assert_eq!(outcome.response(), b"FWCR\0\x01\x01\x01");
    assert_eq!(outcome.original_time().get(), 11);
    Ok(())
}

#[test]
fn claim_response_binds_every_selected_postimage_and_frontier() -> TestResult {
    let expected = request("claim_any")?;
    let encoded = encode_prepared(&expected, CodecLimits::default())?;
    let record = fixture_record()?;
    let before = record.clone();
    with_window(&record, &expected.scope, |window| {
        let decoded = prepare_claim_phase(encoded.as_bytes(), CodecLimits::default(), WORKSPACE)?;
        let inspection = inspect_claim_phase_point(
            decoded,
            at(1),
            PointGateView {
                scope: &expected.scope,
                control: initial_control()?,
                outcome: None,
                retained_nonempty: false,
            },
            CodecLimits::default(),
            WORKSPACE,
            ClaimWorkLimits::default(),
        )?;
        let ClaimPhaseInspection::Fresh(fresh) = inspection else {
            return Err("missing fresh physical leaf".into());
        };
        assert!(fresh.query_preflight().is_ok());
        let plan = decide_fresh_claim_phase_observed(fresh, Some(window), 0, 100)?;
        let ClaimApplyDecision::Fresh { operation, outcome } = plan.decision() else {
            return Err("missing retained claim decision".into());
        };
        assert_eq!(outcome.response(), original_receipt(0)?);
        assert_eq!(outcome.response_version(), 3);
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        assert_eq!(outcome.original_position(), at(1));
        assert_eq!(outcome.expires_at().get(), 111);
        assert!(claim_response_matches(
            operation,
            outcome.response(),
            TimestampMillis::new(11),
            AdmissionLimits::default()
        )?);
        check_postimage_neighbors(operation, outcome.response())?;
        let mut partial = outcome.response().to_vec();
        partial[7] = 1;
        partial[8] = 1;
        assert!(!claim_response_matches(
            operation,
            &partial,
            TimestampMillis::new(11),
            AdmissionLimits::default()
        )?);
        Ok(())
    })?;
    assert_eq!(record, before);
    with_window(&record, &expected.scope, |_| {
        let decoded = prepare_claim_phase(encoded.as_bytes(), CodecLimits::default(), WORKSPACE)?;
        let inspection = inspect_claim_phase_point(
            decoded,
            at(1),
            PointGateView {
                scope: &expected.scope,
                control: initial_control()?,
                outcome: None,
                retained_nonempty: false,
            },
            CodecLimits::default(),
            WORKSPACE,
            ClaimWorkLimits::default(),
        )?;
        let ClaimPhaseInspection::Fresh(fresh) = inspection else {
            return Err("missing fresh None-window control".into());
        };
        assert!(matches!(
            decide_fresh_claim_phase_observed(fresh, None, 0, 100),
            Err(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority))
        ));
        Ok(())
    })?;
    Ok(())
}

fn check_postimage_neighbors(operation: &ClaimDecision<'_>, body: &[u8]) -> TestResult {
    for index in [25_usize, 31, 37, 48, 73, 89, 97, 113, 130, 138] {
        let mut changed = body.to_vec();
        changed[index] ^= 1;
        assert_eq!(summary(&changed), Ok((RetainedStatus::Accepted, Some(1))));
        assert!(
            !claim_response_matches(
                operation,
                &changed,
                TimestampMillis::new(11),
                AdmissionLimits::default()
            )?,
            "healthy changed postimage byte {index}"
        );
    }
    for index in [54_usize, 65, 81, 105, 114] {
        let mut changed = body.to_vec();
        changed[index] ^= 1;
        assert!(
            !claim_response_matches(
                operation,
                &changed,
                TimestampMillis::new(11),
                AdmissionLimits::default()
            )
            .unwrap_or(false),
            "invalid postimage byte {index}"
        );
    }
    let mut present_empty = body.to_vec();
    present_empty[139] = 1;
    present_empty.splice(140..140, [0; 4]);
    assert_eq!(
        summary(&present_empty),
        Ok((RetainedStatus::Accepted, Some(1)))
    );
    assert!(!claim_response_matches(
        operation,
        &present_empty,
        TimestampMillis::new(11),
        AdmissionLimits::default()
    )?);
    Ok(())
}
