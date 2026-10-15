//! Private mutation-batch bytes join indexed decisions and immutable retention.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use wf_domain::claim::Claim;
use wf_domain::lease::{ExtendLease, LeaseGuard, LeaseToken};
use wf_domain::policy::{Backoff, BackoffKind, InstalledRetryPolicy, RetryOverride};
use wf_domain::retry::{JitterSample, PolicyRetry, Retry, RetryTime};
use wf_domain::workflow::admission::{AdmissionError, AdmissionLimits};
use wf_domain::workflow::batch::{BatchRejection, GroupView};
use wf_domain::workflow::control::ControlState;
use wf_domain::workflow::indexed::{IndexRejection, IndexedDecision, IndexedGroupView};
use wf_domain::workflow::request::gate::GateView;
use wf_domain::workflow::request::indexed::IndexedRequestView;
use wf_domain::workflow::request::{ClientIntent, RequestScope, RetainedStatus};
use wf_domain::workflow::{Cancel, CommittedCommand, Create, Operation, Terminal, Transition};
use wf_ports::consensus::{
    PreparedBatchItem, PreparedClaimPhase, PreparedClaimQuery, PreparedOperation, PreparedRequest,
};
use wf_ports::log::{
    CodecError, CodecLimits, RecordBatchApplyDecision, RecordBatchApplyError, RecordBatchApplyPlan,
    decide_record_batch, decode_prepared, encode_prepared, encode_record_batch,
};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{
    AppliedPosition, CommittedTime, Fence, HlcTimestamp, LogIdentity, TimestampMillis,
};

mod replay;
mod source;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const WORKSPACE: usize = 64 * 1_024 * 1_024;
const MIN_INTENT: &str = "25a2a227783574ca47f08fb9036bb53611b0cb7b509f7a81ab36a18cf02c31ec";
const MIN_FRAME: &str = "4657435000010100000001740000000167000000017a0000000171000525a2a227783574ca47f08fb9036bb53611b0cb7b509f7a81ab36a18cf02c31ec000000000000000100000000000000000001000000010000000170000000016900000000000000000101000000017200000001710000";

fn limits() -> CodecLimits {
    CodecLimits::new(AdmissionLimits::default())
}

fn clock(now: u64) -> TestResult<CommittedTime> {
    Ok(CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?)
}

fn log(index: u64) -> LogIdentity {
    LogIdentity::new(1, 7, AppliedPosition::new(index))
}

fn scope() -> TestResult<RequestScope> {
    Ok(RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    ))
}

fn create(id: &[u8]) -> TestResult<PreparedBatchItem> {
    Ok(PreparedBatchItem {
        partition: PartitionId::new(b"p".to_vec())?,
        command: CommittedCommand {
            id: RecordId::new(id.to_vec())?,
            now: TimestampMillis::new(1),
            operation: Operation::Create(Create {
                workflow_type: b"r".to_vec(),
                state: b"q".to_vec(),
                run_at: None,
                payload: None,
            }),
        },
        create_priority: 0,
    })
}

fn request(items: Vec<PreparedBatchItem>) -> TestResult<PreparedRequest> {
    Ok(PreparedRequest {
        scope: scope()?,
        request: RequestId::new(b"q".to_vec())?,
        intent: ClientIntent::new(1, [0xaa; 32])?,
        stamp: clock(1)?,
        operation: PreparedOperation::Batch(items),
    })
}

fn hex(value: &str) -> TestResult<Vec<u8>> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?))
        .collect()
}

fn fingerprint(value: PreparedRequest) -> TestResult<(ClientIntent, [u8; 32])> {
    let frame = encode_record_batch(value, limits(), WORKSPACE)?;
    let decoded = decode_prepared(frame.as_bytes(), limits())?;
    Ok((decoded.request().intent, *frame.digest().as_bytes()))
}

fn guard() -> TestResult<LeaseGuard> {
    Ok(LeaseGuard {
        token: LeaseToken::new(b"tok".to_vec())?,
        fence: Fence::new(7),
    })
}

fn full_operations() -> TestResult<Vec<Operation>> {
    Ok(vec![
        Operation::Create(Create {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            run_at: Some(TimestampMillis::new(9)),
            payload: Some(Vec::new()),
        }),
        Operation::Transition(Transition {
            from: b"running".to_vec(),
            to: b"waiting".to_vec(),
            fence: Fence::new(7),
            lease_token: Some(LeaseToken::new(b"tok".to_vec())?),
            run_at: Some(TimestampMillis::new(9)),
            payload: Some(Vec::new()),
        }),
        Operation::Complete(Terminal {
            guard: guard()?,
            payload: Some(Vec::new()),
            value: Some(b"v\0\xff".to_vec()),
        }),
        Operation::Fail(Terminal {
            guard: guard()?,
            payload: Some(b"p".to_vec()),
            value: Some(b"e".to_vec()),
        }),
        Operation::Retry(Retry {
            guard: guard()?,
            run_at: RetryTime::Explicit(TimestampMillis::new(9)),
            payload: Some(Vec::new()),
            error: Some(b"e".to_vec()),
        }),
        Operation::RetryWithPolicy(PolicyRetry {
            guard: guard()?,
            run_at: Some(TimestampMillis::new(9)),
            policy_override: RetryOverride {
                max_retries: Some(3),
                backoff: Some(Backoff {
                    kind: BackoffKind::Exponential,
                    base_ms: 11,
                    max_ms: 999,
                    jitter_pct: 25,
                }),
                exhausted_to: Some(b"failed".to_vec()),
            },
            jitter: JitterSample::new(42),
            payload: Some(Vec::new()),
            error: Some(b"e".to_vec()),
        }),
        Operation::ExtendLease(ExtendLease {
            guard: guard()?,
            lease_ms: 19,
        }),
        Operation::Cancel(Cancel {
            fence: Fence::new(7),
            lease_token: Some(LeaseToken::new(b"tok".to_vec())?),
            expected_state: Some(b"q".to_vec()),
            expected_version: Some(4),
            error: Some(Vec::new()),
        }),
    ])
}

fn assert_operation_vectors() -> TestResult {
    // Independently packed typed field streams, before any new production body.
    let expected = [
        "4024ed5e7574859bb911d41c081239fda0c9c30185fc8ec6c03c1a1a23adb094",
        "bdda0cba78e478e9301d99d4a91102c2ad82b73efde881dc95108c1157f6145e",
        "a4ab338a29ebe4a55b5cebd78b09df9149f3c0dfe0a2067c888b70d30649c94f",
        "44567962ba8f06b9eb2b04c5ccdfe3e5860b2919c74a24ffa8daaf67d394f8ea",
        "e8e7a8292fc9ef00862cb256fec0e5bf9e84a3192dec4d574fbed8b1063f46fc",
        "ea1f1f9874bc8eeb75b4677f42a326fb21e193a980ce28d274078c4b0a596f46",
        "cda7c587bd5652682ff406eceba39a6cc339a0cf5d32bd985ee61220b6aee40b",
        "9e729ae9d8e42a6d5ef153350618e043dbeb456ae371c0e9654d357bc6336e63",
    ];
    for (operation, expected) in full_operations()?.into_iter().zip(expected) {
        let mut item = create(b"i")?;
        item.create_priority = if matches!(operation, Operation::Create(_)) {
            2
        } else {
            0
        };
        item.command.operation = operation;
        let value = request(vec![item])?;
        let (intent, prepared) = fingerprint(value.clone())?;
        assert_eq!(intent.version(), 5);
        assert_eq!(intent.digest().as_slice(), hex(expected)?);
        let mut regenerated = value;
        regenerated.stamp = clock(2)?;
        regenerated.request = RequestId::new(b"other-request".to_vec())?;
        regenerated.scope = RequestScope::new(
            TenantId::new(b"t".to_vec())?,
            GroupId::new(b"new-group".to_vec())?,
            RecoveryId::new(b"new-recovery".to_vec())?,
        );
        regenerated.intent = ClientIntent::new(1, [0x55; 32])?;
        if let PreparedOperation::Batch(items) = &mut regenerated.operation {
            items[0].command.now = TimestampMillis::new(2);
            if let Operation::RetryWithPolicy(retry) = &mut items[0].command.operation {
                retry.jitter = JitterSample::new(99);
            }
            if !matches!(items[0].command.operation, Operation::Create(_)) {
                items[0].create_priority = 255;
            }
        }
        let (again, changed_prepared) = fingerprint(regenerated)?;
        assert_eq!(again, intent);
        assert_ne!(changed_prepared, prepared);
    }
    Ok(())
}

fn assert_caller_identity() -> TestResult {
    let base = request(vec![create(b"i")?])?;
    let expected = fingerprint(base.clone())?.0;
    let mut present = base.clone();
    if let PreparedOperation::Batch(items) = &mut present.operation {
        let Operation::Create(value) = &mut items[0].command.operation else {
            return Err("not create".into());
        };
        value.payload = Some(Vec::new());
    }
    assert_ne!(fingerprint(present)?.0, expected);
    let mut explicit_now = base.clone();
    if let PreparedOperation::Batch(items) = &mut explicit_now.operation {
        let Operation::Create(value) = &mut items[0].command.operation else {
            return Err("not create".into());
        };
        value.run_at = Some(TimestampMillis::new(1));
    }
    assert_ne!(fingerprint(explicit_now)?.0, expected);
    for field in 0..4 {
        let mut changed = base.clone();
        if field == 0 {
            changed.scope = RequestScope::new(
                TenantId::new(b"other".to_vec())?,
                changed.scope.group().clone(),
                changed.scope.recovery().clone(),
            );
        } else if let PreparedOperation::Batch(items) = &mut changed.operation {
            match field {
                1 => items[0].partition = PartitionId::new(b"other".to_vec())?,
                2 => items[0].command.id = RecordId::new(b"other".to_vec())?,
                _ => items[0].create_priority = 1,
            }
        }
        assert_ne!(fingerprint(changed)?.0, expected);
    }
    let mut ordered = request(vec![create(b"i")?, create(b"j")?])?;
    let forward = fingerprint(ordered.clone())?.0;
    if let PreparedOperation::Batch(items) = &mut ordered.operation {
        items.reverse();
    }
    assert_ne!(fingerprint(ordered)?.0, forward);
    let mut item = create(b"i")?;
    item.command.operation = Operation::Retry(Retry {
        guard: guard()?,
        run_at: RetryTime::PolicyResolved(TimestampMillis::new(9)),
        payload: None,
        error: None,
    });
    let resolved = fingerprint(request(vec![item.clone()])?)?.0;
    if let Operation::Retry(value) = &mut item.command.operation {
        value.run_at = RetryTime::PolicyResolved(TimestampMillis::new(99));
    }
    assert_eq!(fingerprint(request(vec![item.clone()])?)?.0, resolved);
    if let Operation::Retry(value) = &mut item.command.operation {
        value.run_at = RetryTime::Explicit(TimestampMillis::new(99));
    }
    assert_ne!(fingerprint(request(vec![item])?)?.0, resolved);
    Ok(())
}

fn assert_header_matrix(batch_intent: ClientIntent) -> TestResult {
    let mut phase = request(vec![create(b"i")?])?;
    phase.operation = PreparedOperation::ClaimPhase(PreparedClaimPhase {
        query: PreparedClaimQuery {
            workflow_type: b"r".to_vec(),
            states: None,
            exclude_states: Vec::new(),
            partitions: None,
            priority: None,
            worker: b"w".to_vec(),
            lease_ms: 1,
            limit: 1,
        },
        token_version: 1,
        first_ordinal: 0,
        scan_budget: 1,
        head_budget: 1,
    });
    let old = encode_prepared(&phase, limits())?;
    assert_eq!(
        decode_prepared(old.as_bytes(), limits())?
            .request()
            .intent
            .version(),
        1
    );
    with_empty(old.as_bytes(), limits(), WORKSPACE, |result| {
        assert!(matches!(
            result,
            Err(RecordBatchApplyError::Encoding(CodecError::UnsupportedTag))
        ));
        Ok(())
    })?;
    phase.intent = batch_intent;
    assert!(matches!(
        encode_prepared(&phase, limits()),
        Err(CodecError::UnsupportedVersion)
    ));
    let mut invalid = old.as_bytes().to_vec();
    invalid[27..29].copy_from_slice(&5_u16.to_be_bytes());
    assert!(matches!(
        decode_prepared(&invalid, limits()),
        Err(CodecError::UnsupportedVersion)
    ));
    Ok(())
}

#[test]
fn record_batch_intent_and_frame_bind_all_mutation_fields() -> TestResult {
    let canonical = hex(
        "57462d5245434f52442d424154434800563500000000017400000001000000017000000001690100000000017200000001710000",
    )?;
    assert_eq!(canonical.len(), 52);
    assert_eq!(Sha256::digest(&canonical).as_slice(), hex(MIN_INTENT)?);
    let frame = encode_record_batch(request(vec![create(b"i")?])?, limits(), WORKSPACE)?;
    assert_eq!(frame.as_bytes(), hex(MIN_FRAME)?);
    assert_eq!(frame.as_bytes().len(), 115);
    assert_eq!(
        frame.digest().as_bytes().as_slice(),
        hex("d49c43b59fcd1be1f457b8f8fbc0d63f45d719f43c3fe80ed3b3cc18ac2c9840")?
    );
    let decoded = decode_prepared(frame.as_bytes(), limits())?;
    assert_eq!(decoded.request().intent.version(), 5);
    assert_eq!(
        decoded.request().intent.digest().as_slice(),
        hex(MIN_INTENT)?
    );
    assert_eq!(decoded.state_key(0).ok_or("key absent")?.len(), 41);
    assert_header_matrix(decoded.request().intent)?;
    assert_operation_vectors()?;
    assert_caller_identity()?;
    for reclaim in [false, true] {
        let mut item = create(b"i")?;
        let claim = Claim {
            expected_state: b"q".to_vec(),
            worker: b"w".to_vec(),
            token: LeaseToken::new(b"tok".to_vec())?,
            lease_ms: 1,
        };
        item.command.operation = if reclaim {
            Operation::Reclaim(claim)
        } else {
            Operation::Claim(claim)
        };
        let value = request(vec![item])?;
        let old = encode_prepared(&value, limits())?;
        assert_eq!(
            decode_prepared(old.as_bytes(), limits())?
                .request()
                .intent
                .version(),
            1
        );
        assert!(matches!(
            encode_record_batch(value, limits(), WORKSPACE),
            Err(RecordBatchApplyError::Encoding(CodecError::UnsupportedTag))
        ));
    }
    let mut unsupported = frame.as_bytes().to_vec();
    unsupported[27..29].copy_from_slice(&6_u16.to_be_bytes());
    assert!(matches!(
        decode_prepared(&unsupported, limits()),
        Err(CodecError::UnsupportedVersion)
    ));
    Ok(())
}

fn with_empty(
    bytes: &[u8],
    profile: CodecLimits,
    workspace: usize,
    check: impl FnOnce(Result<RecordBatchApplyPlan<'_>, RecordBatchApplyError>) -> TestResult,
) -> TestResult {
    let scope = scope()?;
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition, scope.group().clone())]);
    let current = BTreeMap::new();
    let records = BTreeMap::new();
    let parallel = BTreeMap::new();
    let fifo = BTreeMap::new();
    let policies = BTreeMap::new();
    let retries: BTreeMap<&[u8], &InstalledRetryPolicy> = BTreeMap::new();
    let outcomes = BTreeMap::new();
    let view = IndexedRequestView {
        gate: GateView {
            scope: &scope,
            control: ControlState::initial(clock(0)?),
            outcomes: &outcomes,
        },
        group: IndexedGroupView {
            batch: GroupView {
                group: scope.group(),
                placements: &placements,
                records: &current,
            },
            records: &records,
            parallel: &parallel,
            fifo: &fifo,
            policies: &policies,
        },
    };
    check(decide_record_batch(
        bytes,
        log(0),
        view,
        &retries,
        100,
        profile,
        workspace,
    ))
}

fn assert_workspace(bytes: &[u8], maximum: usize, attempted: usize) -> TestResult {
    with_empty(bytes, limits(), maximum, |result| {
        assert!(
            matches!(result, Err(RecordBatchApplyError::Workspace { attempted: actual, maximum: cap })
            if actual == attempted && cap == maximum)
        );
        Ok(())
    })
}

fn assert_present_value_admission() -> TestResult {
    let mut item = create(b"i")?;
    let Operation::Create(value) = &mut item.command.operation else {
        return Err("not create".into());
    };
    value.payload = Some(b"v".to_vec());
    let frame = encode_record_batch(request(vec![item])?, limits(), WORKSPACE)?;
    assert_eq!(
        decode_prepared(frame.as_bytes(), limits())?.value_bytes(0),
        Some([Some(b"v".as_slice()), None])
    );
    // Command4 + actual key41 + partition1 + PRESENT encoded value1 = V47.
    // Core charge is 4*47+1024=1212; omitting the derived occurrence gives1208.
    for budget in [1_211, 1_212] {
        let profile = CodecLimits::new(AdmissionLimits::new(1_024, budget, 1)?);
        with_empty(frame.as_bytes(), profile, WORKSPACE, |result| {
            let plan: RecordBatchApplyPlan<'_> = result?;
            let RecordBatchApplyDecision::Fresh { operation, outcome } = plan.decision() else {
                return Err("not fresh".into());
            };
            if budget == 1_211 {
                let reason =
                    IndexRejection::Batch(BatchRejection::Admission(AdmissionError::DecisionBytes));
                assert_eq!(operation, &IndexedDecision::Rejected(reason));
                assert_eq!(outcome.status(), RetainedStatus::IndexedRejected(reason));
                assert_eq!(outcome.response(), [1, 1, 4]);
            } else {
                let IndexedDecision::Accepted(batch) = operation else {
                    return Err("exact core budget rejected".into());
                };
                assert_eq!(
                    batch.batch().items()[0]
                        .plan()
                        .write()
                        .ok_or("write absent")?
                        .record
                        .payload(),
                    Some(b"v".as_slice())
                );
                assert_eq!(outcome.response(), [0]);
            }
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn record_batch_workspace_refuses_before_each_owned_stage() -> TestResult {
    assert_present_value_admission()?;
    let value = request(vec![create(b"i")?])?;
    // Frozen source grammar: owned input8, U1544, E115, D=B+3532, Fresh=2B+3980.
    assert!(matches!(
        encode_record_batch(value.clone(), limits(), 1_658),
        Err(RecordBatchApplyError::Workspace {
            attempted: 1_659,
            maximum: 1_658
        })
    ));
    let frame = encode_record_batch(value, limits(), 1_659)?;
    assert_eq!(frame.as_bytes(), hex(MIN_FRAME)?);
    assert_workspace(frame.as_bytes(), 16_780_747, 16_780_748)?;
    assert_workspace(frame.as_bytes(), 16_780_748, 33_558_412)?;
    assert_workspace(frame.as_bytes(), 33_558_411, 33_558_412)?;
    with_empty(frame.as_bytes(), limits(), 33_558_412, |result| {
        let plan: RecordBatchApplyPlan<'_> = result?;
        let RecordBatchApplyDecision::Fresh { operation, outcome } = plan.decision() else {
            return Err("not fresh".into());
        };
        let IndexedDecision::Accepted(batch) = operation else {
            return Err("create rejected".into());
        };
        assert_eq!(batch.consumed_ordinals(), 1);
        assert_eq!(
            batch
                .metadata()
                .next()
                .ok_or("metadata absent")?
                .state_enter_seq,
            0
        );
        assert_eq!(
            batch.batch().items()[0]
                .plan()
                .write()
                .ok_or("write absent")?
                .record
                .state(),
            b"q"
        );
        assert_eq!(outcome.response(), [0]);
        assert_eq!(outcome.status(), RetainedStatus::Accepted);
        assert_eq!(outcome.response_version(), 1);
        assert_eq!(outcome.original_position(), log(0));
        assert_eq!(outcome.expires_at(), TimestampMillis::new(101));
        Ok(())
    })?;
    let items = (0..1_000)
        .map(|id| create(format!("i{id:04}").as_bytes()))
        .collect::<TestResult<Vec<_>>>()?;
    let thousand = encode_record_batch(request(items)?, limits(), WORKSPACE)?;
    with_empty(thousand.as_bytes(), limits(), WORKSPACE, |result| {
        let plan: RecordBatchApplyPlan<'_> = result?;
        let RecordBatchApplyDecision::Fresh {
            operation: IndexedDecision::Accepted(batch),
            outcome,
        } = plan.decision()
        else {
            return Err("default 1000-item batch rejected".into());
        };
        assert_eq!(batch.batch().items().len(), 1_000);
        assert_eq!(batch.consumed_ordinals(), 1_000);
        assert_eq!(
            batch
                .metadata()
                .map(|row| row.state_enter_seq)
                .collect::<Vec<_>>(),
            (0..1_000).collect::<Vec<u128>>()
        );
        assert_eq!(outcome.response(), [0]);
        Ok(())
    })?;
    let mut damaged = frame.as_bytes().to_vec();
    damaged[0] = 0;
    with_empty(&damaged, limits(), 0, |result| {
        assert!(matches!(
            result,
            Err(RecordBatchApplyError::Encoding(CodecError::Malformed))
        ));
        Ok(())
    })?;
    Ok(())
}
