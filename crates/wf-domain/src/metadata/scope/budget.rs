//! Continuous accounting across schema, shared ETF and partition preparation.
use super::{OriginalScopeError as E, OriginalScopeLimits as L, OriginalScopeResult};
use crate::value::{OriginalValueEncodeError, OriginalValueEncodeLimits};
use std::mem::size_of;

pub(super) struct Budget {
    pub(super) limits: L,
    pub(super) work: usize,
    live: usize,
    peak: usize,
}

pub(super) fn add(a: usize, b: usize) -> Result<usize, E> {
    a.checked_add(b).ok_or(E::Limit)
}

impl Budget {
    pub(super) fn new(limits: L) -> Result<Self, E> {
        if [
            limits.maximum_output_bytes,
            limits.maximum_work_units,
            limits.maximum_temporary_bytes,
        ]
        .contains(&0)
        {
            return Err(E::InvalidLimits);
        }
        let mut budget = Self {
            limits,
            work: 0,
            live: 0,
            peak: 0,
        };
        budget.spend(4096)?;
        budget.admit(4096)?;
        Ok(budget)
    }
    pub(super) fn spend(&mut self, units: usize) -> Result<(), E> {
        self.work = add(self.work, units)?;
        if self.work > self.limits.maximum_work_units {
            return Err(E::Limit);
        }
        Ok(())
    }
    pub(super) fn admit(&mut self, bytes: usize) -> Result<(), E> {
        self.live = add(self.live, bytes)?;
        if self.live > self.limits.maximum_temporary_bytes {
            return Err(E::Limit);
        }
        self.peak = self.peak.max(self.live);
        Ok(())
    }
    pub(super) fn reserve<T>(&mut self, count: usize) -> Result<Vec<T>, E> {
        let bytes = count.checked_mul(size_of::<T>()).ok_or(E::Limit)?;
        self.spend(add(bytes, 1)?)?;
        self.admit(bytes)?;
        let mut output = Vec::new();
        output.try_reserve_exact(count).map_err(E::ReserveFailed)?;
        Ok(output)
    }
    pub(super) fn output(&self, length: usize) -> Result<(), E> {
        if length > self.limits.maximum_output_bytes {
            return Err(E::Limit);
        }
        Ok(())
    }
    pub(super) const fn live(&self) -> usize {
        self.live
    }
    pub(super) fn retain(&mut self, before: usize, bytes: usize) -> Result<(), E> {
        self.live = before;
        self.admit(bytes)
    }
    pub(super) fn codec_limits(&self) -> Result<OriginalValueEncodeLimits, E> {
        Ok(OriginalValueEncodeLimits {
            max_input_bytes: usize::MAX,
            max_nodes: 116,
            max_edges: 114,
            max_depth: 5,
            max_encoded_bytes: usize::MAX,
            max_work_units: self
                .limits
                .maximum_work_units
                .checked_sub(self.work)
                .filter(|n| *n > 0)
                .ok_or(E::Limit)?,
            max_temporary_bytes: self
                .limits
                .maximum_temporary_bytes
                .checked_sub(self.live)
                .filter(|n| *n > 0)
                .ok_or(E::Limit)?,
        })
    }
    pub(super) fn codec_used(&mut self, work: usize, temporary: usize) -> Result<(), E> {
        self.spend(work)?;
        let peak = add(self.live, temporary)?;
        if peak > self.limits.maximum_temporary_bytes {
            return Err(E::Limit);
        }
        self.peak = self.peak.max(peak);
        Ok(())
    }
    pub(super) fn finish<T>(self, value: T) -> OriginalScopeResult<T> {
        OriginalScopeResult {
            value,
            work_units: self.work,
            temporary_bytes: self.peak,
        }
    }
}

pub(super) fn encoding(error: OriginalValueEncodeError) -> E {
    match error {
        OriginalValueEncodeError::InvalidInput => E::InvalidSystemMetadata,
        OriginalValueEncodeError::Limit | OriginalValueEncodeError::InvalidLimits => E::Limit,
        other => E::Encoding(other),
    }
}
