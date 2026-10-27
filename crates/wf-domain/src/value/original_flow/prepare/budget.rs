//! Continuous work and prospectively requested ownership, not allocator/RSS use.

use std::mem::size_of;

use crate::value::original_flow::prepare::{
    OriginalValueEncodeError as E, OriginalValueEncodeLimits as L,
};

pub(in crate::value::original_flow::prepare) struct Budget {
    pub(in crate::value::original_flow::prepare) limits: L,
    pub(in crate::value::original_flow::prepare) work: usize,
    pub(in crate::value::original_flow::prepare) peak: usize,
    live: usize,
}

impl Budget {
    pub(in crate::value::original_flow::prepare) fn new(limits: L) -> Result<Self, E> {
        if [
            limits.max_input_bytes,
            limits.max_nodes,
            limits.max_edges,
            limits.max_depth,
            limits.max_encoded_bytes,
            limits.max_work_units,
            limits.max_temporary_bytes,
        ]
        .contains(&0)
        {
            return Err(E::InvalidLimits);
        }
        Ok(Self {
            limits,
            work: 0,
            peak: 0,
            live: 0,
        })
    }

    pub(in crate::value::original_flow::prepare) fn spend(
        &mut self,
        units: usize,
    ) -> Result<(), E> {
        let next = add(self.work, units)?;
        if next > self.limits.max_work_units {
            return Err(E::Limit);
        }
        self.work = next;
        Ok(())
    }

    pub(in crate::value::original_flow::prepare) fn admit(
        &mut self,
        bytes: usize,
    ) -> Result<(), E> {
        let next = add(self.live, bytes)?;
        if next > self.limits.max_temporary_bytes {
            return Err(E::Limit);
        }
        self.live = next;
        self.peak = self.peak.max(next);
        Ok(())
    }

    pub(in crate::value::original_flow::prepare) fn release(
        &mut self,
        bytes: usize,
    ) -> Result<(), E> {
        self.live = self.live.checked_sub(bytes).ok_or(E::Limit)?;
        Ok(())
    }

    pub(in crate::value::original_flow::prepare) fn reserve<T>(
        &mut self,
        count: usize,
    ) -> Result<Vec<T>, E> {
        let bytes = mul(count, size_of::<T>())?;
        self.spend(add(bytes, 1)?)?;
        self.admit(bytes)?;
        let mut result = Vec::new();
        result.try_reserve_exact(count).map_err(E::ReserveFailed)?;
        Ok(result)
    }

    pub(in crate::value::original_flow::prepare) fn encoded(
        &self,
        size: usize,
    ) -> Result<usize, E> {
        if size > self.limits.max_encoded_bytes {
            return Err(E::Limit);
        }
        Ok(size)
    }
}

pub(in crate::value::original_flow::prepare) fn add(a: usize, b: usize) -> Result<usize, E> {
    a.checked_add(b).ok_or(E::Limit)
}

pub(in crate::value::original_flow::prepare) fn mul(a: usize, b: usize) -> Result<usize, E> {
    a.checked_mul(b).ok_or(E::Limit)
}

pub(in crate::value::original_flow::prepare) fn frames(nodes: usize) -> Result<usize, E> {
    add(mul(nodes, 2)?, 4)
}

pub(in crate::value::original_flow::prepare) fn wire_length(length: usize) -> Result<u32, E> {
    u32::try_from(length).map_err(|_| E::Limit)
}
