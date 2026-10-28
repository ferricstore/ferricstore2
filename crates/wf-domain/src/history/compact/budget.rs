//! Continuous logical work and prospective requested owners, without refunds.

use std::mem::size_of;

use super::{
    DecodedOriginalCompactHistory, OriginalCompactHistoryContext as Context,
    OriginalCompactHistoryError as E, OriginalCompactHistoryField,
    OriginalCompactHistoryLimits as Limits, OriginalCompactHistorySnapshot,
    PreparedOriginalCompactHistory,
    decode::Parsed,
    primitives::{Output, Reader},
};

pub(super) struct Budget {
    pub(super) limits: Limits,
    pub(super) work: usize,
    pub(super) peak: usize,
    live: usize,
    input: usize,
    output: usize,
}

impl Budget {
    pub(super) fn new(limits: Limits) -> Result<Self, E> {
        if [
            limits.max_input_bytes,
            limits.max_metadata_pairs,
            limits.max_encoded_bytes,
            limits.max_output_bytes,
            limits.max_work_units,
            limits.max_temporary_bytes,
        ]
        .contains(&0)
        {
            return Err(E::InvalidLimits);
        }
        let mut budget = Self {
            limits,
            work: 0,
            peak: 0,
            live: 0,
            input: 0,
            output: 0,
        };
        // Named control covers both facade/result lanes, parser/writer/renderer
        // descriptors and fixed 16/40-byte numeric scratch. The additional 1024
        // is a conservative logical helper cohort, not a native stack bound.
        let mut control = 1024;
        for size in [
            size_of::<Self>(),
            size_of::<Limits>(),
            size_of::<Context<'_>>(),
            size_of::<OriginalCompactHistorySnapshot<'_, '_>>(),
            size_of::<PreparedOriginalCompactHistory>(),
            size_of::<DecodedOriginalCompactHistory<'_>>(),
            size_of::<OriginalCompactHistoryField<'_>>(),
            size_of::<Result<PreparedOriginalCompactHistory, E>>(),
            size_of::<Result<DecodedOriginalCompactHistory<'_>, E>>(),
            size_of::<Parsed<'_>>(),
            size_of::<Reader<'_>>(),
            size_of::<Output<'_>>(),
        ] {
            control = add(control, size)?;
        }
        budget.spend(control)?;
        budget.own(control)?;
        Ok(budget)
    }

    pub(super) fn spend(&mut self, amount: usize) -> Result<(), E> {
        self.work = add(self.work, amount)?;
        if self.work > self.limits.max_work_units {
            return Err(E::Limit);
        }
        Ok(())
    }

    fn own(&mut self, amount: usize) -> Result<(), E> {
        self.live = add(self.live, amount)?;
        self.peak = self.peak.max(self.live);
        if self.live > self.limits.max_temporary_bytes {
            return Err(E::Limit);
        }
        Ok(())
    }

    pub(super) fn input(&mut self, amount: usize) -> Result<(), E> {
        self.spend(add(amount, 1)?)?;
        self.input = add(self.input, amount)?;
        if self.input > self.limits.max_input_bytes {
            return Err(E::Limit);
        }
        Ok(())
    }

    pub(super) fn output(&mut self, amount: usize) -> Result<(), E> {
        self.spend(add(amount, 1)?)?;
        self.output = add(self.output, amount)?;
        if self.output > self.limits.max_output_bytes {
            return Err(E::Limit);
        }
        Ok(())
    }

    pub(super) fn encoded(&self, amount: usize) -> Result<(), E> {
        if amount > self.limits.max_encoded_bytes {
            return Err(E::Limit);
        }
        Ok(())
    }

    pub(super) fn pairs(&mut self, amount: usize) -> Result<(), E> {
        self.spend(1)?;
        if amount > self.limits.max_metadata_pairs {
            return Err(E::Limit);
        }
        Ok(())
    }

    pub(super) fn reserve<T>(&mut self, count: usize) -> Result<Vec<T>, E> {
        let bytes = product(count, size_of::<T>())?;
        self.spend(add(bytes, 1)?)?;
        self.own(bytes)?;
        let mut values = Vec::new();
        values.try_reserve_exact(count).map_err(E::ReserveFailed)?;
        Ok(values)
    }

    pub(super) fn context(&mut self, context: Context<'_>) -> Result<(), E> {
        self.input(size_of::<Context<'_>>())?;
        for bytes in [
            context.id,
            context.workflow_type,
            context.parent_flow_id,
            context.root_flow_id,
            context.correlation_id,
        ]
        .into_iter()
        .flatten()
        {
            self.input(bytes.len())?;
        }
        Ok(())
    }
}

pub(super) fn add(left: usize, right: usize) -> Result<usize, E> {
    left.checked_add(right).ok_or(E::Limit)
}

pub(super) fn product(left: usize, right: usize) -> Result<usize, E> {
    left.checked_mul(right).ok_or(E::Limit)
}
