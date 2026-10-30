//! Real credit genesis, immutable committed frames and complete database donors.

use super::{TestResult, f};
use redb::{ReadableDatabase, ReadableTable, TableDefinition, TableHandle};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
use wf_domain::governance::credit::{CreditLimits, CreditOwner};
use wf_domain::governance::identified::{
    CreditConfiguration, IdentifiedCreditMutation as Op, IdentifiedCreditRequest,
};
use wf_domain::workflow::active::LifecycleDefaults;
use wf_domain::workflow::control::PreparedDigest;
use wf_domain::workflow::request::RetainedOutcome;
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::log::{PreparedBytes, RaftLogLimits, RaftLogStore, encode_credit_request};
use wf_ports::state::{GroupApplyStore, GroupApplyTransaction};
use wf_storage::log::RedbLogStore;
use wf_storage::state::{RedbStateStore, StateStoreGenesis, StateStoreLimits};
use wf_storage::transaction::{RecordBatchTransactionOutcome, apply_credit_transaction};
use wf_types::identity::RequestId;

pub(super) fn ids() -> Vec<Vec<u8>> {
    vec![b"a".to_vec(), vec![0, 255]]
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

pub(super) fn pair(dir: &Path, shards: Option<u64>) -> TestResult<(RedbLogStore, RedbStateStore)> {
    let scope = f::scope()?;
    let genesis = StateStoreGenesis {
        initial_time: f::clock(0, 0, 0)?,
        placements: &[],
        policies: &[],
    };
    let database = redb::Builder::new().create(dir.join("state.redb"))?;
    let state = if let Some(shards) = shards {
        RedbStateStore::create_credit_database(
            database,
            scope.group().clone(),
            scope.recovery().clone(),
            genesis,
            defaults(),
            shards,
            StateStoreLimits::default(),
        )?
    } else {
        RedbStateStore::create_stored_value_database(
            database,
            scope.group().clone(),
            scope.recovery().clone(),
            genesis,
            defaults(),
            StateStoreLimits::default(),
        )?
    };
    Ok((log(dir)?, state))
}

fn frame(
    request: &[u8],
    now: u64,
    configuration: Option<CreditConfiguration>,
    mutation: Op<'_>,
) -> TestResult<PreparedBytes> {
    Ok(encode_credit_request(
        &f::scope()?,
        &RequestId::new(request.to_vec())?,
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

pub(super) fn frames() -> TestResult<Vec<PreparedBytes>> {
    let ids = ids();
    let one = vec![b"a".to_vec()];
    Ok(vec![
        frame(
            b"lease",
            1000,
            Some(CreditConfiguration {
                limit: 3,
                config_version: Some(1),
                policy_fingerprint: Some(PreparedDigest::new([1; 32])),
            }),
            Op::Lease {
                shard_id: 0,
                amount: 3,
                ttl_ms: 1000,
            },
        )?,
        frame(
            b"spend",
            1001,
            None,
            Op::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 2,
                ttl_ms: None,
                reservation_ids: &ids,
            },
        )?,
        frame(
            b"release-a",
            1002,
            None,
            Op::Release {
                shard_id: 0,
                amount: 1,
                reservation_ids: &one,
            },
        )?,
        frame(
            b"retry",
            1003,
            None,
            Op::Spend {
                shard_id: 0,
                lease_epoch: 1,
                amount: 2,
                ttl_ms: Some(1),
                reservation_ids: &ids,
            },
        )?,
        frame(
            b"release-all",
            1004,
            None,
            Op::Release {
                shard_id: 0,
                amount: 2,
                reservation_ids: &ids,
            },
        )?,
        frame(b"cleanup", 1005, None, Op::Cleanup)?,
    ])
}

pub(super) fn apply(
    state: &mut RedbStateStore,
    frame: &PreparedBytes,
    index: u64,
) -> TestResult<Box<RetainedOutcome>> {
    let result = apply_credit_transaction(
        state,
        &f::scope()?,
        frame.as_bytes(),
        f::at(index),
        f::driver_limits(),
        CreditLimits::default(),
    )?;
    let RecordBatchTransactionOutcome::Retained { control, outcome } = result else {
        return Err("expected actual retained credit outcome".into());
    };
    assert_eq!(control.position(), f::at(index).index());
    Ok(outcome)
}

pub(super) fn input(
    dir: &Path,
    corrupt_first: bool,
) -> TestResult<(RedbLogStore, RedbStateStore, Vec<PreparedBytes>)> {
    let (mut log, mut state) = pair(dir, Some(4))?;
    let frames = frames()?;
    for (index, frame) in frames.iter().enumerate() {
        let index = u64::try_from(index)?;
        if corrupt_first && index == 0 {
            let mut bytes = frame.as_bytes().to_vec();
            bytes.push(0);
            f::append(&mut log, index, &bytes)?;
        } else {
            f::append(&mut log, index, frame.as_bytes())?;
        }
        if index < 2 {
            apply(&mut state, frame, index)?;
        }
    }
    f::append(&mut log, 6, b"uncommitted opaque credit tail")?;
    log.save_committed(f::at(5))?;
    Ok((log, state, frames))
}

pub(super) fn owner(state: &RedbStateStore) -> TestResult<CreditOwner> {
    state
        .read_credit_owner(
            &f::scope()?,
            b"global",
            CreditLimits::default(),
            10000,
            64 * f::MIB,
        )?
        .ok_or_else(|| "missing actual credit owner".into())
}

pub(super) fn outcome(state: &mut RedbStateStore, request: &[u8]) -> TestResult<RetainedOutcome> {
    let scope = f::scope()?;
    state
        .begin_apply(
            scope.group(),
            scope.recovery(),
            f::driver_limits().codec.admission(),
            10000,
            64 * f::MIB,
            32 * f::MIB,
        )?
        .outcome(scope.tenant(), &RequestId::new(request.to_vec())?)?
        .ok_or_else(|| "missing actual retained credit outcome".into())
}

pub(super) fn cohort(dir: &Path) -> TestResult<BTreeMap<String, f::Rows>> {
    let database = redb::Builder::new().open(dir.join("state.redb"))?;
    let tx = database.begin_read()?;
    assert_eq!(tx.list_multimap_tables()?.count(), 0);
    let mut result = BTreeMap::new();
    for handle in tx.list_tables()? {
        let name = handle.name();
        let table = tx.open_table(TableDefinition::<&[u8], &[u8]>::new(name))?;
        let mut rows = f::Rows::new();
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
