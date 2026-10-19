//! Whole-input admission and one Immediate publication of an empty namespace.

use super::{
    META, ROWS, StateStoreGenesis, StateStoreLimits,
    admission::{self, engine_pair, policy_error, storage_error, table_error, total, value_error},
    store::RedbStateStore,
};
use crate::state::{key, value};
use redb::Durability;
use sha2::{Digest, Sha256};
use wf_domain::policy::complete::{
    PolicySchemaCharge, PolicySchemaError, complete_policy_value_owned_bytes,
    verify_complete_policy_value_v2,
};
use wf_domain::workflow::control::ControlState;
use wf_ports::state::StateTransactionError;
use wf_types::identity::{GroupId, RecoveryId};

pub(super) fn admit(
    group: &GroupId,
    recovery: &RecoveryId,
    genesis: &StateStoreGenesis<'_>,
    limits: StateStoreLimits,
) -> Result<usize, StateTransactionError> {
    admission::workspace(group, recovery, limits, 0)?;
    let count = total(genesis.placements.len(), [genesis.policies.len()])?;
    if count > limits.maximum_genesis_rows() {
        return Err(StateTransactionError::Limit);
    }
    if genesis
        .policies
        .iter()
        .any(|(_, p)| p.encoded().len() > 262_162)
    {
        return Err(StateTransactionError::InvalidChanges);
    }
    let mut charge = PolicySchemaCharge::new(limits.schema());
    let mut input = 0;
    for (partition, target) in genesis.placements {
        input = total(
            input,
            [512, partition.as_bytes().len(), target.as_bytes().len()],
        )?;
    }
    for (tenant, policy) in genesis.policies {
        let owner = complete_policy_value_owned_bytes(policy, limits.schema(), &mut charge)
            .map_err(|e| match e {
                PolicySchemaError::WorkBytes { .. }
                | PolicySchemaError::LengthOverflow
                | PolicySchemaError::Policy(
                    wf_domain::policy::installation::PolicyError::StateBudget,
                ) => StateTransactionError::Limit,
                _ => StateTransactionError::InvalidChanges,
            })?;
        input = total(input, [512, tenant.as_bytes().len(), owner])?;
    }
    if input > limits.maximum_genesis_bytes() {
        return Err(StateTransactionError::Limit);
    }
    admission::workspace(group, recovery, limits, input)?;
    if genesis
        .placements
        .windows(2)
        .any(|w| w[0].0.as_bytes() >= w[1].0.as_bytes())
        || genesis.policies.windows(2).any(|w| {
            (w[0].0.as_bytes(), w[0].1.data().workflow_type.as_slice())
                >= (w[1].0.as_bytes(), w[1].1.data().workflow_type.as_slice())
        })
    {
        return Err(StateTransactionError::InvalidChanges);
    }
    for (_, policy) in genesis.policies {
        verify_complete_policy_value_v2(policy, limits.schema(), &mut charge).map_err(|e| {
            match policy_error(e) {
                StateTransactionError::Corrupt => StateTransactionError::InvalidChanges,
                other => other,
            }
        })?;
    }
    usize::try_from(charge.p()).map_err(|_| StateTransactionError::Limit)
}

pub(super) fn initialize(
    store: &RedbStateStore,
    genesis: &StateStoreGenesis<'_>,
    scratch: usize,
) -> Result<(), StateTransactionError> {
    let mut tx = store.transaction(
        false,
        store.limits.admission(),
        store.limits.maximum_recovery_steps(),
        usize::MAX,
        store.limits.maximum_temporary_bytes(),
    )?;
    ensure_empty(&tx.write)?;
    tx.temporary(scratch)?;
    let mut rows = tx.write.open_table(ROWS).map_err(table_error)?;
    let mut hash = Sha256::new();
    hash.update(b"wf-state-genesis-v1");
    for (tenant, policy) in genesis.policies {
        let scope = tx.scope(tenant)?;
        let key = tx.output_key(
            &[
                store.group.as_bytes(),
                store.recovery.as_bytes(),
                tenant.as_bytes(),
                &policy.data().workflow_type,
            ],
            0,
            policy.encoded().len(),
            |cap| key::encode_policy_key(&scope, &policy.data().workflow_type, cap),
        )?;
        engine_pair(key.len(), policy.encoded().len())?;
        hash_row(&mut hash, &key, policy.encoded())?;
        rows.insert(key.as_slice(), policy.encoded())
            .map_err(|e| storage_error(&e))?;
    }
    for (partition, target) in genesis.placements {
        let key = tx.output_key(
            &[store.recovery.as_bytes(), partition.as_bytes()],
            0,
            target.as_bytes().len(),
            |cap| key::encode_placement_key(&store.recovery, partition, cap),
        )?;
        engine_pair(key.len(), target.as_bytes().len())?;
        hash_row(&mut hash, &key, target.as_bytes())?;
        rows.insert(key.as_slice(), target.as_bytes())
            .map_err(|e| storage_error(&e))?;
    }
    let bytes = total(
        33,
        [
            store.group.as_bytes().len(),
            store.recovery.as_bytes().len(),
        ],
    )?;
    let control_key = tx.output_key(
        &[store.group.as_bytes(), store.recovery.as_bytes()],
        0,
        bytes,
        |cap| key::encode_control_key(&store.group, &store.recovery, cap),
    )?;
    engine_pair(control_key.len(), bytes)?;
    tx.temporary(bytes)?;
    let control = value::encode_control(
        &store.group,
        &store.recovery,
        ControlState::initial(genesis.initial_time),
        bytes,
    )
    .map_err(value_error)?;
    rows.insert(control_key.as_slice(), control.as_slice())
        .map_err(|e| storage_error(&e))?;
    drop(rows);
    let bytes = total(
        108,
        [
            store.group.as_bytes().len(),
            store.recovery.as_bytes().len(),
        ],
    )?;
    tx.temporary(bytes)?;
    engine_pair(8, bytes)?;
    let manifest = manifest(store, genesis, hash.finalize().into(), bytes)?;
    let mut meta = tx.write.open_table(META).map_err(table_error)?;
    meta.insert(b"manifest".as_slice(), manifest.as_slice())
        .map_err(|e| storage_error(&e))?;
    drop(meta);
    tx.write
        .set_durability(Durability::Immediate)
        .map_err(|_| StateTransactionError::Unavailable)?;
    tx.write
        .commit()
        .map_err(|_| StateTransactionError::Uncertain)
}

fn ensure_empty(write: &redb::WriteTransaction) -> Result<(), StateTransactionError> {
    if write
        .list_tables()
        .map_err(|e| storage_error(&e))?
        .next()
        .is_some()
        || write
            .list_multimap_tables()
            .map_err(|e| storage_error(&e))?
            .next()
            .is_some()
    {
        return Err(StateTransactionError::NamespaceConflict);
    }
    Ok(())
}

fn manifest(
    store: &RedbStateStore,
    genesis: &StateStoreGenesis<'_>,
    hash: [u8; 32],
    bytes: usize,
) -> Result<Vec<u8>, StateTransactionError> {
    let mut out = Vec::new();
    out.try_reserve_exact(bytes)
        .map_err(|_| StateTransactionError::Allocation)?;
    out.extend_from_slice(b"FWSS\0\x01");
    for field in [store.group.as_bytes(), store.recovery.as_bytes()] {
        out.extend_from_slice(
            &u16::try_from(field.len())
                .map_err(|_| StateTransactionError::Limit)?
                .to_be_bytes(),
        );
        out.extend_from_slice(field);
    }
    out.extend_from_slice(&genesis.initial_time.hlc().physical().get().to_be_bytes());
    out.extend_from_slice(&genesis.initial_time.hlc().logical().to_be_bytes());
    out.extend_from_slice(&genesis.initial_time.wall().get().to_be_bytes());
    for n in [genesis.placements.len(), genesis.policies.len()] {
        out.extend_from_slice(
            &u64::try_from(n)
                .map_err(|_| StateTransactionError::Limit)?
                .to_be_bytes(),
        );
    }
    out.extend_from_slice(&hash);
    let checksum = Sha256::digest(&out);
    out.extend_from_slice(&checksum);
    Ok(out)
}

pub(super) fn hash_row(
    hash: &mut Sha256,
    key: &[u8],
    value: &[u8],
) -> Result<(), StateTransactionError> {
    hash.update(
        u64::try_from(key.len())
            .map_err(|_| StateTransactionError::Limit)?
            .to_be_bytes(),
    );
    hash.update(key);
    hash.update(
        u64::try_from(value.len())
            .map_err(|_| StateTransactionError::Limit)?
            .to_be_bytes(),
    );
    hash.update(value);
    Ok(())
}
