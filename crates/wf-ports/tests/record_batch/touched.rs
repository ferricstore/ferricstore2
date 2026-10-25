//! Bounded Fresh inputs and additive caller-residency admission.

use std::collections::BTreeMap;
use std::mem::size_of;

use wf_domain::claim::{Claim, ClaimRecord, DueOrder, LaneKey, ScopedRecordId};
use wf_domain::history::EventKind;
use wf_domain::lease::{LeaseGuard, LeaseToken};
use wf_domain::policy::{InstalledRetryPolicy, RetryOverride};
use wf_domain::retry::{JitterSample, PolicyRetry};
use wf_domain::workflow::batch::ApplyError;
use wf_domain::workflow::control::{
    ControlState, EntryIdentity, EntryStamp, PreparedDigest, plan_control,
};
use wf_domain::workflow::indexed::{IndexedDecision, TouchedIndexedGroupView};
use wf_domain::workflow::request::gate::PointGateView;
use wf_domain::workflow::{
    CommittedCommand, Operation, WorkflowRecord, reduce, reduce_with_policy,
};
use wf_ports::blob::{RecordBatchValueEffect, RecordBatchValueEffects};
use wf_ports::log::{
    CodecError, FreshRecordBatch, RecordBatchApplyDecision, RecordBatchApplyError,
    RecordBatchApplyPlan, RecordBatchInspection, decide_fresh_record_batch_touched,
    encode_record_batch, inspect_record_batch_point, prepare_record_batch,
};
use wf_types::identity::{GroupId, PartitionId};
use wf_types::version::TimestampMillis;

use super::{TestResult, WORKSPACE, clock, create, hex, limits, log, request, scope, with_empty};

const VALUE_FRAME: &str = "4657435000010100000001740000000167000000017a00000001710005ad6eeaaf282d9eff4643ae161d860eba1a40f3c35f472482292fd2ff05bd812e0000000000000001000000000000000000010000000100000001700000000169000000000000000001010000000172000000017100010000000176";
const FRESH_WITH_L: usize = 33_558_449;

fn inspected(
    encoded: &[u8],
    workspace: usize,
    control: ControlState,
    index: u64,
) -> TestResult<FreshRecordBatch<'_>> {
    let query = scope()?;
    let RecordBatchInspection::Fresh(fresh) = inspect_record_batch_point(
        prepare_record_batch(encoded, limits(), workspace)?,
        log(index),
        PointGateView {
            scope: &query,
            control,
            retained_nonempty: false,
            outcome: None,
        },
        limits(),
        workspace,
    )?
    else {
        return Err("not fresh".into());
    };
    drop(query);
    Ok(fresh)
}

fn empty_decision(
    fresh: FreshRecordBatch<'_>,
    live: usize,
    known: bool,
) -> Result<RecordBatchApplyPlan<'static>, RecordBatchApplyError> {
    let group = GroupId::new(b"g".to_vec()).map_err(|_| ApplyError::InvalidAuthority)?;
    let partition = PartitionId::new(b"p".to_vec()).map_err(|_| ApplyError::InvalidAuthority)?;
    let placements = BTreeMap::from([(partition, group.clone())]);
    let current = if known {
        BTreeMap::from([(
            ScopedRecordId {
                partition_key: b"p".as_slice(),
                id: b"i".as_slice(),
            },
            None,
        )])
    } else {
        BTreeMap::new()
    };
    let parallel = BTreeMap::new();
    let members = BTreeMap::new();
    let heads = BTreeMap::new();
    let modes = BTreeMap::from([(b"r".as_slice(), None)]);
    let retries = BTreeMap::new();
    decide_fresh_record_batch_touched(
        fresh,
        TouchedIndexedGroupView {
            group: &group,
            placements: &placements,
            current: &current,
            parallel: &parallel,
            fifo_members: &members,
            fifo_heads: &heads,
            policies: &modes,
            retry_policies: &retries,
        },
        live,
        100,
    )
}

#[test]
fn touched_fresh_matches_complete_output() -> TestResult {
    let encoded = hex(VALUE_FRAME)?;
    assert_eq!(encoded.len(), 120);
    let owner_bytes =
        size_of::<Option<RecordBatchValueEffects>>() + size_of::<RecordBatchValueEffect>();
    let owned = FRESH_WITH_L + owner_bytes;
    let fresh = inspected(&encoded, owned - 1, ControlState::initial(clock(0)?), 0)?;
    assert_eq!(
        empty_decision(fresh, 17, true).err(),
        Some(RecordBatchApplyError::Workspace {
            attempted: owned,
            maximum: owned - 1,
        })
    );
    let fresh = inspected(&encoded, owned, ControlState::initial(clock(0)?), 0)?;
    assert_eq!(
        fresh.decoded().value_bytes(0).ok_or("values absent")?[0],
        Some(b"v".as_slice())
    );
    // L17 exercises numeric composition; it does not measure these fixture maps.
    let plan = empty_decision(fresh, 17, true)?;
    with_empty(&encoded, limits(), WORKSPACE, |complete| {
        assert_eq!(plan, complete?);
        Ok(())
    })?;
    drop(encoded);
    let RecordBatchApplyDecision::Fresh {
        operation: IndexedDecision::Accepted(batch),
        outcome,
    } = plan.decision()
    else {
        return Err("create rejected".into());
    };
    let item = &batch.batch().items()[0];
    let record = &item.plan().write().ok_or("write absent")?.record;
    assert_eq!(record.payload(), Some(b"v".as_slice()));
    assert_eq!(record.version(), 1);
    assert_eq!(
        item.plan().history().ok_or("history absent")?.kind,
        EventKind::Created
    );
    assert_eq!(
        batch
            .metadata()
            .next()
            .ok_or("metadata absent")?
            .state_enter_seq,
        0
    );
    assert_eq!(outcome.response(), [0]);
    assert_eq!(outcome.original_position(), log(0));
    assert_eq!(outcome.original_time(), TimestampMillis::new(1));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(101));
    Ok(())
}

#[test]
fn touched_fresh_requires_retry_observations_and_live_admission() -> TestResult {
    let encoded = hex(VALUE_FRAME)?;
    let fresh = inspected(
        &encoded,
        FRESH_WITH_L - 1,
        ControlState::initial(clock(0)?),
        0,
    )?;
    assert_eq!(
        empty_decision(fresh, 17, false).err(),
        Some(RecordBatchApplyError::Workspace {
            attempted: FRESH_WITH_L,
            maximum: FRESH_WITH_L - 1
        })
    );
    let fresh = inspected(&encoded, usize::MAX, ControlState::initial(clock(0)?), 0)?;
    assert_eq!(
        empty_decision(fresh, usize::MAX, false).err(),
        Some(RecordBatchApplyError::Encoding(CodecError::LengthOverflow))
    );
    assert_retry_observations()
}

fn retry_decision<'current>(
    fresh: FreshRecordBatch<'_>,
    record: &'current WorkflowRecord,
    observed: bool,
    policy: Option<&InstalledRetryPolicy>,
) -> Result<RecordBatchApplyPlan<'current>, RecordBatchApplyError> {
    let group = GroupId::new(b"g".to_vec()).map_err(|_| ApplyError::InvalidAuthority)?;
    let partition = PartitionId::new(b"p".to_vec()).map_err(|_| ApplyError::InvalidAuthority)?;
    let placements = BTreeMap::from([(partition, group.clone())]);
    let row = ClaimRecord {
        record,
        partition_key: b"p",
        priority: 0,
        state_enter_seq: 100,
    };
    let current = BTreeMap::from([(ScopedRecordId::for_record(&row), Some(row))]);
    let parallel = BTreeMap::from([(
        DueOrder::for_record(&row).ok_or(ApplyError::InvalidAuthority)?,
        true,
    )]);
    let members = BTreeMap::new();
    let heads = BTreeMap::from([(
        LaneKey::for_record(&row).ok_or(ApplyError::InvalidAuthority)?,
        None,
    )]);
    let modes = BTreeMap::from([(b"r".as_slice(), None)]);
    let retries = if observed {
        BTreeMap::from([(b"r".as_slice(), policy)])
    } else {
        BTreeMap::new()
    };
    decide_fresh_record_batch_touched(
        fresh,
        TouchedIndexedGroupView {
            group: &group,
            placements: &placements,
            current: &current,
            parallel: &parallel,
            fifo_members: &members,
            fifo_heads: &heads,
            policies: &modes,
            retry_policies: &retries,
        },
        17,
        100,
    )
}

fn retry_fresh(encoded: &[u8]) -> TestResult<FreshRecordBatch<'_>> {
    let mut control = ControlState::initial(clock(0)?);
    for index in 0..3 {
        control = plan_control(
            control,
            EntryIdentity::new(log(index), PreparedDigest::new([0; 32])),
            EntryStamp::Stamped(clock(1)?),
        )?
        .next();
    }
    inspected(encoded, WORKSPACE, control, 3)
}

fn assert_retry_observations() -> TestResult {
    let mut item = create(b"i")?;
    let created = reduce(None, &item.command)?
        .into_parts()
        .1
        .ok_or("create absent")?
        .record;
    let claim = CommittedCommand {
        id: item.command.id.clone(),
        now: TimestampMillis::new(1),
        operation: Operation::Claim(Claim {
            expected_state: b"q".to_vec(),
            worker: b"worker".to_vec(),
            token: LeaseToken::new(b"tok".to_vec())?,
            lease_ms: 10,
        }),
    };
    let record = reduce(Some(&created), &claim)?
        .into_parts()
        .1
        .ok_or("claim absent")?
        .record;
    item.command.operation = Operation::RetryWithPolicy(PolicyRetry {
        guard: LeaseGuard {
            token: LeaseToken::new(b"tok".to_vec())?,
            fence: record.fence(),
        },
        run_at: None,
        policy_override: RetryOverride::default(),
        jitter: JitterSample::new(0),
        payload: None,
        error: None,
    });
    let installed = InstalledRetryPolicy::new(
        b"r".to_vec(),
        RetryOverride {
            max_retries: Some(0),
            exhausted_to: Some(b"rescue".to_vec()),
            ..RetryOverride::default()
        },
        BTreeMap::new(),
    )?;
    let expected = reduce_with_policy(Some(&record), &item.command, Some(&installed))?;
    let encoded = encode_record_batch(request(vec![item])?, limits(), WORKSPACE)?;
    assert_eq!(
        retry_decision(retry_fresh(encoded.as_bytes())?, &record, false, None).err(),
        Some(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority))
    );
    let wrong = InstalledRetryPolicy::new(
        b"foreign".to_vec(),
        RetryOverride::default(),
        BTreeMap::new(),
    )?;
    assert_eq!(
        retry_decision(
            retry_fresh(encoded.as_bytes())?,
            &record,
            true,
            Some(&wrong)
        )
        .err(),
        Some(RecordBatchApplyError::Apply(ApplyError::InvalidAuthority))
    );
    let default = retry_decision(retry_fresh(encoded.as_bytes())?, &record, true, None)?;
    assert_retry_result(&default, b"q", 801, 100)?;
    let rescued = retry_decision(
        retry_fresh(encoded.as_bytes())?,
        &record,
        true,
        Some(&installed),
    )?;
    assert_retry_result(&rescued, b"rescue", 1, 12_884_901_888)?;
    let RecordBatchApplyDecision::Fresh {
        operation: IndexedDecision::Accepted(batch),
        ..
    } = rescued.decision()
    else {
        return Err("retry rejected".into());
    };
    assert_eq!(batch.batch().items()[0].plan(), &expected);
    Ok(())
}

fn assert_retry_result(
    plan: &RecordBatchApplyPlan<'_>,
    state: &[u8],
    due: u64,
    sequence: u128,
) -> TestResult {
    let RecordBatchApplyDecision::Fresh {
        operation: IndexedDecision::Accepted(batch),
        outcome,
    } = plan.decision()
    else {
        return Err("retry rejected".into());
    };
    let item = &batch.batch().items()[0];
    let record = &item.plan().write().ok_or("write absent")?.record;
    assert_eq!(record.state(), state);
    assert_eq!(record.next_run_at(), Some(TimestampMillis::new(due)));
    assert_eq!(record.attempts(), 1);
    assert_eq!(record.version(), 3);
    assert_eq!(
        item.plan().history().ok_or("history absent")?.kind,
        EventKind::Retry
    );
    assert_eq!(
        batch
            .metadata()
            .next()
            .ok_or("metadata absent")?
            .state_enter_seq,
        sequence
    );
    assert_eq!(outcome.response(), [0]);
    assert_eq!(outcome.original_position(), log(3));
    assert_eq!(outcome.original_time(), TimestampMillis::new(1));
    assert_eq!(outcome.expires_at(), TimestampMillis::new(101));
    Ok(())
}
