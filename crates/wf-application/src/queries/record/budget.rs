//! Continuous whole-call logical resource admission before allocation.
use super::{
    StrongWorkflowRecordError as E, StrongWorkflowRecordLimits as L,
    StrongWorkflowRecordRequest as R, StrongWorkflowRecordResult,
};
use std::{borrow::Cow, mem::size_of};
use wf_domain::{
    metadata::scope::{OriginalScopeError, OriginalScopeLimits, original_physical_partition},
    workflow::request::RequestScope,
};
use wf_types::identity::{GroupId, PartitionId, RecoveryId, TenantId};
pub(super) struct Budget {
    pub(super) limits: L,
    pub(super) work: usize,
    pub(super) live: usize,
    pub(super) peak: usize,
}
pub(super) fn add(a: usize, b: usize) -> Result<usize, E> {
    a.checked_add(b).ok_or(E::Limit)
}
pub(super) fn scope_bytes(scope: &RequestScope) -> Result<usize, E> {
    add(
        add(
            scope.tenant().as_bytes().len(),
            scope.group().as_bytes().len(),
        )?,
        scope.recovery().as_bytes().len(),
    )
}
impl Budget {
    pub(super) fn new(request: R<'_>, limits: L) -> Result<Self, E> {
        let q = limits.preparation.query;
        let r = limits.read.record;
        let m = limits.preparation.metadata;
        if [
            limits.maximum_input_bytes,
            q.maximum_output_bytes,
            q.maximum_work_units,
            q.maximum_temporary_bytes,
            r.maximum_row_reads,
            r.maximum_inspected_bytes,
            r.maximum_returned_bytes,
            r.maximum_work,
            r.maximum_temporary_bytes,
            m.max_encoded_bytes,
            m.max_nodes,
            m.max_edges,
            m.max_depth,
            m.max_work_units,
        ]
        .contains(&0)
        {
            return Err(E::InvalidLimits);
        }
        let mut input = scope_bytes(request.scope)?;
        for bytes in [
            request.id.as_bytes(),
            request.partition,
            request.scope_prefix.unwrap_or_default(),
        ] {
            input = add(input, bytes.len())?;
        }
        if let Some(after) = request.after {
            input = add(
                input,
                add(
                    after.group.as_bytes().len(),
                    after.recovery.as_bytes().len(),
                )?,
            )?;
        }
        if input > limits.maximum_input_bytes {
            return Err(E::Limit);
        }
        let mut b = Self {
            limits,
            work: 0,
            live: 0,
            peak: 0,
        };
        b.spend(add(8192, input)?)?;
        b.admit(add(
            add(8192, size_of::<StrongWorkflowRecordResult>())?,
            input,
        )?)?;
        Ok(b)
    }
    pub(super) fn spend(&mut self, n: usize) -> Result<(), E> {
        let next = add(self.work, n)?;
        if next > self.limits.preparation.query.maximum_work_units {
            return Err(E::Limit);
        }
        self.work = next;
        Ok(())
    }
    pub(super) fn admit(&mut self, n: usize) -> Result<(), E> {
        let next = add(self.live, n)?;
        if next > self.limits.preparation.query.maximum_temporary_bytes {
            return Err(E::Limit);
        }
        self.live = next;
        self.peak = self.peak.max(next);
        Ok(())
    }
    pub(super) fn peer(&mut self, work: usize, temporary: usize) -> Result<(), E> {
        self.spend(work)?;
        let peak = add(self.live, temporary)?;
        if peak > self.limits.preparation.query.maximum_temporary_bytes {
            return Err(E::Limit);
        }
        self.peak = self.peak.max(peak);
        Ok(())
    }
    pub(super) fn remaining(&self) -> Result<usize, E> {
        self.limits
            .preparation
            .query
            .maximum_work_units
            .checked_sub(self.work)
            .filter(|n| *n > 0)
            .ok_or(E::Limit)
    }
    pub(super) fn spare(&self) -> Result<usize, E> {
        self.limits
            .preparation
            .query
            .maximum_temporary_bytes
            .checked_sub(self.live)
            .filter(|n| *n > 0)
            .ok_or(E::Limit)
    }
    pub(super) fn copy(&mut self, bytes: &[u8]) -> Result<Vec<u8>, E> {
        self.spend(add(bytes.len(), 1)?)?;
        self.admit(bytes.len())?;
        let mut out = Vec::new();
        out.try_reserve_exact(bytes.len())
            .map_err(|_| E::Allocation)?;
        out.extend_from_slice(bytes);
        Ok(out)
    }
    pub(super) fn scope(&mut self, source: &RequestScope) -> Result<RequestScope, E> {
        Ok(RequestScope::new(
            TenantId::new(self.copy(source.tenant().as_bytes())?)
                .map_err(|_| E::InvalidObservation)?,
            GroupId::new(self.copy(source.group().as_bytes())?)
                .map_err(|_| E::InvalidObservation)?,
            RecoveryId::new(self.copy(source.recovery().as_bytes())?)
                .map_err(|_| E::InvalidObservation)?,
        ))
    }
    pub(super) fn route(&mut self, request: R<'_>) -> Result<PartitionId, E> {
        let result = original_physical_partition(
            request.partition,
            request.scope_prefix,
            OriginalScopeLimits {
                maximum_output_bytes: self.spare()?,
                maximum_work_units: self.remaining()?,
                maximum_temporary_bytes: self.spare()?,
            },
        )
        .map_err(|e| match e {
            OriginalScopeError::Limit => E::Limit,
            other => E::Scope(other),
        })?;
        self.peer(result.work_units, result.temporary_bytes)?;
        let bytes = match result.value {
            Cow::Borrowed(bytes) => self.copy(bytes)?,
            Cow::Owned(bytes) => {
                self.admit(bytes.len())?;
                bytes
            }
        };
        PartitionId::new(bytes).map_err(|_| E::InvalidObservation)
    }
    pub(super) fn output(&self, bytes: usize) -> Result<(), E> {
        if bytes > self.limits.preparation.query.maximum_output_bytes {
            Err(E::Limit)
        } else {
            Ok(())
        }
    }
}
