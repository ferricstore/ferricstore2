//! Sparse exact observations sharing one captured capability and real writer.

use super::super::{
    admission::{storage_error, table_error, total, value_error},
    store::RedbStateTransaction,
};
use crate::state::{key, value};
use redb::ReadableTable;
use wf_domain::governance::credit::{CreditLeaseData, CreditLimits, CreditOwner};
use wf_domain::governance::identified::{
    CreditProtocolObservation as O, CreditProtocolQuery as Q, CreditReservationObservation,
};
use wf_domain::workflow::request::RequestScope;
use wf_ports::log::decode_credit_owner;
use wf_ports::state::StateTransactionError as E;

pub(crate) struct CreditReadContext<'a, 's> {
    pub(super) tx: &'a RedbStateTransaction<'s>,
    pub(super) scope: &'a RequestScope,
    pub(super) credit_scope: &'a [u8],
    pub(crate) shards: u64,
    credit: CreditLimits,
}

impl<'a, 's> CreditReadContext<'a, 's> {
    pub(crate) fn new(
        tx: &'a RedbStateTransaction<'s>,
        scope: &'a RequestScope,
        credit_scope: &'a [u8],
        credit: CreditLimits,
    ) -> Result<Self, E> {
        if scope.group() != tx.group || scope.recovery() != tx.recovery {
            return Err(E::NamespaceConflict);
        }
        if credit_scope.is_empty() || credit_scope.len() > 65_535 {
            return Err(E::InvalidChanges);
        }
        let shards = tx
            .checked(|| tx.value_manifest())?
            .credit_shard_count
            .ok_or(E::UnsupportedPolicyProfile)?;
        Ok(Self {
            tx,
            scope,
            credit_scope,
            shards,
            credit,
        })
    }

    pub(crate) fn owner(&self) -> Result<Option<CreditOwner>, E> {
        let key = self.owner_key()?;
        self.tx
            .checked(|| self.read(&key, |bytes| self.decode_owner(bytes)))
    }

    pub(super) fn recovered(
        tx: &'a RedbStateTransaction<'s>,
        scope: &'a RequestScope,
        credit_scope: &'a [u8],
        shards: u64,
    ) -> Self {
        Self {
            tx,
            scope,
            credit_scope,
            shards,
            credit: CreditLimits::default(),
        }
    }

    pub(super) fn decode_owner(&self, bytes: &[u8]) -> Result<CreditOwner, E> {
        if bytes.len() > 900_000 {
            return Err(E::Corrupt);
        }
        let bound = bytes
            .len()
            .checked_div(61)
            .and_then(|n| n.checked_mul(std::mem::size_of::<CreditLeaseData>()))
            .and_then(|n| n.checked_add(bytes.len()))
            .and_then(|n| n.checked_add(2048))
            .ok_or(E::Limit)?;
        self.tx.admit_read(bound)?;
        let owner = decode_credit_owner(bytes, self.credit, 900_000, bound)
            .map_err(super::read::codec_error)?;
        if owner.view().scope != self.credit_scope
            || owner
                .view()
                .leases
                .iter()
                .any(|l| l.shard_id >= self.shards)
        {
            return Err(E::Corrupt);
        }
        Ok(owner)
    }

    pub(crate) fn observe<'ids>(&self, query: Q<'ids>) -> Result<O<'ids>, E> {
        self.tx.checked(|| self.observe_inner(query))
    }

    fn observe_inner<'ids>(&self, query: Q<'ids>) -> Result<O<'ids>, E> {
        match query {
            Q::Reservations {
                shard_id,
                epoch,
                ids,
            } => {
                self.tx.admit_read(
                    ids.len()
                        .checked_mul(std::mem::size_of::<CreditReservationObservation<'_>>())
                        .ok_or(E::Limit)?,
                )?;
                let mut observations = Vec::new();
                observations
                    .try_reserve_exact(ids.len())
                    .map_err(|_| E::Allocation)?;
                self.tx.step()?;
                let table = self
                    .tx
                    .write
                    .open_table(self.tx.rows_table)
                    .map_err(table_error)?;
                for id in ids {
                    let key = self.reservation_key(shard_id, epoch, id)?;
                    self.tx.step()?;
                    let row = table.get(key.as_slice()).map_err(|e| storage_error(&e))?;
                    let status = row
                        .map(|row| reservation_status(row.value(), id))
                        .transpose()?;
                    observations.push(CreditReservationObservation {
                        id,
                        shard_id,
                        epoch,
                        status,
                    });
                }
                Ok(O::Reservations(observations))
            }
            Q::Page {
                shard_id,
                epoch,
                page,
            } => {
                let key = self.page_key(shard_id, epoch, page)?;
                Ok(O::Page(self.read(&key, |bytes| {
                    self.tx.admit_read(total(
                        8192,
                        [bytes.len(), 256 * std::mem::size_of::<Vec<u8>>()],
                    )?)?;
                    value::credit::page(bytes, (shard_id, epoch, page)).map_err(value_error)
                })?))
            }
            Q::CleanupTask { sequence } => {
                let key = self.cleanup_key(sequence)?;
                Ok(O::CleanupTask(self.read(&key, |bytes| {
                    self.tx.admit_read(128)?;
                    value::credit::cleanup(bytes, sequence).map_err(value_error)
                })?))
            }
        }
    }

    pub(super) fn read<T>(
        &self,
        key: &[u8],
        decode: impl FnOnce(&[u8]) -> Result<T, E>,
    ) -> Result<Option<T>, E> {
        self.tx.step()?;
        let table = self
            .tx
            .write
            .open_table(self.tx.rows_table)
            .map_err(table_error)?;
        self.tx.step()?;
        table
            .get(key)
            .map_err(|e| storage_error(&e))?
            .map(|row| decode(row.value()))
            .transpose()
    }

    pub(super) fn owner_key(&self) -> Result<Vec<u8>, E> {
        self.key(&[], 0, |cap| {
            key::encode_credit_owner_key(self.scope, self.credit_scope, cap)
        })
    }

    pub(super) fn reservation_key(&self, shard: u64, epoch: u64, id: &[u8]) -> Result<Vec<u8>, E> {
        self.key(
            &[&shard.to_be_bytes(), &epoch.to_be_bytes(), id],
            0,
            |cap| {
                key::encode_credit_reservation_key(
                    self.scope,
                    self.credit_scope,
                    shard,
                    epoch,
                    id,
                    cap,
                )
            },
        )
    }

    pub(super) fn page_key(&self, shard: u64, epoch: u64, page: u16) -> Result<Vec<u8>, E> {
        self.key(&[&shard.to_be_bytes(), &epoch.to_be_bytes()], 8, |cap| {
            key::encode_credit_page_key(self.scope, self.credit_scope, shard, epoch, page, cap)
        })
    }

    pub(super) fn cleanup_key(&self, sequence: u64) -> Result<Vec<u8>, E> {
        self.key(&[], 8, |cap| {
            key::encode_credit_cleanup_key(self.scope, self.credit_scope, sequence, cap)
        })
    }

    fn key(
        &self,
        extra: &[&[u8]],
        scalar: usize,
        encode: impl FnOnce(usize) -> Result<Vec<u8>, value::StateValueError>,
    ) -> Result<Vec<u8>, E> {
        let mut fields = [&[][..]; 7];
        fields[..4].copy_from_slice(&[
            self.scope.group().as_bytes(),
            self.scope.recovery().as_bytes(),
            self.scope.tenant().as_bytes(),
            self.credit_scope,
        ]);
        fields[4..4 + extra.len()].copy_from_slice(extra);
        self.tx.key(&fields[..4 + extra.len()], scalar, encode)
    }
}

fn reservation_status(
    bytes: &[u8],
    id: &[u8],
) -> Result<wf_domain::governance::identified::CreditReservationStatus, E> {
    let row = value::credit::reservation(bytes).map_err(value_error)?;
    if row.id != id {
        return Err(E::Corrupt);
    }
    Ok(row.status)
}
