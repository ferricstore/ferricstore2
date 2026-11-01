//! Continuous logical work and prospectively admitted owner growth.

use std::mem::size_of;

use crate::value::original_flow::{OriginalFlowValueError as E, OriginalValueLimits as L};

pub(in crate::value::original_flow) struct Budget {
    pub(in crate::value::original_flow) limits: L,
    work: usize,
    auxiliary: usize,
    maximum_auxiliary: usize,
}

impl Budget {
    pub(in crate::value::original_flow) const fn work_units(&self) -> usize {
        self.work
    }
    pub(in crate::value::original_flow) fn new(
        limits: L,
        input: (usize, usize),
    ) -> Result<Self, E> {
        if [
            limits.max_encoded_bytes,
            limits.max_nodes,
            limits.max_edges,
            limits.max_depth,
            limits.max_work_units,
        ]
        .contains(&0)
        {
            return Err(E::InvalidLimits);
        }
        if input.0 > limits.max_encoded_bytes || input.1 > limits.max_encoded_bytes {
            return Err(E::Limit);
        }
        Ok(Self {
            limits,
            work: 0,
            auxiliary: 0,
            maximum_auxiliary: usize::MAX,
        })
    }

    pub(in crate::value::original_flow) fn bounded(
        limits: L,
        input: (usize, usize),
        maximum: usize,
    ) -> Result<Self, E> {
        if maximum == 0 {
            return Err(E::InvalidLimits);
        }
        let mut budget = Self::new(limits, input)?;
        budget.maximum_auxiliary = maximum;
        Ok(budget)
    }
    pub(in crate::value::original_flow) const fn auxiliary_bytes(&self) -> usize {
        self.auxiliary
    }
    fn auxiliary(&mut self, bytes: usize) -> Result<(), E> {
        let next = self.auxiliary.checked_add(bytes).ok_or(E::Limit)?;
        if next > self.maximum_auxiliary {
            return Err(E::Limit);
        }
        self.auxiliary = next;
        Ok(())
    }

    pub(in crate::value::original_flow) fn spend(&mut self, units: usize) -> Result<(), E> {
        let next = self.work.checked_add(units).ok_or(E::Limit)?;
        if next > self.limits.max_work_units {
            return Err(E::Limit);
        }
        self.work = next;
        Ok(())
    }

    pub(in crate::value::original_flow) fn require_nodes(
        &self,
        current: usize,
        additional: usize,
    ) -> Result<usize, E> {
        let target = current.checked_add(additional).ok_or(E::Limit)?;
        if target > self.limits.max_nodes {
            return Err(E::Limit);
        }
        Ok(target)
    }

    pub(in crate::value::original_flow) fn require_edges(
        &self,
        current: usize,
        additional: usize,
    ) -> Result<usize, E> {
        let target = current.checked_add(additional).ok_or(E::Limit)?;
        if target > self.limits.max_edges {
            return Err(E::Limit);
        }
        Ok(target)
    }

    pub(in crate::value::original_flow) fn require_depth(&self, depth: usize) -> Result<(), E> {
        if depth == 0 || depth > self.limits.max_depth {
            return Err(E::Limit);
        }
        Ok(())
    }
}

pub(in crate::value::original_flow) fn reserve_exact<T>(
    owner: &mut Vec<T>,
    target: usize,
    budget: &mut Budget,
) -> Result<(), E> {
    if target <= owner.capacity() {
        return Ok(());
    }
    let requested = target.checked_mul(size_of::<T>()).ok_or(E::Limit)?;
    let relocation = owner
        .capacity()
        .checked_mul(size_of::<T>())
        .ok_or(E::Limit)?;
    budget.auxiliary(requested.checked_add(relocation).ok_or(E::Limit)?)?;
    budget.spend(
        requested
            .checked_add(relocation)
            .and_then(|n| n.checked_add(1))
            .ok_or(E::Limit)?,
    )?;
    let additional = target.checked_sub(owner.len()).ok_or(E::Limit)?;
    owner
        .try_reserve_exact(additional)
        .map_err(E::ReserveFailed)
}

pub(in crate::value::original_flow) fn grow_scratch<T>(
    owner: &mut Vec<T>,
    needed: usize,
    ceiling: usize,
    budget: &mut Budget,
) -> Result<(), E> {
    if needed > ceiling {
        return Err(E::Limit);
    }
    if needed <= owner.capacity() {
        return Ok(());
    }
    let doubled = owner.capacity().checked_mul(2).ok_or(E::Limit)?;
    let target = ceiling.min(needed.max(8.max(doubled)));
    reserve_exact(owner, target, budget)
}
