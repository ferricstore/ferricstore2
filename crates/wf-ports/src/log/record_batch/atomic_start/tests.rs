//! Consumer validation binds real atomic postimages to independent actual control.

use std::collections::BTreeMap;

use wf_domain::claim::ScopedRecordId;
use wf_domain::workflow::control::{ControlState, EntryStamp, PreparedDigest, plan_control};
use wf_domain::workflow::indexed::{IndexRejection, IndexedDecision};
use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_domain::workflow::{CommittedCommand, Operation, StartAndClaim};
use wf_types::identity::{GroupId, PartitionId, RecordId, RecoveryId, RequestId, TenantId};
use wf_types::version::{AppliedPosition, CommittedTime, HlcTimestamp, TimestampMillis};

use super::*;
use crate::consensus::{PreparedBatchItem, PreparedOperation, PreparedRequest};
use crate::state::{GroupApplyChanges, StateTransactionError};

fn atomic_start_changes_fixture(
    now: u64,
    payload: Option<&[u8]>,
) -> Result<(RequestScope, ControlState, RecordBatchApplyPlan<'static>), Box<dyn std::error::Error>>
{
    let scope = RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"z".to_vec())?,
    );
    let limits = CodecLimits::default();
    let initial = CommittedTime::new(HlcTimestamp::new(0, 0)?, 0)?;
    let stamp = CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?;
    let predecessor = plan_control(
        ControlState::initial(initial),
        EntryIdentity::new(
            LogIdentity::new(1, 7, AppliedPosition::new(0)),
            PreparedDigest::new([0x31; 32]),
        ),
        EntryStamp::Unstamped,
    )?
    .next();
    let frame = encode_atomic_start(
        PreparedRequest {
            scope: scope.clone(),
            request: RequestId::new(b"q".to_vec())?,
            intent: ClientIntent::new(1, [0; 32])?,
            stamp,
            operation: PreparedOperation::Batch(vec![PreparedBatchItem {
                partition: PartitionId::new(b"p".to_vec())?,
                command: CommittedCommand {
                    id: RecordId::new(b"i".to_vec())?,
                    now: TimestampMillis::new(now),
                    operation: Operation::StartAndClaim(StartAndClaim {
                        workflow_type: b"r".to_vec(),
                        state: b"q".to_vec(),
                        worker: b"w\0\xff".to_vec(),
                        lease_ms: 1,
                        payload: payload.map(<[u8]>::to_vec),
                    }),
                },
                create_priority: 0,
            }]),
        },
        limits,
        256 * 1_048_576,
    )?;
    let at = LogIdentity::new(1, 7, AppliedPosition::new(1));
    let RecordBatchInspection::Fresh(fresh) = inspect_atomic_start_point(
        prepare_atomic_start(frame.as_bytes(), limits, 256 * 1_048_576)?,
        at,
        PointGateView {
            scope: &scope,
            control: predecessor,
            retained_nonempty: false,
            outcome: None,
        },
        limits,
        256 * 1_048_576,
    )?
    else {
        return Err("fixture did not produce actual Fresh authority".into());
    };
    let partition = PartitionId::new(b"p".to_vec())?;
    let placements = BTreeMap::from([(partition, scope.group().clone())]);
    let current = BTreeMap::from([(
        ScopedRecordId {
            partition_key: b"p",
            id: b"i",
        },
        None,
    )]);
    let parallel = BTreeMap::new();
    let fifo_members = BTreeMap::new();
    let fifo_heads = BTreeMap::new();
    let policies = BTreeMap::from([(b"r".as_slice(), None)]);
    let retry_policies = BTreeMap::new();
    let plan = decide_fresh_atomic_start_touched(
        fresh,
        TouchedIndexedGroupView {
            group: scope.group(),
            placements: &placements,
            current: &current,
            parallel: &parallel,
            fifo_members: &fifo_members,
            fifo_heads: &fifo_heads,
            policies: &policies,
            retry_policies: &retry_policies,
        },
        0,
        1_000,
    )?;
    Ok((scope, predecessor, plan))
}

#[test]
fn atomic_start_changes_reject_mismatched_retained_postimage()
-> Result<(), Box<dyn std::error::Error>> {
    let (scope, expected, plan) = atomic_start_changes_fixture(17, None)?;
    let baseline = GroupApplyChanges::new(&scope, expected, plan);
    assert!(baseline.is_ok(), "{:?}", baseline.as_ref().err());
    for case in 0..6 {
        let (scope, expected, plan) = atomic_start_changes_fixture(17, Some(b"v"))?;
        let RecordBatchApplyPlan {
            control,
            decision:
                RecordBatchApplyDecision::Fresh {
                    mut operation,
                    mut outcome,
                },
        } = plan
        else {
            return Err("actual accepted fixture lost its plan".into());
        };
        if case == 2 || case == 4 {
            let (_, _, donor) = atomic_start_changes_fixture(
                if case == 2 { 18 } else { 17 },
                Some(if case == 4 { b"x" } else { b"v" }),
            )?;
            let RecordBatchApplyDecision::Fresh {
                operation: changed,
                outcome: response,
            } = donor.decision
            else {
                return Err("actual donor was not Fresh".into());
            };
            if case == 2 {
                operation = changed;
            }
            outcome = response;
        } else if case == 5 {
            operation = IndexedDecision::Rejected(IndexRejection::DirectRunningFifoEntry);
        } else {
            let original = outcome.original_position();
            let wrong = match case {
                0 => LogIdentity::new(2, original.leader_node(), original.index()),
                1 => LogIdentity::new(original.term(), 8, original.index()),
                _ => original,
            };
            outcome = Box::new(RetainedOutcome::new(
                RetainedMetadata {
                    scope: outcome.scope().clone(),
                    request: outcome.request().clone(),
                    intent: outcome.intent(),
                    original_position: wrong,
                    original_time: outcome.original_time(),
                    expires_at: outcome.expires_at(),
                    status: if case == 3 {
                        RetainedStatus::IndexedRejected(IndexRejection::DirectRunningFifoEntry)
                    } else {
                        outcome.status()
                    },
                },
                outcome.response_version(),
                outcome.response().to_vec(),
                CodecLimits::default().admission(),
            )?);
        }
        let changed = RecordBatchApplyPlan {
            control,
            decision: RecordBatchApplyDecision::Fresh { operation, outcome },
        };
        assert!(
            matches!(
                GroupApplyChanges::new(&scope, expected, changed),
                Err(StateTransactionError::InvalidChanges)
            ),
            "{case}"
        );
    }
    Ok(())
}
