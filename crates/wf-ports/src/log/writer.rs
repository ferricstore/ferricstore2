//! Checked measuring and exact-reservation writing of the same private grammar.

use super::CodecError;

pub(super) struct Writer<'a> {
    data: Option<Vec<u8>>,
    expected: Option<&'a [u8]>,
    length: usize,
    maximum: usize,
}

impl<'a> Writer<'a> {
    pub(super) const fn measure(maximum: usize) -> Self {
        Self {
            data: None,
            expected: None,
            length: 0,
            maximum,
        }
    }
    pub(super) fn allocate(length: usize, maximum: usize) -> Result<Self, CodecError> {
        if length > maximum {
            return Err(CodecError::ByteLimit);
        }
        let mut data = Vec::new();
        data.try_reserve_exact(length)
            .map_err(|_| CodecError::Allocation)?;
        Ok(Self {
            data: Some(data),
            expected: None,
            length: 0,
            maximum,
        })
    }
    pub(super) const fn verify(expected: &'a [u8], maximum: usize) -> Result<Self, CodecError> {
        if expected.len() > maximum {
            return Err(CodecError::ByteLimit);
        }
        Ok(Self {
            data: None,
            expected: Some(expected),
            length: 0,
            maximum,
        })
    }
    pub(super) const fn len(&self) -> usize {
        self.length
    }
    pub(super) fn write(&mut self, bytes: &[u8]) -> Result<(), CodecError> {
        let start = self.length;
        self.length = self
            .length
            .checked_add(bytes.len())
            .ok_or(CodecError::LengthOverflow)?;
        if self.length > self.maximum {
            return Err(CodecError::ByteLimit);
        }
        if self
            .expected
            .is_some_and(|expected| expected.get(start..self.length) != Some(bytes))
        {
            return Err(CodecError::Malformed);
        }
        if let Some(data) = &mut self.data {
            if self.length > data.capacity() {
                return Err(CodecError::LengthOverflow);
            }
            data.extend_from_slice(bytes);
        }
        Ok(())
    }
    pub(super) fn byte(&mut self, value: u8) -> Result<(), CodecError> {
        self.write(&[value])
    }
    pub(super) fn u16(&mut self, value: u16) -> Result<(), CodecError> {
        self.write(&value.to_be_bytes())
    }
    pub(super) fn u32(&mut self, value: u32) -> Result<(), CodecError> {
        self.write(&value.to_be_bytes())
    }
    pub(super) fn u64(&mut self, value: u64) -> Result<(), CodecError> {
        self.write(&value.to_be_bytes())
    }
    pub(super) fn raw(&mut self, bytes: &[u8], maximum: usize) -> Result<(), CodecError> {
        if bytes.len() > maximum {
            return Err(CodecError::ValueLimit);
        }
        self.u32(u32::try_from(bytes.len()).map_err(|_| CodecError::LengthOverflow)?)?;
        self.write(bytes)
    }
    pub(super) fn optional<T: ?Sized>(
        &mut self,
        value: Option<&T>,
        write: impl FnOnce(&mut Self, &T) -> Result<(), CodecError>,
    ) -> Result<(), CodecError> {
        if let Some(value) = value {
            self.byte(1)?;
            write(self, value)
        } else {
            self.byte(0)
        }
    }
    pub(super) fn finish(self) -> Result<Vec<u8>, CodecError> {
        self.data.ok_or(CodecError::Malformed)
    }
    pub(super) fn finish_verification(self) -> Result<(), CodecError> {
        if self
            .expected
            .is_some_and(|expected| expected.len() == self.length)
        {
            Ok(())
        } else {
            Err(CodecError::Malformed)
        }
    }
}
