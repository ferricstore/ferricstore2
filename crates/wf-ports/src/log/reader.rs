//! Borrowed checked field navigation, followed by bounded owned materialization.

use super::{CodecError, CodecLimits};

pub(super) struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
    pub(super) limits: CodecLimits,
}

impl<'a> Reader<'a> {
    pub(super) const fn new(data: &'a [u8], limits: CodecLimits) -> Self {
        Self {
            data,
            offset: 0,
            limits,
        }
    }
    pub(super) fn take(&mut self, length: usize) -> Result<&'a [u8], CodecError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(CodecError::LengthOverflow)?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or(CodecError::Malformed)?;
        self.offset = end;
        Ok(bytes)
    }
    pub(super) fn fixed<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        self.take(N)?.try_into().map_err(|_| CodecError::Malformed)
    }
    pub(super) fn byte(&mut self) -> Result<u8, CodecError> {
        Ok(self.fixed::<1>()?[0])
    }
    pub(super) fn u16(&mut self) -> Result<u16, CodecError> {
        Ok(u16::from_be_bytes(self.fixed()?))
    }
    pub(super) fn u32(&mut self) -> Result<u32, CodecError> {
        Ok(u32::from_be_bytes(self.fixed()?))
    }
    pub(super) fn u64(&mut self) -> Result<u64, CodecError> {
        Ok(u64::from_be_bytes(self.fixed()?))
    }
    pub(super) fn raw(&mut self, maximum: usize) -> Result<&'a [u8], CodecError> {
        let length = usize::try_from(self.u32()?).map_err(|_| CodecError::LengthOverflow)?;
        if length > maximum {
            return Err(CodecError::ValueLimit);
        }
        self.take(length)
    }
    pub(super) fn nonempty(&mut self, maximum: usize) -> Result<&'a [u8], CodecError> {
        let value = self.raw(maximum)?;
        if value.is_empty() {
            Err(CodecError::Malformed)
        } else {
            Ok(value)
        }
    }
    pub(super) fn field(&mut self) -> Result<&'a [u8], CodecError> {
        self.raw(self.limits.admission.inline_bytes())
    }
    pub(super) fn owned(&mut self) -> Result<Vec<u8>, CodecError> {
        copy(self.field()?)
    }
    pub(super) fn optional<T>(
        &mut self,
        read: impl FnOnce(&mut Self) -> Result<T, CodecError>,
    ) -> Result<Option<T>, CodecError> {
        match self.byte()? {
            0 => Ok(None),
            1 => read(self).map(Some),
            _ => Err(CodecError::InvalidOptionality),
        }
    }
    pub(super) fn remaining(&self) -> usize {
        self.data.len() - self.offset
    }
    pub(super) fn finish(self) -> Result<(), CodecError> {
        if self.offset == self.data.len() {
            Ok(())
        } else {
            Err(CodecError::TrailingBytes)
        }
    }
}

pub(super) fn copy(bytes: &[u8]) -> Result<Vec<u8>, CodecError> {
    let mut value = Vec::new();
    value
        .try_reserve_exact(bytes.len())
        .map_err(|_| CodecError::Allocation)?;
    value.extend_from_slice(bytes);
    Ok(value)
}
