//! Verified original byte binding; data ownership does not certify application.

use crate::log::common::RawHeader;
use wf_domain::governance::identified::{
    CreditConfiguration, IdentifiedCreditMutation as Op, IdentifiedCreditRequest,
};
use wf_domain::workflow::{
    control::{EntryIdentity, PreparedDigest},
    request::{ClientIntent, RequestScope, gate::CommittedHeader},
};
use wf_types::{
    identity::RequestId,
    version::{CommittedTime, LogIdentity},
};

pub(super) enum OwnedCreditMutation {
    Lease {
        shard_id: u64,
        amount: u64,
        ttl_ms: u64,
    },
    Spend {
        shard_id: u64,
        lease_epoch: u64,
        amount: u64,
        ttl_ms: Option<u64>,
        ids: Vec<Vec<u8>>,
    },
    Release {
        shard_id: u64,
        amount: u64,
        ids: Vec<Vec<u8>>,
    },
    Renew {
        shard_id: u64,
        ttl_ms: u64,
    },
    Get,
    Cleanup,
}

impl OwnedCreditMutation {
    pub(super) fn borrowed(&self) -> Op<'_> {
        match self {
            Self::Lease {
                shard_id,
                amount,
                ttl_ms,
            } => Op::Lease {
                shard_id: *shard_id,
                amount: *amount,
                ttl_ms: *ttl_ms,
            },
            Self::Spend {
                shard_id,
                lease_epoch,
                amount,
                ttl_ms,
                ids,
            } => Op::Spend {
                shard_id: *shard_id,
                lease_epoch: *lease_epoch,
                amount: *amount,
                ttl_ms: *ttl_ms,
                reservation_ids: ids,
            },
            Self::Release {
                shard_id,
                amount,
                ids,
            } => Op::Release {
                shard_id: *shard_id,
                amount: *amount,
                reservation_ids: ids,
            },
            Self::Renew { shard_id, ttl_ms } => Op::Renew {
                shard_id: *shard_id,
                ttl_ms: *ttl_ms,
            },
            Self::Get => Op::Get,
            Self::Cleanup => Op::Cleanup,
        }
    }

    pub(super) const fn amount(&self) -> Option<u64> {
        match self {
            Self::Spend { amount, .. } | Self::Release { amount, .. } => Some(*amount),
            _ => None,
        }
    }

    pub(super) fn install_ids(&mut self, values: Vec<Vec<u8>>) {
        match self {
            Self::Spend { ids, .. } | Self::Release { ids, .. } => *ids = values,
            _ => {}
        }
    }
}

pub(super) struct ScannedCreditRequest<'a> {
    pub(super) header: RawHeader<'a>,
    pub(super) scope: &'a [u8],
    pub(super) shard_count: u64,
    pub(super) configuration: Option<CreditConfiguration>,
    pub(super) mutation: OwnedCreditMutation,
    pub(super) ids: Vec<&'a [u8]>,
    pub(super) body_at: usize,
    pub(super) owned_bytes: usize,
}

/// Complete prepared byte binding; no storage, routing or spend authority.
pub struct PreparedCreditRequest<'input> {
    pub(super) encoded: &'input [u8],
    pub(super) scope: RequestScope,
    pub(super) request: RequestId,
    pub(super) stamp: CommittedTime,
    pub(super) intent: ClientIntent,
    pub(super) digest: PreparedDigest,
    pub(super) isolation: Vec<u8>,
    pub(super) credit_scope: Vec<u8>,
    pub(super) shard_count: u64,
    pub(super) configuration: Option<CreditConfiguration>,
    pub(super) mutation: OwnedCreditMutation,
    pub(super) owned_bytes: usize,
}

impl PreparedCreditRequest<'_> {
    /// Borrows the exact verified original bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        self.encoded
    }

    /// Reads the prospectively admitted logical decoder cohort.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned_bytes
    }

    /// Borrows complete original fields without cloning ID payloads.
    #[must_use]
    pub fn command(&self) -> IdentifiedCreditRequest<'_> {
        IdentifiedCreditRequest {
            scope: &self.credit_scope,
            shard_count: self.shard_count,
            configuration: self.configuration,
            mutation: self.mutation.borrowed(),
        }
    }

    /// Binds the assigned log identity to verified original bytes and scope.
    #[must_use]
    pub fn header(&self, log: LogIdentity) -> CommittedHeader<'_> {
        CommittedHeader {
            format_version: 1,
            scope: &self.scope,
            request: &self.request,
            intent: self.intent,
            identity: EntryIdentity::new(log, self.digest),
            stamp: self.stamp,
            encoded_entry: self.encoded,
            encoded_isolation_scope: &self.isolation,
        }
    }
}
