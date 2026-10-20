//! Borrowed structural reads through the original schema account.

use std::ops::Range;

use super::super::admission::charge_schema;
use super::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};

pub(in crate::policy::complete) struct Reader<'b, 'c> {
    pub(super) bytes: &'b [u8],
    cursor: usize,
    pub(super) limits: PolicySchemaLimits,
    pub(super) charge: &'c mut PolicySchemaCharge,
}

impl<'b, 'c> Reader<'b, 'c> {
    pub(in crate::policy::complete) fn new(
        bytes: &'b [u8],
        limits: PolicySchemaLimits,
        charge: &'c mut PolicySchemaCharge,
    ) -> Self {
        Self {
            bytes,
            cursor: 0,
            limits,
            charge,
        }
    }

    pub(in crate::policy::complete) const fn position(&self) -> usize {
        self.cursor
    }

    pub(in crate::policy::complete) fn take(
        &mut self,
        n: usize,
    ) -> Result<&'b [u8], CompletePolicyEncodingError> {
        let end = self
            .cursor
            .checked_add(n)
            .ok_or(CompletePolicyEncodingError::Schema(
                PolicySchemaError::LengthOverflow,
            ))?;
        let bytes = self
            .bytes
            .get(self.cursor..end)
            .ok_or(CompletePolicyEncodingError::Malformed)?;
        let work = u64::try_from(n)
            .map_err(|_| CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow))?;
        charge_schema(self.charge, work, 0).map_err(CompletePolicyEncodingError::Schema)?;
        self.cursor = end;
        Ok(bytes)
    }

    pub(in crate::policy::complete) fn byte(&mut self) -> Result<u8, CompletePolicyEncodingError> {
        Ok(self.take(1)?[0])
    }

    pub(in crate::policy::complete) fn u16(&mut self) -> Result<u16, CompletePolicyEncodingError> {
        Ok(u16::from_be_bytes(
            self.take(2)?
                .try_into()
                .map_err(|_| CompletePolicyEncodingError::Malformed)?,
        ))
    }

    pub(in crate::policy::complete) fn u32(&mut self) -> Result<u32, CompletePolicyEncodingError> {
        Ok(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| CompletePolicyEncodingError::Malformed)?,
        ))
    }

    pub(in crate::policy::complete) fn u64(&mut self) -> Result<u64, CompletePolicyEncodingError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| CompletePolicyEncodingError::Malformed)?,
        ))
    }

    pub(in crate::policy::complete) fn optional<T>(
        &mut self,
        read: impl FnOnce(&mut Self) -> Result<T, CompletePolicyEncodingError>,
    ) -> Result<Option<T>, CompletePolicyEncodingError> {
        match self.byte()? {
            0 => Ok(None),
            1 => {
                self.node()?;
                read(self).map(Some)
            }
            _ => Err(CompletePolicyEncodingError::InvalidOptionality),
        }
    }

    pub(in crate::policy::complete) fn raw(
        &mut self,
        maximum: usize,
    ) -> Result<&'b [u8], CompletePolicyEncodingError> {
        self.node()?;
        let n = usize::try_from(self.u32()?)
            .map_err(|_| CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow))?;
        if n > maximum {
            return Err(CompletePolicyEncodingError::ByteLimit);
        }
        self.take(n)
    }

    pub(in crate::policy::complete) fn count(
        &mut self,
        maximum: usize,
    ) -> Result<usize, CompletePolicyEncodingError> {
        let count = usize::try_from(self.u32()?)
            .map_err(|_| CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow))?;
        if count > maximum {
            return Err(CompletePolicyEncodingError::ItemLimit);
        }
        self.node()?;
        Ok(count)
    }

    pub(in crate::policy::complete) fn state_count(
        &mut self,
        profiled: bool,
    ) -> Result<usize, CompletePolicyEncodingError> {
        let count = usize::try_from(self.u32()?)
            .map_err(|_| CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow))?;
        if count > self.limits.policy_limits().max_states() {
            return Err(if profiled {
                CompletePolicyEncodingError::Schema(PolicySchemaError::Policy(
                    crate::policy::installation::PolicyError::StateBudget,
                ))
            } else {
                CompletePolicyEncodingError::ItemLimit
            });
        }
        self.node()?;
        Ok(count)
    }

    pub(in crate::policy::complete) fn node(&mut self) -> Result<(), CompletePolicyEncodingError> {
        charge_schema(self.charge, 128, 0).map_err(CompletePolicyEncodingError::Schema)
    }

    pub(in crate::policy::complete) fn region(
        &mut self,
        range: Range<usize>,
    ) -> Result<Reader<'b, '_>, CompletePolicyEncodingError> {
        let bytes = self
            .bytes
            .get(range)
            .ok_or(CompletePolicyEncodingError::Malformed)?;
        Ok(Reader::new(bytes, self.limits, self.charge))
    }

    pub(in crate::policy::complete) fn keys(
        &mut self,
        previous: &[u8],
        current: &[u8],
        maximum: usize,
    ) -> Result<(), CompletePolicyEncodingError> {
        if previous.len() > maximum || current.len() > maximum {
            return Err(CompletePolicyEncodingError::ByteLimit);
        }
        let work = previous
            .len()
            .checked_add(current.len())
            .and_then(|n| n.checked_add(128))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or(CompletePolicyEncodingError::Schema(
                PolicySchemaError::LengthOverflow,
            ))?;
        charge_schema(self.charge, work, 0).map_err(CompletePolicyEncodingError::Schema)?;
        if previous >= current {
            return Err(CompletePolicyEncodingError::Malformed);
        }
        Ok(())
    }

    pub(in crate::policy::complete) fn finish(&self) -> Result<(), CompletePolicyEncodingError> {
        if self.cursor != self.bytes.len() {
            return Err(CompletePolicyEncodingError::TrailingBytes);
        }
        Ok(())
    }
}
