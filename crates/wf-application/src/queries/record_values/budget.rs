//! Whole-call logical account with nonrefundable reached-stage ceilings.
use super::super::StrongWorkflowRecordResult;
use super::{
    StrongWorkflowRecordValueError as E, StrongWorkflowRecordValueLimits as L,
    StrongWorkflowRecordValueResult,
};
use std::mem::size_of;
use wf_domain::history::values::{HistoryValue, HistoryValueWrite};
pub(super) struct Budget {
    pub(super) limits: L,
    pub(super) work: usize,
    pub(super) live: usize,
    pub(super) peak: usize,
}
pub(super) fn add(a: usize, b: usize) -> Result<usize, E> {
    a.checked_add(b).ok_or(E::Limit)
}
pub(super) fn product(a: usize, b: usize) -> Result<usize, E> {
    a.checked_mul(b).ok_or(E::Limit)
}
impl Budget {
    pub(super) fn new(limits: &L) -> Result<Self, E> {
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
            limits: *limits,
            work: 0,
            live: 0,
            peak: 0,
        };
        b.spend(16384)?;
        b.admit(add(16384, size_of::<StrongWorkflowRecordValueResult>())?)?;
        Ok(b)
    }
    pub(super) fn spend(&mut self, units: usize) -> Result<(), E> {
        let next = add(self.work, units)?;
        if next > self.limits.maximum_work_units {
            return Err(E::Limit);
        }
        self.work = next;
        Ok(())
    }
    pub(super) fn admit(&mut self, bytes: usize) -> Result<(), E> {
        let next = add(self.live, bytes)?;
        if next > self.limits.maximum_temporary_bytes {
            return Err(E::Limit);
        }
        self.live = next;
        self.peak = self.peak.max(next);
        Ok(())
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
    pub(super) fn remaining(&self) -> Result<usize, E> {
        self.limits
            .maximum_work_units
            .checked_sub(self.work)
            .filter(|n| *n > 0)
            .ok_or(E::Limit)
    }
    pub(super) fn spare(&self) -> Result<usize, E> {
        self.limits
            .maximum_temporary_bytes
            .checked_sub(self.live)
            .filter(|n| *n > 0)
            .ok_or(E::Limit)
    }
    pub(super) fn fund(&mut self, work: usize, temporary: usize) -> Result<(), E> {
        if work == 0 || temporary == 0 {
            return Err(E::InvalidLimits);
        }
        self.spend(work)?;
        self.admit(temporary)
    }
    pub(super) fn finish(
        mut self,
        record: StrongWorkflowRecordResult,
        writes: Vec<HistoryValueWrite>,
    ) -> Result<StrongWorkflowRecordValueResult, E> {
        let spine = product(writes.capacity(), size_of::<HistoryValueWrite>())?;
        self.spend(add(128, product(writes.len(), 128)?)?)?;
        let mut owned = record
            .owned_bytes()
            .checked_sub(size_of::<StrongWorkflowRecordResult>())
            .and_then(|n| n.checked_add(size_of::<StrongWorkflowRecordValueResult>()))
            .ok_or(E::Limit)?;
        owned = add(owned, spine)?;
        for write in &writes {
            if let HistoryValue::Decoded(document) = &write.value {
                owned = add(owned, document.owned_buffer_bytes().ok_or(E::Limit)?)?;
            }
        }
        if owned > self.limits.maximum_output_bytes {
            return Err(E::Limit);
        }
        Ok(StrongWorkflowRecordValueResult {
            record,
            writes,
            work: self.work,
            temporary: self.peak,
            owned,
        })
    }
}
