//! Real public-credit frames, durable catalog-capable stores and original receipts.
use super::*;
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
use wf_domain::{
    governance::caller::{CreditCallKind as K, CreditOptionValue as V, normalize_credit_call},
    workflow::{active::LifecycleDefaults, control::PreparedDigest},
};
use wf_ports::{
    consensus::WorkflowValueDefaults,
    log::{CreditSpendPreparation, PreparedBytes, RaftLogLimits, encode_public_credit_request},
};
use wf_storage::{
    state::{StateStoreGenesis, StateStoreLimits},
    transaction::{RecordBatchTransactionOutcome as O, apply_public_credit_transaction},
};
use wf_types::identity::RequestId;
pub(super) fn selected() -> TestResult<CommittedRecoveryLimits> {
    Ok(caps()?
        .with_governance_catalog_pruning()
        .with_public_credit_limits(4, CreditLimits::default())?)
}
pub(super) fn defaults() -> WorkflowValueDefaults {
    let lifecycle = LifecycleDefaults {
        ttl_ms: 7000,
        history_hot_max_events: 2,
        history_max_events: 10,
        max_history_hot_max_events: 100,
        max_history_max_events: 1000,
    };
    let mut hash = Sha256::new();
    hash.update(b"wf-workflow-value-defaults-v1");
    hash.update(lifecycle.ttl_ms.to_be_bytes());
    hash.update(lifecycle.history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.history_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_hot_max_events.to_be_bytes());
    hash.update(lifecycle.max_history_max_events.to_be_bytes());
    WorkflowValueDefaults {
        configuration_digest: PreparedDigest::new(hash.finalize().into()),
        lifecycle,
    }
}
pub(super) fn log(dir: &Path) -> TestResult<RedbLogStore> {
    let scope = f::scope()?;
    Ok(RedbLogStore::create(
        &dir.join("log.redb"),
        scope.group().clone(),
        scope.recovery().clone(),
        RaftLogLimits::default(),
    )?)
}
pub(super) fn pair(dir: &Path) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let scope = f::scope()?;
    Ok((
        log(dir)?,
        RedbStateStore::create_governance_prune_database(
            redb::Builder::new().create(dir.join("state.redb"))?,
            scope.group().clone(),
            scope.recovery().clone(),
            StateStoreGenesis {
                initial_time: f::clock(0, 0, 0)?,
                placements: &[],
                policies: &[],
            },
            defaults(),
            Some(4),
            StateStoreLimits::default(),
        )?,
    ))
}
fn frame(
    id: &[u8],
    kind: K,
    now: u64,
    opts: &[(&str, V<'_>)],
    spend: Option<CreditSpendPreparation>,
) -> TestResult<PreparedBytes> {
    Ok(encode_public_credit_request(
        &f::scope()?,
        &RequestId::new(id.to_vec())?,
        normalize_credit_call(kind, b"global", 4, opts, 16 * f::MIB)?,
        f::clock(now, 0, now)?,
        spend,
        f::driver_limits().codec,
        16 * f::MIB,
    )?)
}
pub(super) fn frames() -> TestResult<Vec<PreparedBytes>> {
    let lease = [
        ("shard_id", V::Integer(0)),
        ("amount", V::Integer(2)),
        ("limit", V::Integer(2)),
        ("ttl_ms", V::Integer(1000)),
    ];
    let spend = [("shard_id", V::Integer(0)), ("amount", V::Integer(2))];
    Ok(vec![
        frame(b"lease", K::Lease, 1000, &lease, None)?,
        frame(
            b"spend",
            K::Spend,
            1001,
            &spend,
            Some(CreditSpendPreparation {
                lease_epoch: 1,
                batch: [0; 16],
            }),
        )?,
        frame(
            b"spend",
            K::Spend,
            1002,
            &spend,
            Some(CreditSpendPreparation {
                lease_epoch: 999,
                batch: [9; 16],
            }),
        )?,
    ])
}
pub(super) fn append(
    log: &mut RedbLogStore,
    frames: &[PreparedBytes],
    trailing: bool,
) -> TestResult {
    for (index, frame) in frames.iter().enumerate() {
        let mut body = frame.as_bytes().to_vec();
        if trailing && index == 1 {
            body.push(0);
        }
        f::append(log, u64::try_from(index)?, &body)?;
    }
    f::append(log, 3, b"opaque uncommitted public-credit suffix")?;
    log.save_committed(f::at(2))?;
    Ok(())
}
pub(super) fn apply(
    state: &mut RedbStateStore,
    frames: &[PreparedBytes],
    count: usize,
) -> TestResult {
    for (i, frame) in frames.iter().take(count).enumerate() {
        let O::Retained { outcome, .. } = apply_public_credit_transaction(
            state,
            &f::scope()?,
            frame.as_bytes(),
            f::at(u64::try_from(i)?),
            f::driver_limits(),
            CreditLimits::default(),
        )?
        else {
            return Err("missing public receipt".into());
        };
        assert_eq!(outcome.intent().version(), 17);
    }
    Ok(())
}
pub(super) fn check(state: &mut RedbStateStore) -> TestResult {
    let scope = f::scope()?;
    let owner = state
        .read_credit_owner(
            &scope,
            b"global",
            CreditLimits::default(),
            10000,
            64 * f::MIB,
        )?
        .ok_or("missing public owner")?;
    assert_eq!(owner.lease(0).ok_or("missing public lease")?.in_use, 2);
    let tx = state.begin_apply(
        scope.group(),
        scope.recovery(),
        f::driver_limits().codec.admission(),
        10000,
        64 * f::MIB,
        32 * f::MIB,
    )?;
    let outcome = tx
        .outcome(scope.tenant(), &RequestId::new(b"spend".to_vec())?)?
        .ok_or("missing original public spend")?;
    assert_eq!(
        (outcome.intent().version(), outcome.response_version()),
        (17, 10)
    );
    assert_eq!(outcome.original_position(), f::at(1));
    assert_eq!(outcome.original_time().get(), 1001);
    let reply = wf_ports::log::decode_credit_reply(
        outcome.response(),
        CreditLimits::default(),
        16 * f::MIB,
        32 * f::MIB,
    )?;
    assert_eq!(
        reply.reservation_ids,
        vec![
            b"flr1:1:AAAAAAAAAAAAAAAAAAAAAA:1".to_vec(),
            b"flr1:1:AAAAAAAAAAAAAAAAAAAAAA:2".to_vec()
        ]
    );
    assert_eq!(tx.control()?.position(), f::at(2).index());
    Ok(())
}
pub(super) fn cohort(dir: &Path) -> TestResult<BTreeMap<String, f::Rows>> {
    let db = redb::Builder::new().open(dir.join("state.redb"))?;
    let tx = db.begin_read()?;
    let mut result = BTreeMap::new();
    for handle in tx.list_tables()? {
        let table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new(handle.name()))?;
        let mut rows = BTreeMap::new();
        for row in table.iter()? {
            let (k, v) = row?;
            rows.insert(k.value().to_vec(), v.value().to_vec());
        }
        result.insert(handle.name().to_string(), rows);
    }
    assert_eq!(result.len(), 6);
    Ok(result)
}
