//! Measuring or appending to one prospectively reserved final byte owner.
use super::budget::{Budget, add};
use super::model::OriginalQueryRecordError as E;
pub(super) struct Writer {
    bytes: Option<Vec<u8>>,
    pub(super) length: usize,
    maximum: usize,
}
impl Writer {
    pub(super) const fn measure() -> Self {
        Self {
            bytes: None,
            length: 0,
            maximum: usize::MAX,
        }
    }
    pub(super) fn owned(length: usize, budget: &mut Budget) -> Result<Self, E> {
        budget.output(length)?;
        Ok(Self {
            bytes: Some(budget.reserve(length)?),
            length: 0,
            maximum: length,
        })
    }
    pub(super) fn put(&mut self, bytes: &[u8], budget: &mut Budget) -> Result<(), E> {
        budget.spend(add(bytes.len(), 1)?)?;
        let next = add(self.length, bytes.len())?;
        if next > self.maximum {
            return Err(E::Limit);
        }
        if let Some(output) = &mut self.bytes {
            output.extend_from_slice(bytes);
        }
        self.length = next;
        Ok(())
    }
    pub(super) fn byte(&mut self, value: u8, budget: &mut Budget) -> Result<(), E> {
        self.put(&[value], budget)
    }
    pub(super) fn u16(&mut self, value: usize, budget: &mut Budget) -> Result<(), E> {
        self.put(
            &u16::try_from(value)
                .map_err(|_| E::InvalidRecord)?
                .to_be_bytes(),
            budget,
        )
    }
    pub(super) fn varint(&mut self, mut value: u64, budget: &mut Budget) -> Result<(), E> {
        let mut bytes = [0; 10];
        let mut length = 0;
        loop {
            let low = u8::try_from(value & 127).map_err(|_| E::Limit)?;
            value >>= 7;
            *bytes.get_mut(length).ok_or(E::Limit)? = low | if value == 0 { 0 } else { 128 };
            length += 1;
            if value == 0 {
                break;
            }
        }
        self.put(bytes.get(..length).ok_or(E::Limit)?, budget)
    }
    pub(super) fn finish(self) -> Result<Vec<u8>, E> {
        if self.length != self.maximum {
            return Err(E::Limit);
        }
        self.bytes.ok_or(E::Limit)
    }
}
