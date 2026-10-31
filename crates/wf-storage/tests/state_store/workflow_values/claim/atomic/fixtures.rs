use super::super::{fixtures as leaf, *};
use std::path::Path;
use wf_domain::claim::{
    caller::{
        ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput,
        normalize_claim_due,
    },
    coordination::ClaimCoordinationLimits,
};
use wf_domain::governance::credit::CreditLimits;
use wf_ports::{
    consensus::{PreparedClaimPhase, PreparedClaimQuery, PreparedOperation, PreparedRequest},
    log::{AtomicClaimOptions, encode_atomic_claim_request, encode_prepared},
};
use wf_storage::{
    state::{StateStoreGenesis, StateStoreLimits},
    transaction::AtomicClaimTransactionLimits,
};
use wf_types::identity::{PartitionId, RequestId};

pub(in crate::workflow_values::claim) fn limits() -> AtomicClaimTransactionLimits {
    AtomicClaimTransactionLimits {
        values: v::limits(),
        work: ClaimWorkLimits::default(),
        caller: ClaimCallerLimits::default(),
        coordination: ClaimCoordinationLimits::default(),
        credit: CreditLimits::default(),
    }
}

pub(in crate::workflow_values::claim) fn filled(path: &Path) -> TestResult<RedbStateStore> {
    let scope = f::scope()?;
    let placements = [(PartitionId::new(b"p".to_vec())?, scope.group().clone())];
    let mut store = RedbStateStore::create_governed_claim_database(
        redb::Builder::new().create(path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: f::clock(0)?,
            placements: &placements,
            policies: &[],
        },
        v::defaults(),
        4,
        StateStoreLimits::default(),
    )?;
    for (index, id) in [b"r1", b"r2", b"r3"].into_iter().enumerate() {
        let index = u64::try_from(index)?;
        leaf::create(
            &mut store,
            id,
            b"q",
            (index, 1000 + index, 0),
            Some(b"payload"),
        )?;
    }
    let frame = encode_prepared(
        &PreparedRequest {
            scope,
            request: RequestId::new(b"old".to_vec())?,
            intent: ClientIntent::new(1, [42; 32])?,
            stamp: f::clock(2000)?,
            operation: PreparedOperation::ClaimPhase(PreparedClaimPhase {
                query: PreparedClaimQuery {
                    workflow_type: b"r".to_vec(),
                    states: Some(vec![b"q".to_vec()]),
                    exclude_states: vec![],
                    partitions: Some(vec![b"p".to_vec()]),
                    priority: None,
                    worker: b"old".to_vec(),
                    lease_ms: 100,
                    limit: 3,
                },
                token_version: 1,
                first_ordinal: 0,
                scan_budget: 100,
                head_budget: 100,
            }),
        },
        v::limits().record_batch.codec,
    )?;
    assert_eq!(
        leaf::retained(leaf::apply(&mut store, &frame, 3)?)?.status(),
        RetainedStatus::Accepted
    );
    leaf::create(&mut store, b"q1", b"q", (4, 2001, 0), None)?;
    Ok(store)
}

pub(super) fn frame(scans: usize) -> TestResult<PreparedBytes> {
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", b"new");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(4);
    input.lease_ms = Some(100);
    let call = normalize_claim_due(input, ClaimCallerLimits::default())?;
    Ok(encode_atomic_claim_request(
        &f::scope()?,
        &RequestId::new(b"parent".to_vec())?,
        f::clock(3000)?,
        &call,
        AtomicClaimOptions {
            token_version: 1,
            first_ordinal: 0,
            scan_budget: scans,
            head_budget: 100,
            payment: None,
        },
        v::limits().record_batch.codec,
        v::limits().record_batch.workspace_bytes,
    )?)
}
