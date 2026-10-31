//! Actual original reservations bind monotonically across all parent phases.
use super::super::fixtures as leaf;
use super::{fixtures as a, *};
use wf_domain::{
    claim::caller::{
        ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput,
        normalize_claim_due,
    },
    governance::{
        identified::{CreditConfiguration, IdentifiedCreditMutation as M, IdentifiedCreditRequest},
        release::LimitEnforcement,
    },
};
use wf_ports::{
    log::{
        AtomicClaimOptions, GovernedClaimPayment, encode_atomic_claim_request,
        encode_credit_request, inspect_atomic_claim_reply,
    },
    state::GroupApplyStore,
};
use wf_storage::transaction::{apply_atomic_claim_transaction, apply_credit_transaction};
use wf_types::identity::{PartitionId, RecordId, RequestId};

fn fund(store: &mut RedbStateStore, ids: &[Vec<u8>]) -> TestResult {
    for (index, name, configuration, mutation) in [
        (
            5,
            b"lease".as_slice(),
            Some(CreditConfiguration {
                limit: 4,
                config_version: Some(1),
                policy_fingerprint: None,
            }),
            M::Lease {
                shard_id: 0,
                amount: 4,
                ttl_ms: 100_000,
            },
        ),
        (
            6,
            b"spend".as_slice(),
            None,
            M::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 4,
                ttl_ms: None,
                reservation_ids: ids,
            },
        ),
    ] {
        let frame = encode_credit_request(
            &f::scope()?,
            &RequestId::new(name.to_vec())?,
            f::clock(1997 + index)?,
            IdentifiedCreditRequest {
                scope: b"global",
                shard_count: 4,
                configuration,
                mutation,
            },
            v::limits().record_batch.codec,
            v::limits().record_batch.workspace_bytes,
        )?;
        assert_eq!(
            leaf::retained_record(apply_credit_transaction(
                store,
                &f::scope()?,
                frame.as_bytes(),
                f::log(index),
                v::limits().record_batch,
                wf_domain::governance::credit::CreditLimits::default()
            )?)?
            .status(),
            RetainedStatus::Accepted
        );
    }
    Ok(())
}

fn frame(ids: &[Vec<u8>], epoch: u64, now: u64, scans: usize) -> TestResult<PreparedBytes> {
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", b"new");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(4);
    input.lease_ms = Some(100);
    input.governance_limit_scope = Some(b"global");
    input.governance_shard_id = Some(0);
    let call = normalize_claim_due(input, ClaimCallerLimits::default())?;
    Ok(encode_atomic_claim_request(
        &f::scope()?,
        &RequestId::new(b"paid-parent".to_vec())?,
        f::clock(now)?,
        &call,
        AtomicClaimOptions {
            token_version: 1,
            first_ordinal: 0,
            scan_budget: scans,
            head_budget: 100,
            payment: Some(GovernedClaimPayment {
                scope: b"global",
                shard_id: 0,
                epoch,
                enforcement: LimitEnforcement::ApproximateGlobal,
                reservation_ids: ids,
            }),
        },
        v::limits().record_batch.codec,
        v::limits().record_batch.workspace_bytes,
    )?)
}

#[test]
fn one_original_paid_batch_is_used_once_across_three_phases_and_replays_before_changed_credit()
-> TestResult {
    let dir = f::directory("atomic-claim-paid-monotone")?;
    let path = dir.join("state");
    let mut store = a::filled(&path)?;
    let ids = [b"a\0".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()];
    fund(&mut store, &ids)?;
    let encoded = frame(&ids, 1, 3000, 100)?;
    let outcome = leaf::retained(apply_atomic_claim_transaction(
        &mut store,
        &f::scope()?,
        encoded.as_bytes(),
        f::log(7),
        a::limits(),
    )?)?;
    let reply = inspect_atomic_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    assert_eq!(
        (reply.status(), reply.count(), reply.phase_count()),
        (RetainedStatus::Accepted, 4, 3)
    );
    assert_eq!(reply.used_indices().collect::<Vec<_>>(), [0, 1, 2, 3]);
    assert_eq!(
        reply
            .used_reservation_ids()
            .collect::<Result<Vec<_>, _>>()?,
        ids.iter().map(Vec::as_slice).collect::<Vec<_>>()
    );
    let scope = f::scope()?;
    let tx = store.begin_apply(
        scope.group(),
        scope.recovery(),
        v::limits().record_batch.codec.admission(),
        10000,
        1024 * 1024,
        1024 * 1024,
    )?;
    for (workflow, original) in [b"r1", b"q1", b"r2", b"r3"].into_iter().zip(&ids) {
        let (epoch, binding) = tx
            .workflow_governance_binding(
                &scope,
                &PartitionId::new(b"p".to_vec())?,
                &RecordId::new(workflow.to_vec())?,
            )?
            .ok_or("original binding")?;
        assert_eq!(epoch, 1);
        assert_eq!(&binding.reservation_id, original);
    }
    drop(tx);
    drop(store);
    let mut store = f::open(&path)?;
    let changed = frame(&[b"unspent".to_vec()], 99, 3001, 100)?;
    let retried = leaf::retained(apply_atomic_claim_transaction(
        &mut store,
        &scope,
        changed.as_bytes(),
        f::log(8),
        a::limits(),
    )?)?;
    assert_eq!(retried, outcome);
    assert_eq!(leaf::record(&mut store, b"r1")?.record.version(), 3);
    drop(store);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[test]
fn late_paid_phase_refusal_rolls_back_every_binding_and_consumed_marker() -> TestResult {
    let dir = f::directory("atomic-claim-paid-rollback")?;
    let path = dir.join("state");
    let mut store = a::filled(&path)?;
    let ids = [b"a\0".to_vec(), b"b".to_vec(), b"c".to_vec(), b"d".to_vec()];
    fund(&mut store, &ids)?;
    drop(store);
    let before = leaf::cohort(&path)?;
    let mut store = f::open(&path)?;
    let encoded = frame(&ids, 1, 3000, 1)?;
    let outcome = leaf::retained(apply_atomic_claim_transaction(
        &mut store,
        &f::scope()?,
        encoded.as_bytes(),
        f::log(7),
        a::limits(),
    )?)?;
    let reply = inspect_atomic_claim_reply(
        outcome.response(),
        outcome.original_time(),
        v::limits().record_batch.codec.admission(),
    )?;
    assert!(matches!(reply.status(), RetainedStatus::ClaimRejected(_)));
    assert_eq!((reply.count(), reply.phase_count()), (0, 0));
    assert!(reply.used_reservation_ids().next().is_none());
    assert_eq!(
        reply
            .payment()
            .ok_or("original payment")?
            .reservation_ids()
            .collect::<Result<Vec<_>, _>>()?,
        ids.iter().map(Vec::as_slice).collect::<Vec<_>>()
    );
    drop(store);
    let after = leaf::cohort(&path)?;
    let mut controls = 0;
    for (key, value) in &before[1] {
        if value.starts_with(b"FWSC\0\x01") {
            controls += 1;
        } else {
            assert_eq!(
                after[1].get(key),
                Some(value),
                "unchanged workflow/credit row {key:?}"
            );
        }
    }
    assert_eq!(controls, 1);
    assert_eq!(after[1].len(), before[1].len() + 1);
    assert!(
        after[1]
            .iter()
            .find(|(key, _)| !before[1].contains_key(*key))
            .ok_or("one outcome")?
            .1
            .starts_with(b"FWSO\0\x01")
    );
    assert_eq!(after[0], before[0]);
    assert_eq!(after[2..], before[2..]);
    let mut reopened = f::open(&path)?;
    assert_eq!(leaf::record(&mut reopened, b"r1")?.record.version(), 2);
    drop(reopened);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}
