//! Prepared original timeline; terminal guards come from actual accepted Claim.
use super::fixtures as p;
use super::*;
use wf_domain::{
    governance::{
        identified::{CreditConfiguration, IdentifiedCreditMutation as M, IdentifiedCreditRequest},
        release::{LimitEnforcement, ReleaseMaintenanceRequest as C},
    },
    workflow::{Cancel, CommittedCommand, Operation},
};
use wf_ports::{
    consensus::{
        PreparedBatchItem, PreparedClaimPhase, PreparedClaimQuery, PreparedOperation,
        PreparedWorkflowValueBatch, WorkflowValueItemInput, WorkflowValuePolicyWitness,
        WorkflowValueTargetGuard,
    },
    log::{
        GovernedClaimPayment, PreparedBytes, encode_credit_request, encode_governed_claim_request,
        encode_release_maintenance_request, encode_workflow_stored_value_batch,
    },
};
use wf_types::{
    identity::{PartitionId, RecordId, RequestId},
    version::TimestampMillis,
};

pub(super) fn stored_frame(id: &[u8], now: u64, operation: Operation) -> TestResult<PreparedBytes> {
    stored_batch(id, id, now, operation, 5)
}
fn stored_batch(
    request: &[u8],
    id: &[u8],
    now: u64,
    operation: Operation,
    profile: u8,
) -> TestResult<PreparedBytes> {
    let policy = WorkflowValuePolicyWitness {
        workflow_type: b"r".to_vec(),
        profile: 0,
        generation: 0,
        digest: wf_domain::workflow::control::PreparedDigest::new(
            f::hex("6e9978636a6017b6e2abf81647a5aab1b71f003b16e23ad931a1beda2d62b9a9")?
                .try_into()
                .map_err(|_| "digest")?,
        ),
    };
    let batch = PreparedWorkflowValueBatch {
        scope: f::scope()?,
        request: RequestId::new(request.to_vec())?,
        stamp: f::clock(now, 0, now)?,
        profile,
        policy_references_captured: Some(true),
        defaults: p::defaults(),
        policies: vec![policy],
        operation: PreparedOperation::Batch(vec![PreparedBatchItem {
            partition: PartitionId::new(b"p".to_vec())?,
            create_priority: 0,
            command: CommittedCommand {
                id: RecordId::new(id.to_vec())?,
                now: TimestampMillis::new(now),
                operation,
            },
        }]),
        inputs: vec![WorkflowValueItemInput {
            source_index: 0,
            policy_index: 0,
            target: WorkflowValueTargetGuard::Unspecified,
            birth: None,
            terminal_ttl_ms: None,
            requested_at: None,
        }],
    };
    Ok(encode_workflow_stored_value_batch(
        &batch,
        f::driver_limits().codec,
    )?)
}
fn credit(
    id: &[u8],
    now: u64,
    configuration: Option<CreditConfiguration>,
    mutation: M<'_>,
) -> TestResult<PreparedBytes> {
    Ok(encode_credit_request(
        &f::scope()?,
        &RequestId::new(id.to_vec())?,
        f::clock(now, 0, now)?,
        IdentifiedCreditRequest {
            scope: b"global",
            shard_count: 4,
            configuration,
            mutation,
        },
        f::driver_limits().codec,
        16 * f::MIB,
    )?)
}
pub(super) fn paid() -> TestResult<PreparedBytes> {
    Ok(encode_governed_claim_request(
        &f::scope()?,
        &RequestId::new(b"claim".to_vec())?,
        f::clock(104, 0, 104)?,
        &PreparedClaimPhase {
            query: PreparedClaimQuery {
                workflow_type: b"r".to_vec(),
                states: Some(vec![b"q".to_vec()]),
                exclude_states: vec![],
                partitions: Some(vec![b"p".to_vec()]),
                priority: None,
                worker: b"w".to_vec(),
                lease_ms: 30000,
                limit: 2,
            },
            token_version: 1,
            first_ordinal: 0,
            scan_budget: 1000,
            head_budget: 1000,
        },
        GovernedClaimPayment {
            scope: b"global",
            shard_id: 0,
            epoch: 1,
            enforcement: LimitEnforcement::StrictGlobal,
            reservation_ids: &[b"a\0".to_vec(), b"b".to_vec()],
        },
        f::driver_limits().codec,
        128 * f::MIB,
    )?)
}
pub(super) fn maintenance(id: &[u8], now: u64, command: &C) -> TestResult<PreparedBytes> {
    Ok(encode_release_maintenance_request(
        &f::scope()?,
        &RequestId::new(id.to_vec())?,
        f::clock(now, 0, now)?,
        command,
        f::driver_limits().codec,
        16 * f::MIB,
    )?)
}
pub(super) fn all() -> TestResult<Vec<PreparedBytes>> {
    let mut frames = vec![
        stored_frame(b"i", 100, f::create_op())?,
        stored_frame(b"j", 101, f::create_op())?,
        credit(
            b"lease",
            102,
            Some(CreditConfiguration {
                limit: 3,
                config_version: Some(1),
                policy_fingerprint: None,
            }),
            M::Lease {
                shard_id: 0,
                amount: 3,
                ttl_ms: 1000,
            },
        )?,
        credit(
            b"spend",
            103,
            None,
            M::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 2,
                ttl_ms: None,
                reservation_ids: &[b"a\0".to_vec(), b"b".to_vec()],
            },
        )?,
        paid()?,
    ];
    let dir = f::directory("release-node-reference-guards")?;
    let (log, mut state) = p::pair(&dir, true)?;
    p::apply(&mut state, &frames, frames.len())?;
    let record = p::record(&mut state, b"i")?.record;
    frames.push(terminal(&record)?);
    drop((log, state));
    std::fs::remove_dir_all(dir)?;
    frames.push(credit(
        b"release",
        106,
        None,
        M::Release {
            shard_id: 0,
            amount: 1,
            reservation_ids: &[b"a\0".to_vec()],
        },
    )?);
    frames.push(maintenance(
        b"mark",
        107,
        &C::MarkCompleted { sequences: vec![1] },
    )?);
    frames.push(maintenance(
        b"ack",
        108,
        &C::Acknowledge {
            expected_head: 1,
            up_to: 1,
        },
    )?);
    Ok(frames)
}
fn terminal(record: &wf_domain::workflow::WorkflowRecord) -> TestResult<PreparedBytes> {
    stored_batch(
        b"cancel",
        b"i",
        105,
        Operation::Cancel(Cancel {
            fence: record.fence(),
            lease_token: record.lease().map(|lease| lease.token.clone()),
            expected_state: Some(b"running".to_vec()),
            expected_version: Some(record.version()),
            error: None,
        }),
        5,
    )
}

pub(super) fn combined() -> TestResult<PreparedBytes> {
    stored_batch(
        b"unpaid",
        b"i",
        100,
        Operation::StartAndClaim(wf_domain::workflow::StartAndClaim {
            workflow_type: b"r".to_vec(),
            state: b"q".to_vec(),
            worker: b"w".to_vec(),
            lease_ms: 1000,
            payload: None,
        }),
        7,
    )
}
