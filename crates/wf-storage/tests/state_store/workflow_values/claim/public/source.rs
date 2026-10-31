//! Native source-postimage controls with actual created predecessors.
use super::super::fixtures as leaf;
use super::{fixtures as p, *};
use std::path::Path;
use wf_domain::{
    claim::caller::{ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput},
    policy::complete::RawFlowPolicyValue as Raw,
    retry::source_binary_hash,
};
use wf_ports::{
    log::{inspect_public_claim_reply, running_claim_scope},
    state::GroupApplyStore,
};
use wf_storage::{
    state::{StateStoreGenesis, StateStoreLimits},
    transaction::apply_public_claim_transaction,
};
use wf_types::identity::{PartitionId, RecordId};

fn fresh(path: &Path, state: &[u8], governed: bool) -> TestResult<RedbStateStore> {
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
    leaf::create(&mut store, b"first", state, (0, 1000, 0), None)?;
    leaf::create(&mut store, b"second", b"q", (1, 1001, 1), None)?;
    if governed {
        policy(&mut store, 2)?;
    }
    Ok(store)
}

fn policy(store: &mut RedbStateStore, index: u64) -> TestResult {
    let rule = [
        (Raw::Atom("limit"), Raw::Integer(8)),
        (Raw::Atom("lease_size"), Raw::Integer(6)),
    ];
    let running = [(Raw::Binary(b"running"), Raw::Map(&rule))];
    let governance = [(Raw::Atom("limits"), Raw::Map(&running))];
    let attrs = [
        (Raw::Atom("version"), Raw::Integer(17)),
        (Raw::Atom("governance"), Raw::Map(&governance)),
    ];
    leaf::policy(store, index, 2500, Raw::Map(&attrs))
}

fn frame(worker: &[u8]) -> TestResult<PreparedBytes> {
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", worker);
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(4);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(false);
    p::encode(input, p::BATCH, 100)
}

fn assert_binding(store: &mut RedbStateStore, id: &[u8], expected: Option<&[u8]>) -> TestResult {
    let scope = f::scope()?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        v::limits().record_batch.codec.admission(),
        10000,
        1_048_576,
        1_048_576,
    )?;
    let binding = tx.workflow_governance_binding(
        &scope,
        &PartitionId::new(b"p".to_vec())?,
        &RecordId::new(id.to_vec())?,
    )?;
    assert_eq!(
        binding.as_ref().map(|(_, b)| b.reservation_id.as_slice()),
        expected
    );
    Ok(())
}

fn assert_reservations(path: &Path, ids: &[&[u8]], used: &[usize]) -> TestResult {
    let cohort = leaf::cohort(path)?;
    let rows = &cohort[1];
    assert_eq!(
        rows.keys().filter(|key| key.get(6) == Some(&13)).count(),
        ids.len()
    );
    assert_eq!(
        rows.keys().filter(|key| key.get(6) == Some(&24)).count(),
        used.len()
    );
    for (index, id) in ids.iter().enumerate() {
        let mut expected = b"FWGI\0\x01".to_vec();
        expected.extend_from_slice(&u32::try_from(id.len())?.to_be_bytes());
        expected.extend_from_slice(id);
        let matching = rows
            .iter()
            .filter(|(key, value)| key.get(6) == Some(&13) && value.starts_with(&expected))
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1);
        let (key, actual) = matching[0];
        let retained = used.contains(&index);
        expected.extend_from_slice(&1_u16.to_be_bytes());
        expected.push(u8::from(!retained));
        assert_eq!(actual, &expected);
        let mut marker = key.clone();
        marker[6] = 24;
        assert_eq!(
            rows.get(&marker).map(Vec::as_slice),
            retained.then_some(b"FWGU\0\x01".as_slice())
        );
    }
    Ok(())
}

#[test]
fn large_worker_below_the_known_source_limits_remains_claimable() -> TestResult {
    let dir = f::directory("claim-source-large-valid-worker")?;
    let path = dir.join("state");
    let mut store = fresh(&path, b"q", false)?;
    let worker = vec![b'w'; 32500];
    let frame = frame(&worker)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(2),
        p::limits()?,
    )?)?;
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    assert_eq!(reply.claim().ok_or("claim")?.count(), 2);
    for id in [b"first".as_slice(), b"second".as_slice()] {
        assert_eq!(
            leaf::record(&mut store, id)?
                .record
                .lease()
                .ok_or("lease")?
                .worker,
            worker
        );
    }
    drop(store);
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn admitted_large_worker_is_a_late_source_skip_with_no_governance() -> TestResult {
    for length in [32769, 32768, 32600] {
        let dir = f::directory(&format!("claim-source-worker-{length}"))?;
        let path = dir.join("state");
        let mut store = fresh(&path, b"q", false)?;
        let before = [
            leaf::record(&mut store, b"first")?,
            leaf::record(&mut store, b"second")?,
        ];
        let worker = vec![b'w'; length];
        let frame = frame(&worker)?;
        let outcome = leaf::retained(apply_public_claim_transaction(
            &mut store,
            &f::scope()?,
            frame.as_bytes(),
            f::log(2),
            p::limits()?,
        )?)?;
        let reply = inspect_public_claim_reply(
            outcome.response(),
            outcome.original_time(),
            v::limits().record_batch.codec.admission(),
        )?;
        let claim = reply.claim().ok_or("claim")?;
        assert_eq!(claim.count(), 0, "worker length {length}");
        assert_eq!(claim.work_counts().0, 2);
        assert!(claim.payment().is_none());
        assert_eq!(
            [
                leaf::record(&mut store, b"first")?,
                leaf::record(&mut store, b"second")?
            ],
            before
        );
        drop(store);
        drop(f::open(&path)?);
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[test]
fn known_source_postimage_failure_refunds_all_original_ids_without_no_demand() -> TestResult {
    let dir = f::directory("claim-source-worker-refund")?;
    let path = dir.join("state");
    let mut store = fresh(&path, b"q", true)?;
    let worker = vec![b'w'; 32768];
    let frame = frame(&worker)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(3),
        p::limits()?,
    )?)?;
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    let claim = reply.claim().ok_or("actual claim, not NoDemand")?;
    assert_eq!(claim.count(), 0);
    assert_eq!(claim.payment().ok_or("original paid batch")?.count(), 4);
    assert!(claim.used_indices().next().is_none());
    let ids = claim
        .payment()
        .ok_or("payment")?
        .reservation_ids()
        .collect::<Result<Vec<_>, _>>()?;
    let owner = store
        .read_credit_owner(
            &f::scope()?,
            &running_claim_scope(b"r"),
            p::limits()?.base.credit,
            10000,
            16 * 1_048_576,
        )?
        .ok_or("owner")?;
    let lease = owner
        .lease(u64::from(source_binary_hash(&worker)) % 4)
        .ok_or("lease")?;
    assert_eq!((lease.available, lease.in_use), (6, 0));
    assert_binding(&mut store, b"first", None)?;
    assert_binding(&mut store, b"second", None)?;
    drop(store);
    assert_reservations(&path, &ids, &[])?;
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn combined_generated_metadata_skips_first_and_binds_original_one_to_valid_sibling() -> TestResult {
    let dir = f::directory("claim-source-aggregate-nonprefix")?;
    let path = dir.join("state");
    let state = vec![b's'; 32000];
    let mut store = fresh(&path, &state, true)?;
    let before = leaf::record(&mut store, b"first")?;
    let worker = vec![b'w'; 1024];
    let frame = frame(&worker)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(3),
        p::limits()?,
    )?)?;
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    let claim = reply.claim().ok_or("claim")?;
    assert_eq!(claim.count(), 1);
    assert_eq!(claim.used_indices().collect::<Vec<_>>(), [1]);
    let ids = claim
        .payment()
        .ok_or("payment")?
        .reservation_ids()
        .collect::<Result<Vec<_>, _>>()?;
    assert_binding(&mut store, b"first", None)?;
    assert_binding(&mut store, b"second", Some(ids[1]))?;
    assert_eq!(leaf::record(&mut store, b"first")?, before);
    assert_eq!(leaf::record(&mut store, b"second")?.record.version(), 2);
    let owner = store
        .read_credit_owner(
            &f::scope()?,
            &running_claim_scope(b"r"),
            p::limits()?.base.credit,
            10000,
            16 * 1_048_576,
        )?
        .ok_or("owner")?;
    let lease = owner
        .lease(u64::from(source_binary_hash(&worker)) % 4)
        .ok_or("lease")?;
    assert_eq!((lease.available, lease.in_use), (5, 1));
    drop(store);
    assert_reservations(&path, &ids, &[1])?;
    let mut reopened = f::open(&path)?;
    assert_eq!(
        leaf::retained(apply_public_claim_transaction(
            &mut reopened,
            &f::scope()?,
            frame.as_bytes(),
            f::log(4),
            p::limits()?,
        )?)?,
        outcome
    );
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn a_filtered_out_large_state_consumes_no_original_reservation() -> TestResult {
    let dir = f::directory("claim-source-filter-before-binding")?;
    let path = dir.join("state");
    let mut store = fresh(&path, &vec![b's'; 32000], true)?;
    let before = leaf::record(&mut store, b"first")?;
    let worker = vec![b'w'; 1024];
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", &worker);
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(4);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(false);
    let frame = p::encode(input, p::BATCH, 100)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(3),
        p::limits()?,
    )?)?;
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    let claim = reply.claim().ok_or("claim")?;
    assert_eq!(claim.count(), 1);
    assert_eq!(claim.used_indices().collect::<Vec<_>>(), [0]);
    let ids = claim
        .payment()
        .ok_or("payment")?
        .reservation_ids()
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(leaf::record(&mut store, b"first")?, before);
    assert_binding(&mut store, b"first", None)?;
    assert_binding(&mut store, b"second", Some(ids[0]))?;
    drop(store);
    assert_reservations(&path, &ids, &[0])?;
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn reclaim_validates_the_preserved_logical_run_state_before_reusing_any_id() -> TestResult {
    use wf_ports::consensus::{
        PreparedClaimPhase, PreparedClaimQuery, PreparedOperation, PreparedRequest,
    };
    let dir = f::directory("claim-source-reclaim-logical-state")?;
    let path = dir.join("state");
    let state = vec![b's'; 32000];
    let mut store = fresh(&path, &state, false)?;
    let old = wf_ports::log::encode_prepared(
        &PreparedRequest {
            scope: f::scope()?,
            request: wf_types::identity::RequestId::new(b"old-source".to_vec())?,
            intent: ClientIntent::new(1, [42; 32])?,
            stamp: f::clock(2000)?,
            operation: PreparedOperation::ClaimPhase(PreparedClaimPhase {
                query: PreparedClaimQuery {
                    workflow_type: b"r".to_vec(),
                    states: Some(vec![state]),
                    exclude_states: vec![],
                    partitions: Some(vec![b"p".to_vec()]),
                    priority: None,
                    worker: b"old".to_vec(),
                    lease_ms: 100,
                    limit: 1,
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
        leaf::retained(leaf::apply(&mut store, &old, 2)?)?.status(),
        RetainedStatus::Accepted
    );
    let before = leaf::record(&mut store, b"first")?;
    assert_eq!(before.record.state(), b"running");
    assert_eq!(
        before.record.run_state().ok_or("logical state")?.len(),
        32000
    );
    policy(&mut store, 3)?;
    let worker = vec![b'w'; 1024];
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", &worker);
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(4);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(true);
    let frame = p::encode(input, p::BATCH, 100)?;
    let outcome = leaf::retained(apply_public_claim_transaction(
        &mut store,
        &f::scope()?,
        frame.as_bytes(),
        f::log(4),
        p::limits()?,
    )?)?;
    let reply = inspect_public_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    let claim = reply.claim().ok_or("claim")?;
    assert_eq!((claim.count(), claim.phase_count()), (1, 3));
    assert_eq!(claim.used_indices().collect::<Vec<_>>(), [1]);
    let ids = claim
        .payment()
        .ok_or("payment")?
        .reservation_ids()
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(leaf::record(&mut store, b"first")?, before);
    assert_binding(&mut store, b"first", None)?;
    assert_binding(&mut store, b"second", Some(ids[1]))?;
    drop(store);
    assert_reservations(&path, &ids, &[1])?;
    drop(f::open(&path)?);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
