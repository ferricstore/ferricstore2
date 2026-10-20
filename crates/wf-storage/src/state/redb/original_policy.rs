//! Full persisted policy union and one consumed original-policy writer.

use super::{
    admission::{copy_bytes, policy_error, storage_error, table_error, total},
    core_policy,
    store::RedbStateTransaction,
};
use crate::state::key;
use redb::ReadableTable;
use wf_domain::policy::complete::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
    decode_complete_policy_value_v2, decode_original_complete_policy_entry_portable,
};
use wf_ports::state::{
    GroupOriginalPolicyChanges, GroupOriginalPolicyTransaction, StateTransactionError as Error,
    StoredPolicyValue,
};
use wf_types::identity::TenantId;

pub(in crate::state::redb) fn original_owner(length: usize) -> Result<usize, Error> {
    length
        .checked_mul(256)
        .and_then(|n| n.checked_add(65_536))
        .ok_or(Error::Limit)
}

impl RedbStateTransaction<'_> {
    pub(crate) fn original_policy_temporary(&self, bytes: usize) -> Result<(), Error> {
        self.checked(|| self.temporary(bytes))
    }
    pub(crate) fn original_policy_reply_defaults(&self) -> wf_domain::retention::ResolvedRetention {
        self.limits.original_policy_reply_defaults()
    }
    pub(in crate::state::redb) fn read_stored_policy(
        &self,
        tenant: &TenantId,
        kind: &[u8],
        schema: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<StoredPolicyValue>, Error> {
        Ok(self
            .read_stored_with_generation(tenant, kind, schema, charge)?
            .map(|(value, _)| value))
    }
    pub(in crate::state::redb) fn read_stored_with_generation(
        &self,
        tenant: &TenantId,
        kind: &[u8],
        schema: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<(StoredPolicyValue, u64)>, Error> {
        core_policy::validate_profile(self, schema, charge)?;
        self.step()?;
        let scope = self.scope(tenant)?;
        let key = self.key(
            &[
                self.group.as_bytes(),
                self.recovery.as_bytes(),
                tenant.as_bytes(),
                kind,
            ],
            0,
            |cap| key::encode_policy_key(&scope, kind, cap),
        )?;
        self.opened()?;
        let table = self
            .write
            .open_table(self.rows_table)
            .map_err(table_error)?;
        let guard = table.get(key.as_slice()).map_err(|e| storage_error(&e))?;
        let Some(guard) = guard else {
            return Ok(None);
        };
        self.recovery_row(&key, guard.value())?;
        decode_stored_with_generation(self, guard.value(), kind, schema, charge, true, self.layout)
            .map(Some)
    }

    pub(crate) fn commit_original_policy_borrowed(
        self,
        changes: &GroupOriginalPolicyChanges<'_>,
        schema: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<u16, Error> {
        super::commit::original_policy::apply(self, changes, schema, charge)
    }
}
impl GroupOriginalPolicyTransaction for RedbStateTransaction<'_> {
    fn original_policy_profile(&self) -> Result<u32, Error> {
        self.ready()?;
        if self.recovering || self.snapshot_cohort || !(3..=6).contains(&self.layout) {
            return Err(Error::UnsupportedLayout);
        }
        Ok(super::snapshot::original::PROFILE)
    }
    fn original_policy_current(
        &self,
        tenant: &TenantId,
        kind: &[u8],
        schema: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<Option<Vec<u8>>, Error> {
        self.checked(
            || match self.read_stored_policy(tenant, kind, schema, charge)? {
                None => Ok(None),
                Some(StoredPolicyValue::Original(raw)) => Ok(Some(raw)),
                Some(StoredPolicyValue::Complete(value)) => {
                    self.admit_read(total(512, [value.encoded().len()])?)?;
                    Ok(Some(copy_bytes(value.encoded())?))
                }
            },
        )
    }
    fn commit_original_policy(
        self,
        changes: GroupOriginalPolicyChanges<'_>,
        schema: PolicySchemaLimits,
        charge: &mut PolicySchemaCharge,
    ) -> Result<u16, Error> {
        self.commit_original_policy_borrowed(&changes, schema, charge)
    }
}

pub(in crate::state::redb) fn decode_stored(
    tx: &RedbStateTransaction<'_>,
    raw: &[u8],
    kind: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    normal_read: bool,
    layout: u16,
) -> Result<StoredPolicyValue, Error> {
    Ok(decode_stored_with_generation(tx, raw, kind, schema, charge, normal_read, layout)?.0)
}
fn decode_stored_with_generation(
    tx: &RedbStateTransaction<'_>,
    raw: &[u8],
    kind: &[u8],
    schema: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    normal_read: bool,
    layout: u16,
) -> Result<(StoredPolicyValue, u64), Error> {
    if raw.starts_with(b"FWPV") {
        let owner = raw
            .len()
            .checked_mul(216)
            .and_then(|n| n.checked_add(2048))
            .ok_or(Error::Limit)?;
        admit(tx, owner, normal_read)?;
        let value = decode_complete_policy_value_v2(raw, schema, charge).map_err(policy_error)?;
        if value.data().workflow_type != kind {
            return Err(Error::Corrupt);
        }
        let generation = value.generation();
        return Ok((StoredPolicyValue::Complete(Box::new(value)), generation));
    }
    if raw.first() != Some(&131) || layout != 6 {
        return Err(Error::Corrupt);
    }
    if raw.len() > wf_domain::policy::installation::MAX_ENCODED_POLICY_BYTES {
        return Err(Error::Corrupt);
    }
    admit(tx, original_owner(raw.len())?, normal_read)?;
    let value = decode_original_complete_policy_entry_portable(raw, schema, charge)
        .map_err(original_error)?
        .ok_or(Error::Corrupt)?;
    if value.workflow_type() != Some(kind) {
        return Err(Error::Corrupt);
    }
    admit(tx, total(512, [raw.len()])?, normal_read)?;
    Ok((
        StoredPolicyValue::Original(copy_bytes(raw)?),
        value.generation(),
    ))
}
fn admit(tx: &RedbStateTransaction<'_>, owner: usize, read: bool) -> Result<(), Error> {
    if read {
        tx.admit_read(owner)
    } else {
        tx.temporary(owner)
    }
}
pub(in crate::state::redb) fn original_error(error: CompletePolicyEncodingError) -> Error {
    match error {
        CompletePolicyEncodingError::UnsupportedPolicyProfile => Error::UnsupportedPolicyProfile,
        CompletePolicyEncodingError::UnsupportedSourceOrder => Error::UnsupportedSourceOrder,
        CompletePolicyEncodingError::ByteLimit | CompletePolicyEncodingError::ItemLimit => {
            Error::Limit
        }
        CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(
            wf_domain::policy::installation::PolicyError::StateBudget,
        )) => Error::Limit,
        CompletePolicyEncodingError::Schema(PolicySchemaError::InvalidAuthority) => {
            Error::InvalidChanges
        }
        other => policy_error(other),
    }
}
