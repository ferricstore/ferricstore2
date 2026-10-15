//! Same strict reader owners retained for one source merge, with lazy children.

use super::{
    Error, OriginalEtfSafety, OriginalPolicyEntry, PolicySchemaCharge, TermPosition, TermSpan,
    VirtualLocalFun, charge_schema, overflow, pay,
};

mod value;
pub(in crate::policy::complete) use value::OriginalPolicyCurrentCursor;

pub(in crate::policy::complete) struct OriginalPolicyCurrentIndex<'raw, 'context> {
    pub(super) raw: &'raw [u8],
    pub(super) spans: Vec<TermSpan>,
    pub(super) keys: Vec<u32>,
    pub(super) virtual_funs: Vec<VirtualLocalFun>,
    pub(super) safety: OriginalEtfSafety<'context>,
    pub(super) entry: OriginalPolicyEntry<'raw>,
    pub(super) catalog: Vec<u32>,
    pub(super) policy: u32,
    pub(super) depth: usize,
}

impl<'raw, 'context> OriginalPolicyCurrentIndex<'raw, 'context> {
    pub(in crate::policy::complete) const fn entry(&self) -> &OriginalPolicyEntry<'raw> {
        &self.entry
    }

    pub(in crate::policy::complete) fn count(&self) -> usize {
        self.spans.len()
    }

    pub(in crate::policy::complete) const fn depth_bound(&self) -> usize {
        self.depth
    }

    pub(in crate::policy::complete) fn owner_bound(&self) -> Result<usize, Error> {
        self.spans
            .len()
            .checked_add(self.keys.len())
            .and_then(|n| n.checked_add(self.virtual_funs.len()))
            .and_then(|n| n.checked_mul(64))
            .ok_or_else(overflow)
    }

    pub(in crate::policy::complete) fn catalog_bound(&self) -> Result<usize, Error> {
        self.count()
            .checked_mul(8)
            .and_then(|n| n.checked_add(128))
            .ok_or_else(overflow)
    }

    pub(in crate::policy::complete) fn cursor(
        &self,
    ) -> OriginalPolicyCurrentCursor<'_, 'raw, 'context> {
        OriginalPolicyCurrentCursor::new(self, TermPosition::Encoded(self.policy))
    }

    pub(in crate::policy::complete) fn at(
        &self,
        position: TermPosition,
    ) -> OriginalPolicyCurrentCursor<'_, 'raw, 'context> {
        OriginalPolicyCurrentCursor::new(self, position)
    }

    pub(in crate::policy::complete) fn compare_positions(
        &self,
        left: TermPosition,
        right: TermPosition,
        charge: &mut PolicySchemaCharge,
    ) -> Result<std::cmp::Ordering, Error> {
        super::identity::compare(
            (
                self.raw,
                &self.spans,
                &self.keys,
                &self.safety,
                &self.virtual_funs,
            ),
            left,
            right,
            charge,
        )
    }

    pub(in crate::policy::complete) fn build_catalog(
        &mut self,
        live: usize,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        let bytes = self.catalog_bound()?;
        let peak = live.checked_add(bytes).ok_or_else(overflow)?;
        charge_schema(charge, 0, u64::try_from(peak).map_err(|_| overflow())?)
            .map_err(Error::Schema)?;
        pay(charge, u64::try_from(bytes).map_err(|_| overflow())?)?;
        let capacity = self.count().checked_mul(2).ok_or_else(overflow)?;
        self.catalog
            .try_reserve_exact(capacity)
            .map_err(|_| Error::AllocationFailure)?;
        pay(
            charge,
            u64::try_from(self.count().checked_mul(4).ok_or_else(overflow)?)
                .map_err(|_| overflow())?,
        )?;
        self.catalog.resize(self.count(), u32::MAX);
        for index in (0..self.count()).rev() {
            pay(charge, 128)?;
            let span = *self.spans.get(index).ok_or(Error::Malformed)?;
            if span.tag() == 108 && span.datum() == 0 {
                let tail = usize::try_from(span.canonical).map_err(|_| overflow())?;
                let target = self.resolve(tail).ok_or(Error::Malformed)?;
                self.header(
                    index,
                    u32::try_from(target).map_err(|_| overflow())?,
                    charge,
                )?;
            } else if matches!(span.tag(), 104 | 105 | 108) {
                let count = if span.tag() == 108 {
                    span.datum()
                } else {
                    span.keys
                };
                let start = u32::try_from(self.catalog.len()).map_err(|_| overflow())?;
                let mut child = u32::try_from(index + 1).map_err(|_| overflow())?;
                for _ in 0..count {
                    pay(charge, 128)?;
                    let next = self
                        .spans
                        .get(usize::try_from(child).map_err(|_| overflow())?)
                        .ok_or(Error::Malformed)?
                        .next;
                    if self.catalog.len() == capacity {
                        return Err(Error::ItemLimit);
                    }
                    self.catalog.push(child);
                    child = next;
                }
                self.header(index, start, charge)?;
            }
        }
        Ok(())
    }

    fn header(
        &mut self,
        index: usize,
        value: u32,
        charge: &mut PolicySchemaCharge,
    ) -> Result<(), Error> {
        pay(charge, 128)?;
        *self.catalog.get_mut(index).ok_or(Error::Malformed)? = value;
        Ok(())
    }

    fn resolve(&self, index: usize) -> Option<usize> {
        let span = self.spans.get(index)?;
        if span.tag() == 108 && span.datum() == 0 {
            usize::try_from(*self.catalog.get(index)?)
                .ok()
                .filter(|&n| n != usize::MAX)
        } else {
            Some(index)
        }
    }
}
