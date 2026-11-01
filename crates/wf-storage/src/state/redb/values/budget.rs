//! Continuous logical value work and prospective owner/output admission.

use std::mem::size_of;
use wf_ports::state::StateTransactionError as Error;

pub(crate) struct ValueCharge {
    maximum_rows: usize,
    maximum_bytes: usize,
    maximum_work: usize,
    maximum_temporary: usize,
    rows: usize,
    bytes: usize,
    work: usize,
    peak: usize,
}

impl ValueCharge {
    pub(crate) fn new(
        maximum_rows: usize,
        maximum_bytes: usize,
        maximum_work: usize,
        maximum_temporary: usize,
    ) -> Result<Self, Error> {
        if [maximum_rows, maximum_bytes, maximum_work, maximum_temporary].contains(&0) {
            return Err(Error::Limit);
        }
        let mut charge = Self {
            maximum_rows,
            maximum_bytes,
            maximum_work,
            maximum_temporary,
            rows: 0,
            bytes: 0,
            work: 0,
            peak: 0,
        };
        charge.helper()?;
        charge.temporary(size_of::<Self>())?;
        Ok(charge)
    }

    pub(crate) fn helper(&mut self) -> Result<(), Error> {
        self.work(65_536)
    }

    pub(crate) fn work(&mut self, units: usize) -> Result<(), Error> {
        let next = total(self.work, [units])?;
        if next > self.maximum_work {
            return Err(Error::Limit);
        }
        self.work = next;
        Ok(())
    }

    pub(crate) fn candidate(&mut self) -> Result<(), Error> {
        self.work(128)
    }

    pub(crate) fn compare(&mut self, left: usize, right: usize) -> Result<(), Error> {
        self.work(total(128, [left, right])?)
    }

    pub(crate) fn output(&mut self, rows: usize, bytes: usize) -> Result<(), Error> {
        let next_rows = total(self.rows, [rows])?;
        let next_bytes = total(self.bytes, [bytes])?;
        if next_rows > self.maximum_rows || next_bytes > self.maximum_bytes {
            return Err(Error::Limit);
        }
        self.rows = next_rows;
        self.bytes = next_bytes;
        Ok(())
    }

    pub(crate) fn stored(&mut self, bytes: usize) -> Result<(), Error> {
        let next = total(self.bytes, [bytes])?;
        if next > self.maximum_bytes {
            return Err(Error::Limit);
        }
        self.bytes = next;
        Ok(())
    }

    pub(crate) fn temporary(&mut self, bytes: usize) -> Result<(), Error> {
        if bytes > self.maximum_temporary {
            return Err(Error::Limit);
        }
        self.peak = self.peak.max(bytes);
        Ok(())
    }

    pub(crate) fn reserve<T>(&mut self, count: usize, live: usize) -> Result<Vec<T>, Error> {
        self.candidate()?;
        self.temporary(total(live, [product(count, size_of::<T>())?])?)?;
        let mut result = Vec::new();
        result
            .try_reserve_exact(count)
            .map_err(|_| Error::Allocation)?;
        Ok(result)
    }

    pub(crate) fn remaining_work(&self) -> usize {
        self.maximum_work - self.work
    }

    pub(crate) fn peak(&self) -> usize {
        self.peak
    }

    pub(crate) fn usage(&self) -> (usize, usize, usize) {
        (self.rows, self.bytes, self.work)
    }
}

pub(crate) fn total(base: usize, parts: impl IntoIterator<Item = usize>) -> Result<usize, Error> {
    parts.into_iter().try_fold(base, |sum, value| {
        sum.checked_add(value).ok_or(Error::Limit)
    })
}

pub(crate) fn product(count: usize, size: usize) -> Result<usize, Error> {
    count.checked_mul(size).ok_or(Error::Limit)
}
