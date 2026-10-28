//! Prospective named control, continuous work and retained/scratch ownership.

use std::mem::size_of;

use super::{OriginalHistoryMetadataError as E, OriginalHistoryMetadataLimits as L};
use crate::value::{OriginalValueInput, OriginalValueNode};

pub(super) struct Budget {
    limits: L,
    pub(super) work: usize,
    live: usize,
    pub(super) peak: usize,
    output: usize,
}

impl Budget {
    pub(super) fn new(input: OriginalValueInput<'_>, limits: L) -> Result<Self, E> {
        if [
            limits.max_input_bytes,
            limits.max_pairs,
            limits.max_output_bytes,
            limits.max_work_units,
            limits.max_temporary_bytes,
        ]
        .contains(&0)
        {
            return Err(E::InvalidLimits);
        }
        let raw = add(
            input.bytes.len(),
            add(
                mul(input.nodes.len(), size_of::<OriginalValueNode>())?,
                mul(input.edges.len(), size_of::<usize>())?,
            )?,
        )?;
        if raw > limits.max_input_bytes {
            return Err(E::Limit);
        }
        let mut charge = Self {
            limits,
            work: 0,
            live: 0,
            peak: 0,
            output: 0,
        };
        charge.spend(add(1_024, mul(raw, 16)?)?)?;
        // Named fixed formatter/control cohort, not native stack or allocator RSS.
        charge.grow(add(2_048, size_of::<Self>())?)?;
        Ok(charge)
    }

    pub(super) fn spend(&mut self, amount: usize) -> Result<(), E> {
        self.work = add(self.work, amount)?;
        if self.work > self.limits.max_work_units {
            Err(E::Limit)
        } else {
            Ok(())
        }
    }

    pub(super) fn output(&mut self, amount: usize) -> Result<(), E> {
        self.output = add(self.output, amount)?;
        if self.output > self.limits.max_output_bytes {
            return Err(E::Limit);
        }
        self.spend(add(16, mul(amount, 4)?)?)
    }

    pub(super) fn reserve<T>(&mut self, count: usize) -> Result<Vec<T>, E> {
        let requested = mul(count, size_of::<T>())?;
        self.spend(add(32, requested)?)?;
        self.grow(requested)?;
        let mut output = Vec::new();
        output.try_reserve_exact(count).map_err(E::ReserveFailed)?;
        self.grow(mul(
            output.capacity().saturating_sub(count),
            size_of::<T>(),
        )?)?;
        Ok(output)
    }

    pub(super) fn release(&mut self, amount: usize) -> Result<(), E> {
        self.live = self.live.checked_sub(amount).ok_or(E::Limit)?;
        Ok(())
    }

    fn grow(&mut self, amount: usize) -> Result<(), E> {
        self.live = add(self.live, amount)?;
        if self.live > self.limits.max_temporary_bytes {
            return Err(E::Limit);
        }
        self.peak = self.peak.max(self.live);
        Ok(())
    }
}

pub(super) fn add(a: usize, b: usize) -> Result<usize, E> {
    a.checked_add(b).ok_or(E::Limit)
}

pub(super) fn mul(a: usize, b: usize) -> Result<usize, E> {
    a.checked_mul(b).ok_or(E::Limit)
}
