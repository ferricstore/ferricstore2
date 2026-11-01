//! One continuous account for schema, peers, framing and checksum.
use super::model::{OriginalQueryRecordError as E, OriginalQueryRecordLimits as L};
use wf_domain::metadata::scope::{OriginalScopeError, OriginalScopeLimits};

pub(super) struct Budget {
    pub(super) limits: L,
    pub(super) work: usize,
    pub(super) peak: usize,
    live: usize,
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
        let mut b = Self {
            limits,
            work: 0,
            peak: 0,
            live: 0,
        };
        b.spend(16384)?;
        b.admit(16384)?;
        Ok(b)
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
    pub(super) const fn live(&self) -> usize {
        self.live
    }
    pub(super) fn release_to(&mut self, before: usize) {
        self.live = before;
    }
    pub(super) fn reserve<T>(&mut self, count: usize) -> Result<Vec<T>, E> {
        let bytes = count
            .checked_mul(std::mem::size_of::<T>())
            .ok_or(E::Limit)?;
        self.spend(add(bytes, 1)?)?;
        self.admit(bytes)?;
        let mut value = Vec::new();
        value.try_reserve_exact(count).map_err(E::ReserveFailed)?;
        Ok(value)
    }
    pub(super) fn grow<T>(&mut self, stack: &mut Vec<T>) -> Result<(), E> {
        if stack.len() < stack.capacity() {
            return Ok(());
        }
        let count = stack.capacity().max(1);
        let capacity = add(count, stack.capacity())?;
        let bytes = capacity
            .checked_mul(std::mem::size_of::<T>())
            .ok_or(E::Limit)?;
        self.spend(bytes)?;
        self.admit(bytes)?;
        stack.try_reserve_exact(count).map_err(E::ReserveFailed)
    }
    pub(super) fn output(&self, length: usize) -> Result<(), E> {
        if length > self.limits.maximum_output_bytes {
            return Err(E::Limit);
        }
        Ok(())
    }
    pub(super) fn peer_limits(&self) -> Result<OriginalScopeLimits, E> {
        Ok(OriginalScopeLimits {
            maximum_output_bytes: usize::MAX,
            maximum_work_units: self
                .limits
                .maximum_work_units
                .checked_sub(self.work)
                .filter(|v| *v > 0)
                .ok_or(E::Limit)?,
            maximum_temporary_bytes: self
                .limits
                .maximum_temporary_bytes
                .checked_sub(self.live)
                .filter(|v| *v > 0)
                .ok_or(E::Limit)?,
        })
    }
    pub(super) fn peer(&mut self, work: usize, temporary: usize) -> Result<(), E> {
        self.spend(work)?;
        let peak = add(self.live, temporary)?;
        if peak > self.limits.maximum_temporary_bytes {
            return Err(E::Limit);
        }
        self.peak = self.peak.max(peak);
        Ok(())
    }
}
pub(super) fn scope(error: OriginalScopeError) -> E {
    match error {
        OriginalScopeError::InvalidSystemMetadata => E::InvalidRecord,
        OriginalScopeError::Limit | OriginalScopeError::InvalidLimits => E::Limit,
        other => E::Scope(other),
    }
}
