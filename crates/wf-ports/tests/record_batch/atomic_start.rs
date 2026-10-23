//! Private atomic-start framing, actual leased replies and independent bounds.

use super::*;
use wf_domain::claim::{LaneKey, ScopedRecordId};
use wf_domain::history::EventKind;
use wf_domain::policy::definition::{
    FieldPatch, PolicyPatch, PolicySnapshot, StateChange, StateMode, StatePolicyPatch,
};
use wf_domain::policy::installation::{InstallCommand, PolicyLimits, prepare_install};
use wf_domain::workflow::admission::AdmissionError::{DecisionBytes, ValueTooLarge};
use wf_domain::workflow::batch::BatchRejection::Admission;
use wf_domain::workflow::control::{EntryIdentity, EntryStamp, PreparedDigest, plan_control};
use wf_domain::workflow::indexed::IndexRejection::Batch;
use wf_domain::workflow::indexed::{IndexRows, TouchedIndexedGroupView};
use wf_domain::workflow::request::RetainedStatus::{Accepted, IndexedRejected};
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::{Outcome, StartAndClaim};
use wf_ports::log::{
    RecordBatchInspection, atomic_start_response_matches, atomic_start_retained_summary,
    decide_fresh_atomic_start_touched, encode_atomic_start, inspect_atomic_start_point,
    prepare_atomic_start, prepare_record_batch, record_batch_retained_summary,
};

mod response;

const ATOMIC_CANONICAL: &str = concat!(
    "57462d41544f4d49432d535441525400563700000000017400000001700000000169",
    "00000001720000000171000000037700ff00000000000000010000"
);
const ATOMIC_INTENT: &str = "99eb58f7389d4e334a19b606718540304a002ef13e16290292f21baecba6ea9c";
const ATOMIC_FRAME: &str = concat!(
    "4657435000010100000001740000000167000000017a00000001710007",
    "99eb58f7389d4e334a19b606718540304a002ef13e16290292f21baecba6ea9c",
    "0000000000000011000000000000000000110000000100000001700000000169",
    "0000000000000000110b00000001720000000171000000037700ff000000000000000100"
);
const ATOMIC_RESPONSE: &str = concat!(
    "465741530001000000000170000000016900000001720000000171000000037700ff",
    "000000087700ff3a31373a31000000000000000100000000000000010000000000000012",
    "0000000000000011000000000000001100000000000000010000000000000000",
    "000000000100000000000000"
);

fn atomic_request(worker: &[u8], payload: Option<&[u8]>, now: u64) -> TestResult<PreparedRequest> {
    let mut item = create(b"i")?;
    item.command.now = TimestampMillis::new(now);
    item.command.operation = Operation::StartAndClaim(StartAndClaim {
        workflow_type: b"r".to_vec(),
        state: b"q".to_vec(),
        worker: worker.to_vec(),
        lease_ms: 1,
        payload: payload.map(<[u8]>::to_vec),
    });
    let mut value = request(vec![item])?;
    value.stamp = clock(now)?;
    Ok(value)
}

fn with_atomic_view<T>(
    policy: Option<&PolicySnapshot>,
    check: impl for<'view> FnOnce(
        PointGateView<'view, 'static>,
        TouchedIndexedGroupView<'view, 'static>,
    ) -> TestResult<T>,
) -> TestResult<T> {
    let query = scope()?;
    let placements = BTreeMap::from([(PartitionId::new(b"p".to_vec())?, query.group().clone())]);
    let current = BTreeMap::from([(
        ScopedRecordId {
            partition_key: b"p",
            id: b"i",
        },
        None,
    )]);
    let parallel = BTreeMap::new();
    let fifo_members = BTreeMap::new();
    let fifo_heads = BTreeMap::from([(
        LaneKey {
            workflow_type: b"r",
            state: b"q",
            partition_key: b"p",
        },
        None,
    )]);
    let policies = BTreeMap::from([(b"r".as_slice(), policy)]);
    let retry_policies = BTreeMap::new();
    let control = plan_control(
        ControlState::initial(clock(0)?),
        EntryIdentity::new(log(0), PreparedDigest::new([0x31; 32])),
        EntryStamp::Unstamped,
    )?
    .next();
    check(
        PointGateView {
            scope: &query,
            control,
            retained_nonempty: false,
            outcome: None,
        },
        TouchedIndexedGroupView {
            group: query.group(),
            placements: &placements,
            current: &current,
            parallel: &parallel,
            fifo_members: &fifo_members,
            fifo_heads: &fifo_heads,
            policies: &policies,
            retry_policies: &retry_policies,
        },
    )
}

#[test]
fn atomic_start_frame_is_isolated_from_legacy_profiles() -> TestResult {
    let input = atomic_request(b"w\0\xff", None, 17)?;
    let encoded = encode_atomic_start(input.clone(), limits(), WORKSPACE);
    assert!(encoded.is_ok(), "{:?}", encoded.as_ref().err());
    let encoded = encoded?;
    assert_eq!(hex(ATOMIC_CANONICAL)?.len(), 61);
    assert_eq!(
        Sha256::digest(hex(ATOMIC_CANONICAL)?).as_slice(),
        hex(ATOMIC_INTENT)?
    );
    assert_eq!(encoded.as_bytes(), hex(ATOMIC_FRAME)?);
    assert_eq!(encoded.as_bytes().len(), 129);
    assert_eq!(
        encoded.digest().as_bytes().as_slice(),
        hex("4b9473323a3513fd70ae3ce42f806d0230bb884bcfd5bab2ce09a833c5afcce3")?
    );
    let decoded = prepare_atomic_start(encoded.as_bytes(), limits(), WORKSPACE)?;
    assert_eq!(decoded.request().intent.version(), 7);
    assert_eq!(
        decoded.request().intent.digest().as_slice(),
        hex(ATOMIC_INTENT)?
    );
    assert_eq!(decoded.request().scope, input.scope);
    assert_eq!(decoded.request().request, input.request);
    assert_eq!(decoded.request().stamp, input.stamp);
    assert!(decoded.request().operation == input.operation);
    assert_eq!(decoded.digest(), encoded.digest());
    assert_eq!(decoded.encoded(), encoded.as_bytes());
    assert_eq!(decoded.state_key(0).ok_or("missing actual key")?.len(), 41);
    assert_eq!(decoded.value_bytes(0), Some([None, None]));
    assert_eq!(
        decode_prepared(encoded.as_bytes(), limits()).err(),
        Some(CodecError::UnsupportedVersion)
    );
    assert_eq!(
        encode_prepared(&input, limits()).err(),
        Some(CodecError::UnsupportedTag)
    );
    assert_eq!(
        encode_record_batch(input, limits(), WORKSPACE).err(),
        Some(RecordBatchApplyError::Encoding(CodecError::UnsupportedTag))
    );
    assert!(prepare_record_batch(encoded.as_bytes(), limits(), WORKSPACE).is_err());
    let legacy_create = encode_record_batch(request(vec![create(b"i")?])?, limits(), WORKSPACE)?;
    let mut legacy_claim = request(vec![create(b"i")?])?;
    legacy_claim.operation = PreparedOperation::ClaimPhase(PreparedClaimPhase {
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
    for old in [legacy_create, encode_prepared(&legacy_claim, limits())?] {
        assert!(prepare_atomic_start(old.as_bytes(), limits(), WORKSPACE).is_err());
    }
    for case in 0..7 {
        let mut damaged = encoded.as_bytes().to_vec();
        match case {
            0 => damaged[28] = 5,
            1 => damaged[6] = 3,
            2 => damaged[102] = 1,
            3 => damaged[79..83].copy_from_slice(&0_u32.to_be_bytes()),
            4 => damaged[79..83].copy_from_slice(&2_u32.to_be_bytes()),
            5 => damaged[113..117].copy_from_slice(&u32::MAX.to_be_bytes()),
            _ => damaged.push(0),
        }
        assert!(
            prepare_atomic_start(&damaged, limits(), WORKSPACE).is_err(),
            "{case}"
        );
    }
    let mut wrong_digest = encoded.as_bytes().to_vec();
    wrong_digest[29] ^= 1;
    assert_eq!(
        prepare_atomic_start(&wrong_digest, limits(), WORKSPACE).err(),
        Some(RecordBatchApplyError::IntentMismatch)
    );
    Ok(())
}

#[test]
fn atomic_start_intent_binds_inputs_not_leader_time() -> TestResult {
    let original = encode_atomic_start(atomic_request(b"w\0\xff", None, 17)?, limits(), WORKSPACE)?;
    let expected = prepare_atomic_start(original.as_bytes(), limits(), WORKSPACE)?
        .request()
        .intent;
    assert_eq!(expected.digest().as_slice(), hex(ATOMIC_INTENT)?);
    let mut present = None;
    for field in 0..16 {
        let mut changed = atomic_request(b"w\0\xff", None, 17)?;
        let PreparedOperation::Batch(items) = &mut changed.operation else {
            return Err("atomic fixture is not a batch".into());
        };
        match field {
            0 => {
                changed.scope = RequestScope::new(
                    TenantId::new(b"other".to_vec())?,
                    changed.scope.group().clone(),
                    changed.scope.recovery().clone(),
                );
            }
            1 => items[0].partition = PartitionId::new(b"other".to_vec())?,
            2 => items[0].command.id = RecordId::new(b"other".to_vec())?,
            3..=6 | 8 | 14 | 15 => {
                let Operation::StartAndClaim(value) = &mut items[0].command.operation else {
                    return Err("atomic fixture operation missing".into());
                };
                match field {
                    3 => value.workflow_type = b"other".to_vec(),
                    4 => value.state = b"other".to_vec(),
                    5 => value.worker = b"other".to_vec(),
                    6 => value.lease_ms = 2,
                    8 => value.payload = Some(Vec::new()),
                    14 => value.payload = Some(b"v".to_vec()),
                    _ => value.payload = Some(b"x".to_vec()),
                }
            }
            7 => items[0].create_priority = 1,
            9 => {
                changed.stamp = clock(18)?;
                items[0].command.now = TimestampMillis::new(18);
            }
            10 => changed.request = RequestId::new(b"other".to_vec())?,
            11 => {
                changed.scope = RequestScope::new(
                    changed.scope.tenant().clone(),
                    GroupId::new(b"other".to_vec())?,
                    changed.scope.recovery().clone(),
                );
            }
            12 => {
                changed.scope = RequestScope::new(
                    changed.scope.tenant().clone(),
                    changed.scope.group().clone(),
                    RecoveryId::new(b"other".to_vec())?,
                );
            }
            _ => changed.intent = ClientIntent::new(1, [0x55; 32])?,
        }
        let frame = encode_atomic_start(changed.clone(), limits(), WORKSPACE)?;
        let decoded = prepare_atomic_start(frame.as_bytes(), limits(), WORKSPACE)?;
        assert_eq!(decoded.request().scope, changed.scope);
        assert_eq!(decoded.request().request, changed.request);
        assert_eq!(decoded.request().stamp, changed.stamp);
        assert!(decoded.request().operation == changed.operation);
        assert_eq!(decoded.digest(), frame.digest());
        assert_eq!(
            decoded.digest().as_bytes().as_slice(),
            Sha256::digest(frame.as_bytes()).as_slice()
        );
        let actual = decoded.request().intent;
        assert_eq!(actual == expected, (9..=13).contains(&field), "{field}");
        if field == 14 {
            present = Some(actual);
        }
        if field == 15 {
            assert_ne!(
                actual.digest(),
                present.ok_or("Some(v) intent absent")?.digest()
            );
        }
        if (9..=12).contains(&field) {
            assert_ne!(frame.as_bytes(), original.as_bytes());
        }
        if field == 13 {
            assert_eq!(frame.as_bytes(), original.as_bytes());
        }
    }
    Ok(())
}

#[test]
fn atomic_start_retains_exact_admission_and_fifo_refusal() -> TestResult {
    let state = StatePolicyPatch {
        mode: FieldPatch::Set(StateMode::Fifo),
        ..StatePolicyPatch::default()
    };
    let patch = PolicyPatch {
        states: BTreeMap::from([(b"q".to_vec(), StateChange::Patch(state))]),
        ..PolicyPatch::default()
    };
    let install = InstallCommand::low_level(b"r".to_vec(), patch);
    let candidate = prepare_install(None, 0, &install, PolicyLimits::default())?;
    let bounds = [(8, 1_247), (8, 1_248), (7, 4_096), (6, 4_096), (8, 4_096)];
    for (case, (inline, budget)) in bounds.into_iter().enumerate() {
        let profile = CodecLimits::new(AdmissionLimits::new(inline, budget, 1)?);
        let (worker, now) = if case == 3 {
            (b"w".as_slice(), 0)
        } else {
            (b"w\0\xff".as_slice(), 17)
        };
        let policy = (case == 4).then_some(candidate.policy());
        let plan = with_atomic_view(policy, |gate, view| {
            let frame =
                encode_atomic_start(atomic_request(worker, None, now)?, profile, WORKSPACE)?;
            let RecordBatchInspection::Fresh(fresh) = inspect_atomic_start_point(
                prepare_atomic_start(frame.as_bytes(), profile, WORKSPACE)?,
                log(1),
                gate,
                profile,
                WORKSPACE,
            )?
            else {
                return Err("refusal fixture not fresh".into());
            };
            Ok(decide_fresh_atomic_start_touched(fresh, view, 0, 100)?)
        })?;
        let (_, RecordBatchApplyDecision::Fresh { operation, outcome }) = plan.into_parts() else {
            return Err("missing actual retained refusal".into());
        };
        let reason = match case {
            0 => Some(Batch(Admission(DecisionBytes))),
            1 => None,
            2 => Some(Batch(Admission(ValueTooLarge {
                bytes: 8,
                maximum: 7,
            }))),
            3 => Some(Batch(Admission(ValueTooLarge {
                bytes: 7,
                maximum: 6,
            }))),
            _ => Some(IndexRejection::DirectRunningFifoEntry),
        };
        let expected = match case {
            0 => "465741530001010104",
            1 => ATOMIC_RESPONSE,
            2 => "46574153000101010300000000000000080000000000000007",
            3 => "46574153000101010300000000000000070000000000000006",
            _ => "465741530001010700",
        };
        match reason {
            Some(reason) => assert_eq!(operation, IndexedDecision::Rejected(reason)),
            None => assert!(matches!(&operation, IndexedDecision::Accepted(batch)
                if batch.batch().items().len() == 1 && batch.consumed_ordinals() == 1)),
        }
        let status = reason.map_or(Accepted, IndexedRejected);
        let count = reason.is_none().then_some(1);
        assert_eq!(outcome.status(), status);
        assert_eq!(outcome.response(), hex(expected)?);
        assert_eq!(
            atomic_start_retained_summary(
                outcome.intent(),
                4,
                outcome.response(),
                log(1),
                TimestampMillis::new(now),
                profile.admission(),
            )?,
            (status, count)
        );
        assert_eq!(
            record_batch_retained_summary(outcome.intent(), 4, outcome.response()),
            Err(CodecError::UnsupportedVersion)
        );
        assert!(atomic_start_response_matches(
            &operation,
            outcome.response(),
            log(1),
            TimestampMillis::new(now),
            profile.admission(),
        )?);
    }
    Ok(())
}
