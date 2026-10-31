//! One empty-database Immediate publication of all value-capable genesis facts.

use redb::Durability;
use sha2::{Digest, Sha256};
use std::mem::size_of;
use wf_ports::consensus::WorkflowValueDefaults;
use wf_ports::state::StateTransactionError as Error;
use wf_types::identity::{GroupId, RecoveryId};

use super::super::{
    META, StateStoreGenesis, StateStoreLimits, admission, bootstrap,
    core_policy::catalog::CurrentPolicyManifest, membership, snapshot, store::RedbStateStore,
};
use super::manifest::{self, ValueManifest};

pub(super) fn create(
    database: redb::Database,
    group: GroupId,
    recovery: RecoveryId,
    genesis: StateStoreGenesis<'_>,
    defaults: WorkflowValueDefaults,
    limits: StateStoreLimits,
    alias_format: u16,
) -> Result<RedbStateStore, Error> {
    create_profile(
        database,
        group,
        recovery,
        genesis,
        defaults,
        limits,
        (alias_format, None, false),
    )
}

pub(in crate::state::redb) fn create_profile(
    database: redb::Database,
    group: GroupId,
    recovery: RecoveryId,
    genesis: StateStoreGenesis<'_>,
    defaults: WorkflowValueDefaults,
    limits: StateStoreLimits,
    capability: (u16, Option<u64>, bool),
) -> Result<RedbStateStore, Error> {
    if !matches!(capability.0, 1 | 2) {
        return Err(Error::UnsupportedPolicyProfile);
    }
    if capability.2 && capability.1.is_none() {
        return Err(Error::InvalidChanges);
    }
    if capability.1.is_some_and(|count| {
        count == 0 || count > wf_domain::workflow::MAX_EXACT_MILLIS || capability.0 != 2
    }) {
        return Err(Error::InvalidChanges);
    }
    defaults.to_bytes().map_err(|_| Error::InvalidChanges)?;
    let scratch = bootstrap::admit(&group, &recovery, &genesis, limits)?;
    admission::workspace(
        &group,
        &recovery,
        limits,
        admission::total(
            scratch,
            [additional(&group, &recovery, capability.1.is_some())?],
        )?,
    )?;
    let store = RedbStateStore::unvalidated(database, group, recovery, limits);
    initialize(&store, &genesis, defaults, scratch, capability)?;
    Ok(store)
}

fn initialize(
    store: &RedbStateStore,
    genesis: &StateStoreGenesis<'_>,
    defaults: WorkflowValueDefaults,
    scratch: usize,
    capability: (u16, Option<u64>, bool),
) -> Result<(), Error> {
    let mut tx = store.transaction(
        false,
        store.limits.admission(),
        store.limits.maximum_recovery_steps(),
        usize::MAX,
        store.limits.maximum_temporary_bytes(),
    )?;
    bootstrap::ensure_empty(&tx.write)?;
    tx.temporary(admission::total(
        scratch,
        [additional(
            &store.group,
            &store.recovery,
            capability.1.is_some(),
        )?],
    )?)?;
    let genesis_hash = bootstrap::write_genesis_rows(&tx, genesis)?;
    let origin_size = admission::total(
        108,
        [
            store.group.as_bytes().len(),
            store.recovery.as_bytes().len(),
        ],
    )?;
    tx.temporary(origin_size)?;
    let origin = bootstrap::manifest(store, genesis, genesis_hash, origin_size)?;
    let current_hash = current_hash(&tx)?;
    let current = ValueManifest {
        layout: if capability.1.is_some() { 8 } else { 7 },
        alias_format: capability.0,
        credit_shard_count: capability.1,
        credit_catalog: capability.2,
        policy: CurrentPolicyManifest {
            initial_time: genesis.initial_time,
            placements: u64::try_from(genesis.placements.len()).map_err(|_| Error::Limit)?,
            policies: u64::try_from(genesis.policies.len()).map_err(|_| Error::Limit)?,
            catalog_hash: current_hash,
            origin_hash: Sha256::digest(&origin).into(),
        },
        defaults,
    };
    let mut table = tx.write.open_table(META).map_err(admission::table_error)?;
    tx.step()?;
    admission::engine_pair(6, origin.len())?;
    table
        .insert(b"origin".as_slice(), origin.as_slice())
        .map_err(|e| admission::storage_error(&e))?;
    drop(table);
    manifest::publish(&tx, current)?;
    membership::write(&tx, None, None)?;
    tx.layout = current.layout;
    snapshot::initialize(&tx)?;
    let saved_layout = tx.committed_layout;
    tx.write
        .set_durability(Durability::Immediate)
        .map_err(|_| Error::Unavailable)?;
    tx.write.commit().map_err(|_| Error::Uncertain)?;
    saved_layout.set(current.layout);
    Ok(())
}

fn additional(group: &GroupId, recovery: &RecoveryId, credit: bool) -> Result<usize, Error> {
    admission::total(
        size_of::<ValueManifest>(),
        [
            2 * size_of::<Vec<u8>>(),
            size_of::<WorkflowValueDefaults>(),
            size_of::<Sha256>(),
            60,
            32,
            admission::total(108, [group.as_bytes().len(), recovery.as_bytes().len()])?,
            admission::total(
                208,
                [
                    group.as_bytes().len(),
                    recovery.as_bytes().len(),
                    if credit { 10 } else { 0 },
                ],
            )?,
        ],
    )
}

fn current_hash(tx: &super::super::store::RedbStateTransaction<'_>) -> Result<[u8; 32], Error> {
    // Reuse the actual family6/7 census and its retained original-policy domain.
    let (_, _, hash) = super::super::core_policy::catalog::census(tx, 7)?;
    Ok(hash)
}
