use super::*;
use redb::TableDefinition;
use std::path::Path;
use wf_domain::workflow::request::RetainedOutcome;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::RecordBatchTransactionOutcome;
use wf_types::identity::RequestId;

pub(super) fn create(path: &Path) -> TestResult<RedbStateStore> {
    let scope = common::scope()?;
    Ok(RedbStateStore::create_credit_catalog_database(
        redb::Builder::new().create(path)?,
        scope.group().clone(),
        scope.recovery().clone(),
        StateStoreGenesis {
            initial_time: common::clock(0)?,
            placements: &common::placements()?,
            policies: &[],
        },
        credit::defaults(),
        4,
        StateStoreLimits::default(),
    )?)
}

pub(super) fn apply(
    path: &Path,
    name: &[u8],
    request: &[u8],
    index: u64,
    limit: Option<u64>,
    mutation: Mutation<'_>,
) -> TestResult<Box<RetainedOutcome>> {
    let frame = encode_credit_request(
        &common::scope()?,
        &RequestId::new(request.to_vec())?,
        common::clock(1000)?,
        IdentifiedCreditRequest {
            scope: name,
            shard_count: 4,
            configuration: limit.map(|n| credit::config(n, 1)),
            mutation,
        },
        common::driver_limits().codec,
        16 * common::MIB,
    )?;
    let mut store = common::open(path)?;
    let result = apply_credit_transaction(
        &mut store,
        &common::scope()?,
        frame.as_bytes(),
        common::log(index),
        common::driver_limits(),
        CreditLimits::default(),
    )?;
    let RecordBatchTransactionOutcome::Retained { outcome, .. } = result else {
        return Err("expected retained catalog-capable response".into());
    };
    Ok(outcome)
}

pub(super) fn lease(path: &Path, name: &[u8], index: u64) -> TestResult<Box<RetainedOutcome>> {
    apply(
        path,
        name,
        name,
        index,
        Some(3),
        Mutation::Lease {
            shard_id: 0,
            amount: 3,
            ttl_ms: 1000,
        },
    )
}

pub(super) fn page(path: &Path, limit: u16) -> TestResult<CreditCatalogPage> {
    Ok(common::open(path)?.read_credit_catalog(
        &common::scope()?,
        limit,
        10_000,
        64 * common::MIB,
    )?)
}

pub(super) fn row(path: &Path, family: u8) -> TestResult<(Vec<u8>, Vec<u8>)> {
    common::raw_rows(path)?
        .into_iter()
        .find(|(key, _)| key.get(6) == Some(&family))
        .ok_or_else(|| "missing publication row".into())
}

pub(super) fn replace(path: &Path, key: &[u8], value: Option<&[u8]>) -> TestResult {
    let database = redb::Builder::new().open(path)?;
    let tx = database.begin_write()?;
    {
        let mut table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new("wf_state_rows_v1"))?;
        if let Some(value) = value {
            table.insert(key, value)?;
        } else {
            table.remove(key)?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub(super) fn meta(head: u64, tail: u64) -> Vec<u8> {
    let mut bytes = b"FWCM\0\x01".to_vec();
    bytes.extend_from_slice(&head.to_be_bytes());
    bytes.extend_from_slice(&tail.to_be_bytes());
    bytes
}
