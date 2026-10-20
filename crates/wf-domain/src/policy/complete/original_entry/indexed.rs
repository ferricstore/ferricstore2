//! Same strict reader owners retained for one source merge, with lazy children.

use super::{
    Error, EtfMap, OriginalEtfSafety, OriginalPolicyEntry, PolicySchemaCharge, TermPosition,
    TermSpan, VirtualLocalFun, charge_schema, compound, identity, overflow, pay, source_result,
    visit,
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
    // Splice counts are local to one paid census/build; zero LIST aliases stay literal.
    pub(in crate::policy::complete) fn record_splice_map_len(
        &mut self,
        position: TermPosition,
        length: usize,
    ) -> Result<(), Error> {
        self.record_splice_len(position, length, 116)
    }

    pub(in crate::policy::complete) fn splice_map_len(
        &self,
        position: TermPosition,
    ) -> Result<usize, Error> {
        self.splice_len(position, 116)
    }

    pub(in crate::policy::complete) fn record_splice_keyword_len(
        &mut self,
        position: TermPosition,
        length: usize,
    ) -> Result<(), Error> {
        self.record_splice_len(position, length, 108)
    }

    pub(in crate::policy::complete) fn splice_keyword_len(
        &self,
        position: TermPosition,
    ) -> Result<usize, Error> {
        self.splice_len(position, 108)
    }

    fn splice_position(&self, position: TermPosition, tag: u8) -> Result<usize, Error> {
        let TermPosition::Encoded(id) = position else {
            return Err(Error::Malformed);
        };
        let span = self.spans.get(id as usize).ok_or(Error::Malformed)?;
        if span.tag() != tag || (tag == 108 && span.datum() == 0) {
            return Err(Error::Malformed);
        }
        Ok(id as usize)
    }

    fn record_splice_len(
        &mut self,
        position: TermPosition,
        length: usize,
        tag: u8,
    ) -> Result<(), Error> {
        if !matches!(position, TermPosition::Encoded(_)) {
            return Err(Error::Malformed);
        }
        let encoded = length
            .checked_add(1)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(overflow)?;
        let id = self.splice_position(position, tag)?;
        let span = &mut self.spans[id];
        if span.canonical != 0 && span.canonical != encoded {
            return Err(Error::Malformed);
        }
        span.canonical = encoded;
        Ok(())
    }

    fn splice_len(&self, position: TermPosition, tag: u8) -> Result<usize, Error> {
        let id = self.splice_position(position, tag)?;
        usize::try_from(
            self.spans[id]
                .canonical
                .checked_sub(1)
                .ok_or(Error::Malformed)?,
        )
        .map_err(|_| overflow())
    }

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

/// Decodes the original entry with the immutable portable server profile.
/// # Errors
/// Preserves malformed-envelope None, operational bounds and distinct profile refusal.
pub fn decode_original_complete_policy_entry_portable<'raw>(
    raw: &'raw [u8],
    limits: super::PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<OriginalPolicyEntry<'raw>>, Error> {
    Ok(decode_portable_retained(raw, 0, limits, charge)?.map(|index| index.entry))
}

pub(in crate::policy::complete) fn decode_portable_retained<'raw>(
    raw: &'raw [u8],
    live: usize,
    limits: super::PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<OriginalPolicyCurrentIndex<'raw, 'static>>, Error> {
    super::decode_indexed(
        raw,
        &super::canonical_original_etf_safety(),
        (263_168, 263_168, true, 263_168),
        live,
        limits,
        charge,
    )
}

pub(in crate::policy::complete) fn decode_transient_portable_retained<'raw>(
    raw: &'raw [u8],
    maximum_encoded_bytes: usize,
    maximum_spans: usize,
    live: usize,
    limits: super::PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<OriginalPolicyCurrentIndex<'raw, 'static>>, Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(super::PolicySchemaError::ProfileMismatch));
    }
    if raw.len() > maximum_encoded_bytes || u32::try_from(raw.len()).is_err() {
        return Err(Error::ByteLimit);
    }
    let maximum_spans = maximum_spans.min(u32::MAX as usize);
    super::decode_indexed(
        raw,
        &super::canonical_original_etf_safety(),
        (maximum_spans, maximum_spans, true, maximum_encoded_bytes),
        live,
        limits,
        charge,
    )
}

pub(super) fn finish_decoded<'encoded, 'context>(
    raw: &'encoded [u8],
    safety: &OriginalEtfSafety<'context>,
    map_limit: usize,
    owners: (Vec<TermSpan>, Vec<u32>, Vec<VirtualLocalFun>),
    depth: usize,
    limits: super::PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<OriginalPolicyCurrentIndex<'encoded, 'context>>, Error> {
    let (mut spans, mut keys, virtual_funs) = owners;
    if source_result(identity::maps(
        raw,
        &mut spans,
        &mut keys,
        safety,
        &virtual_funs,
        charge,
    ))?
    .is_none()
    {
        return Ok(None);
    }
    let Some((generation, policy)) = compound::envelope(raw, &spans, safety, charge)? else {
        return Ok(None);
    };
    let map = EtfMap {
        raw,
        spans: &spans,
        safety: *safety,
        root: compound::resolve(&spans, policy),
    };
    if if map_limit == 4_096 {
        visit::old_keys(map, limits, charge)?
    } else {
        visit::old_keys_with_node_limit(map, limits, spans.len().min(map_limit), charge)?
    } {
        return Ok(None);
    }
    let workflow_type = visit::workflow_type(map, charge)?;
    pay(charge, 128)?;
    let policy_span = spans[usize::try_from(policy).map_err(|_| overflow())?];
    let entry = OriginalPolicyEntry {
        encoded: raw,
        policy_range: (
            usize::try_from(policy_span.start).map_err(|_| overflow())?,
            usize::try_from(policy_span.end).map_err(|_| overflow())?,
        ),
        generation,
        workflow_type,
    };
    Ok(Some(OriginalPolicyCurrentIndex {
        raw,
        spans,
        keys,
        virtual_funs,
        safety: *safety,
        entry,
        catalog: Vec::new(),
        policy,
        depth: depth.saturating_sub(1),
    }))
}

pub(super) fn reserved<T>(
    count: usize,
    width: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<T>, Error> {
    let mut owner = Vec::new();
    reserve(&mut owner, count, width, charge)?;
    Ok(owner)
}

fn reserve<T>(
    vector: &mut Vec<T>,
    count: usize,
    width: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let fee = u64::try_from(count)
        .map_err(|_| overflow())?
        .checked_mul(width)
        .and_then(|n| n.checked_add(128))
        .ok_or_else(overflow)?;
    pay(charge, fee)?;
    vector
        .try_reserve_exact(count)
        .map_err(|_| Error::AllocationFailure)
}

impl std::fmt::Debug for OriginalPolicyCurrentCursor<'_, '_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OriginalPolicyCurrentCursor")
            .finish_non_exhaustive()
    }
}
