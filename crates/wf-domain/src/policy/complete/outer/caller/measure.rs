//! Private represented intent work over the actual borrowed operands.

use super::super::super::admission::charge_schema;
use super::super::super::{
    CompletePolicyData, CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError,
};
use super::super::CompletePolicyRequestError;

pub(super) struct MeasuredCompletePolicyIntent<'i> {
    pub(super) tenant: &'i [u8],
    pub(super) data: &'i CompletePolicyData,
    pub(super) expected_generation: Option<u64>,
    pub(super) bytes: usize,
    pub(super) nodes: u64,
}

pub(super) struct IntentMeasure<'c> {
    charge: &'c mut PolicySchemaCharge,
    bytes: usize,
    nodes: u64,
}

impl<'c> IntentMeasure<'c> {
    pub(super) const fn new(charge: &'c mut PolicySchemaCharge) -> Self {
        Self {
            charge,
            bytes: 0,
            nodes: 0,
        }
    }

    pub(super) fn node(&mut self) -> Result<(), CompletePolicyRequestError> {
        let next = self
            .nodes
            .checked_add(1)
            .ok_or(PolicySchemaError::LengthOverflow)?;
        charge_schema(self.charge, 128, 0)?;
        self.nodes = next;
        Ok(())
    }

    pub(super) fn write(&mut self, bytes: &[u8]) -> Result<(), CompletePolicyRequestError> {
        let next = self
            .bytes
            .checked_add(bytes.len())
            .ok_or(PolicySchemaError::LengthOverflow)?;
        if next > 1_048_576 {
            return Err(CompletePolicyEncodingError::ByteLimit.into());
        }
        let n = u64::try_from(bytes.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
        charge_schema(self.charge, n, 0)?;
        self.bytes = next;
        Ok(())
    }

    pub(super) fn raw(
        &mut self,
        bytes: &[u8],
        maximum: usize,
    ) -> Result<(), CompletePolicyRequestError> {
        self.node()?;
        if bytes.len() > maximum {
            return Err(CompletePolicyEncodingError::ByteLimit.into());
        }
        let n = u32::try_from(bytes.len()).map_err(|_| PolicySchemaError::LengthOverflow)?;
        self.u32(n)?;
        self.write(bytes)
    }

    pub(super) fn count(
        &mut self,
        count: usize,
        maximum: usize,
    ) -> Result<(), CompletePolicyRequestError> {
        if count > maximum {
            return Err(CompletePolicyEncodingError::ItemLimit.into());
        }
        let n = u32::try_from(count).map_err(|_| PolicySchemaError::LengthOverflow)?;
        self.node()?;
        self.u32(n)
    }

    pub(super) fn optional(&mut self, present: bool) -> Result<(), CompletePolicyRequestError> {
        self.u8(u8::from(present))?;
        if present {
            self.node()?;
        }
        Ok(())
    }

    pub(super) fn u8(&mut self, value: u8) -> Result<(), CompletePolicyRequestError> {
        self.write(&[value])
    }

    pub(super) fn u16(&mut self, value: u16) -> Result<(), CompletePolicyRequestError> {
        self.write(&value.to_be_bytes())
    }

    pub(super) fn u32(&mut self, value: u32) -> Result<(), CompletePolicyRequestError> {
        self.write(&value.to_be_bytes())
    }

    pub(super) fn u64(&mut self, value: u64) -> Result<(), CompletePolicyRequestError> {
        self.write(&value.to_be_bytes())
    }

    pub(super) const fn finish(self) -> (usize, u64) {
        (self.bytes, self.nodes)
    }
}
