//! Prospective logical ownership and nonrefundable preparation work.

use std::mem::size_of;

use crate::value::{OriginalValueEncodeLimits, OriginalValueNode, PreparedOriginalFlowValue};
use crate::workflow::caller::logical::{
    LogicalMutationError as E, LogicalMutationFieldValue, LogicalMutationInput,
    LogicalMutationLimits as L, PreparedLogicalMutation,
};
use crate::workflow::caller::{CoreMutation, MutationFieldValue, NormalizedMutation};

pub(super) struct Budget {
    pub(super) limits: L,
    pub(super) work: usize,
    pub(super) peak: usize,
    fixed: usize,
    retained: usize,
}

impl Budget {
    pub(super) fn new(limits: L) -> Result<Self, E> {
        let value = limits.values;
        if [
            limits.max_raw_bytes,
            limits.max_work_units,
            limits.max_temporary_bytes,
            value.max_input_bytes,
            value.max_nodes,
            value.max_edges,
            value.max_depth,
            value.max_encoded_bytes,
            value.max_work_units,
            value.max_temporary_bytes,
        ]
        .contains(&0)
        {
            return Err(E::InvalidLimits);
        }
        let fixed = fixed_bytes()?;
        let mut budget = Self {
            limits,
            work: 0,
            peak: 0,
            fixed,
            retained: size_of::<PreparedLogicalMutation>(),
        };
        budget.spend(fixed)?;
        budget.observe(0)?;
        Ok(budget)
    }

    pub(super) fn spend(&mut self, amount: usize) -> Result<(), E> {
        let next = add(self.work, amount)?;
        if next > self.limits.max_work_units {
            return Err(E::Limit);
        }
        self.work = next;
        Ok(())
    }

    pub(super) fn observe(&mut self, scratch: usize) -> Result<(), E> {
        let total = add(add(self.fixed, self.retained)?, scratch)?;
        if total > self.limits.max_temporary_bytes {
            return Err(E::Limit);
        }
        self.peak = self.peak.max(total);
        Ok(())
    }

    pub(super) fn metadata(&mut self, caller: &NormalizedMutation<'_>) -> Result<(), E> {
        let mut bytes = add(caller.id().len(), caller.partition().map_or(0, <[u8]>::len))?;
        let widths = match caller.operation() {
            CoreMutation::Create {
                workflow_type,
                state,
                ..
            } => [workflow_type.len(), state.len(), 0],
            CoreMutation::Transition {
                from,
                to,
                lease_token,
                ..
            } => [from.len(), to.len(), lease_token.map_or(0, <[u8]>::len)],
            CoreMutation::Complete { lease_token, .. }
            | CoreMutation::Fail { lease_token, .. }
            | CoreMutation::ExtendLease { lease_token, .. } => [lease_token.len(), 0, 0],
            CoreMutation::Retry {
                lease_token, retry, ..
            } => [
                lease_token.len(),
                retry
                    .and_then(|value| value.exhausted_to.as_ref())
                    .map_or(0, Vec::len),
                0,
            ],
            CoreMutation::Cancel {
                lease_token,
                expected_state,
                ..
            } => [
                lease_token.map_or(0, <[u8]>::len),
                expected_state.map_or(0, <[u8]>::len),
                0,
            ],
        };
        for width in widths {
            bytes = add(bytes, width)?;
        }
        // Covers requested copies and the existing identity/token conversion
        // liability conservatively; native allocator capacity is not certified.
        let liability = mul(bytes, 3)?;
        self.retained = add(self.retained, liability)?;
        self.observe(0)?;
        self.spend(add(liability, 512)?)
    }

    pub(super) fn value_limits(&self) -> Result<OriginalValueEncodeLimits, E> {
        let remaining_work = self
            .limits
            .max_work_units
            .checked_sub(self.work)
            .ok_or(E::Limit)?;
        let remaining_temp = self
            .limits
            .max_temporary_bytes
            .checked_sub(add(self.fixed, self.retained)?)
            .ok_or(E::Limit)?;
        if remaining_work == 0 || remaining_temp == 0 {
            return Err(E::Limit);
        }
        Ok(OriginalValueEncodeLimits {
            max_encoded_bytes: self
                .limits
                .values
                .max_encoded_bytes
                .min(self.limits.admission.inline_bytes()),
            max_work_units: self.limits.values.max_work_units.min(remaining_work),
            max_temporary_bytes: self.limits.values.max_temporary_bytes.min(remaining_temp),
            ..self.limits.values
        })
    }

    pub(super) fn value_done(&mut self, value: &PreparedOriginalFlowValue) -> Result<(), E> {
        self.spend(value.work_units())?;
        self.observe(value.temporary_bytes())?;
        self.retained = add(self.retained, value.as_stored_bytes().len())?;
        self.observe(0)
    }

    pub(super) fn copy(&self, bytes: &[u8]) -> Result<Vec<u8>, E> {
        // Only materialization after metadata() uses this admitted copy path.
        if bytes.len() > self.retained {
            return Err(E::Limit);
        }
        let mut output = Vec::new();
        output
            .try_reserve_exact(bytes.len())
            .map_err(E::ReserveFailed)?;
        output.extend_from_slice(bytes);
        Ok(output)
    }
}

pub(super) fn add(left: usize, right: usize) -> Result<usize, E> {
    left.checked_add(right).ok_or(E::Limit)
}

pub(super) fn mul(left: usize, right: usize) -> Result<usize, E> {
    left.checked_mul(right).ok_or(E::Limit)
}

fn fixed_bytes() -> Result<usize, E> {
    let parts = [
        (size_of::<L>(), 2),
        (size_of::<LogicalMutationInput<'static>>(), 2),
        (size_of::<LogicalMutationFieldValue<'static>>(), 2),
        (size_of::<[(&str, MutationFieldValue<'static>); 8]>(), 2),
        (size_of::<NormalizedMutation<'static>>(), 2),
        (size_of::<[Option<PreparedOriginalFlowValue>; 2]>(), 2),
        (size_of::<Budget>(), 1),
        (size_of::<PreparedLogicalMutation>(), 1),
        (size_of::<Result<PreparedLogicalMutation, E>>(), 1),
        (size_of::<E>(), 2),
        (size_of::<[u8; 16]>(), 1),
        (size_of::<[OriginalValueNode; 1]>(), 1),
        (size_of::<usize>(), 16),
    ];
    parts
        .into_iter()
        .try_fold(0, |total, (size, count)| add(total, mul(size, count)?))
}
