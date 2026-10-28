//! The source's nil/value+1 base128 grammar and bounded borrowed reads.

use super::{
    OriginalCompactHistoryError as E,
    budget::{Budget, add},
};

pub(super) enum DecodeError {
    Malformed,
    Operational(E),
}

impl From<E> for DecodeError {
    fn from(error: E) -> Self {
        Self::Operational(error)
    }
}

pub(super) struct Output<'a> {
    bytes: Option<&'a mut Vec<u8>>,
    length: usize,
    maximum: usize,
}

impl<'a> Output<'a> {
    pub(super) fn new(bytes: Option<&'a mut Vec<u8>>, maximum: usize) -> Self {
        Self {
            bytes,
            length: 0,
            maximum,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.length
    }

    pub(super) fn put(&mut self, bytes: &[u8], budget: &mut Budget) -> Result<(), E> {
        let length = add(self.length, bytes.len())?;
        budget.spend(add(bytes.len(), 1)?)?;
        budget.encoded(length)?;
        if length > self.maximum {
            return Err(E::InvalidInput);
        }
        if let Some(output) = &mut self.bytes {
            if length > output.capacity() {
                return Err(E::InvalidInput);
            }
            output.extend_from_slice(bytes);
        }
        self.length = length;
        Ok(())
    }

    pub(super) fn byte(&mut self, byte: u8, budget: &mut Budget) -> Result<(), E> {
        self.put(&[byte], budget)
    }

    pub(super) fn integer(&mut self, value: Option<u128>, budget: &mut Budget) -> Result<(), E> {
        let mut encoded = match value {
            Some(value) => value.checked_add(1).ok_or(E::Limit)?,
            None => 0,
        };
        while encoded >= 128 {
            self.byte(
                u8::try_from(encoded & 127).map_err(|_| E::InvalidInput)? | 128,
                budget,
            )?;
            encoded >>= 7;
        }
        self.byte(u8::try_from(encoded).map_err(|_| E::InvalidInput)?, budget)
    }

    pub(super) fn binary(&mut self, value: Option<&[u8]>, budget: &mut Budget) -> Result<(), E> {
        match value {
            Some(bytes) => {
                self.integer(Some(bytes.len() as u128), budget)?;
                self.put(bytes, budget)
            }
            None => self.integer(None, budget),
        }
    }
}

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn position(&self) -> usize {
        self.offset
    }

    pub(super) fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    pub(super) fn take(
        &mut self,
        count: usize,
        budget: &mut Budget,
    ) -> Result<&'a [u8], DecodeError> {
        budget.spend(1)?;
        let end = self
            .offset
            .checked_add(count)
            .ok_or(DecodeError::Malformed)?;
        if end > self.bytes.len() {
            return Err(DecodeError::Malformed);
        }
        budget.spend(count)?;
        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    pub(super) fn integer(&mut self, budget: &mut Budget) -> Result<Option<u128>, DecodeError> {
        let mut encoded = 0_u128;
        for shift in (0..70).step_by(7) {
            let byte = self.take(1, budget)?[0];
            encoded |= u128::from(byte & 127) << shift;
            if byte & 128 == 0 {
                return Ok(encoded.checked_sub(1));
            }
        }
        Err(DecodeError::Malformed)
    }

    pub(super) fn binary(&mut self, budget: &mut Budget) -> Result<Option<&'a [u8]>, DecodeError> {
        let Some(length) = self.integer(budget)? else {
            return Ok(None);
        };
        if length > self.remaining() as u128 {
            return Err(DecodeError::Malformed);
        }
        let count = usize::try_from(length).map_err(|_| DecodeError::Malformed)?;
        Ok(Some(self.take(count, budget)?))
    }
}
