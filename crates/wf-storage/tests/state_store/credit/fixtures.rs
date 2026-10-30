//! Original byte owners and complete observed cohorts, not a production oracle.

use super::*;
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
use wf_domain::workflow::active::LifecycleDefaults;
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::RetainedOutcome;
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::log::PreparedBytes;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::RecordBatchTransactionOutcome;
use wf_types::identity::RequestId;

pub(super) type Cohort = BTreeMap<String, BTreeMap<Vec<u8>, Vec<u8>>>;

pub(super) fn defaults() -> WorkflowValueDefaults {
    let lifecycle = LifecycleDefaults {
        ttl_ms: 7_000,
        history_hot_max_events: 2,
        history_max_events: 10,
        max_history_hot_max_events: 100,
        max_history_max_events: 1_000,
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

pub(super) fn create(path: &Path) -> TestResult<RedbStateStore> {
    let scope = common::scope()?;
    Ok(RedbStateStore::create_credit_database(
        redb::Builder::new().create(path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: common::clock(0)?,
            placements: &common::placements()?,
            policies: &[],
        },
        defaults(),
        4,
        StateStoreLimits::default(),
    )?)
}

pub(super) fn config(limit: u64, version: u64) -> CreditConfiguration {
    CreditConfiguration {
        limit,
        config_version: Some(version),
        policy_fingerprint: Some(PreparedDigest::new(
            [u8::try_from(version).unwrap_or(255); 32],
        )),
    }
}

pub(super) fn frame(
    request: &[u8],
    now: u64,
    configuration: Option<CreditConfiguration>,
    mutation: Mutation<'_>,
) -> TestResult<PreparedBytes> {
    Ok(encode_credit_request(
        &common::scope()?,
        &RequestId::new(request.to_vec())?,
        common::clock(now)?,
        IdentifiedCreditRequest {
            scope: b"global",
            shard_count: 4,
            configuration,
            mutation,
        },
        common::driver_limits().codec,
        16 * common::MIB,
    )?)
}

pub(super) fn retained(
    path: &Path,
    frame: &PreparedBytes,
    index: u64,
) -> TestResult<Box<RetainedOutcome>> {
    let mut store = common::open(path)?;
    let result = apply_credit_transaction(
        &mut store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(index),
        common::driver_limits(),
        CreditLimits::default(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, control } = result else {
        return Err("expected retained credit response".into());
    };
    assert_eq!(control.position().get(), index);
    Ok(outcome)
}

pub(super) fn reply(outcome: &RetainedOutcome) -> TestResult<IdentifiedCreditReply> {
    assert_eq!(outcome.intent().version(), 12);
    assert_eq!(outcome.response_version(), 10);
    Ok(decode_credit_reply(
        outcome.response(),
        CreditLimits::default(),
        16 * common::MIB,
        32 * common::MIB,
    )?)
}

pub(super) fn execute(
    path: &Path,
    request: &[u8],
    step: (u64, u64),
    configuration: Option<CreditConfiguration>,
    mutation: Mutation<'_>,
) -> TestResult<IdentifiedCreditReply> {
    let outcome = retained(
        path,
        &frame(request, step.1, configuration, mutation)?,
        step.0,
    )?;
    reply(&outcome)
}

pub(super) fn lease(path: &Path, amount: u64) -> TestResult<IdentifiedCreditReply> {
    execute(
        path,
        b"lease",
        (0, 1000),
        Some(config(amount, 1)),
        Mutation::Lease {
            shard_id: 0,
            amount,
            ttl_ms: 1000,
        },
    )
}

pub(super) fn owner(path: &Path) -> TestResult<CreditOwner> {
    common::open(path)?
        .read_credit_owner(
            &common::scope()?,
            b"global",
            CreditLimits::default(),
            10_000,
            64 * common::MIB,
        )?
        .ok_or_else(|| "missing credit owner".into())
}

pub(super) fn seed(path: &Path) -> TestResult {
    drop(create(path)?);
    assert_eq!(lease(path, 3)?.decision, Decision::Granted { shard_id: 0 });
    let ids = vec![b"a".to_vec(), b"b".to_vec()];
    assert_eq!(
        execute(
            path,
            b"spend",
            (1, 1001),
            None,
            Mutation::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 2,
                ttl_ms: None,
                reservation_ids: &ids,
            }
        )?
        .decision,
        Decision::Spent { shard_id: 0 }
    );
    Ok(())
}

pub(super) fn cohort(path: &Path) -> TestResult<Cohort> {
    let database = redb::Builder::new().open(path)?;
    let tx = database.begin_read()?;
    assert_eq!(tx.list_multimap_tables()?.count(), 0);
    let mut result = BTreeMap::new();
    for handle in tx.list_tables()? {
        let name = handle.name();
        let table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        let mut rows = BTreeMap::new();
        for row in table.iter()? {
            let (key, value) = row?;
            assert!(
                rows.insert(key.value().to_vec(), value.value().to_vec())
                    .is_none()
            );
        }
        assert!(result.insert(name.to_owned(), rows).is_none());
    }
    assert_eq!(result.len(), 6);
    Ok(result)
}

pub(super) fn family_count(path: &Path, family: u8) -> TestResult<usize> {
    let rows = common::raw_rows(path)?;
    Ok(rows
        .keys()
        .filter(|key| key.get(6) == Some(&family))
        .count())
}
