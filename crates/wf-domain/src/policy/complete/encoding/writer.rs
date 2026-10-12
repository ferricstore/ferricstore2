//! Measurement, one bounded output, or streamed comparison on the same account.

use super::super::admission::charge_schema;
use super::{
    CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};

pub(super) enum Sink<'e> {
    Measure,
    Output(Vec<u8>),
    Compare(&'e [u8]),
}

pub(super) struct Writer<'e, 'c> {
    sink: Sink<'e>,
    cursor: usize,
    maximum: usize,
    pub(super) limits: PolicySchemaLimits,
    charge: &'c mut PolicySchemaCharge,
}

impl<'e, 'c> Writer<'e, 'c> {
    pub(super) fn measure(
        maximum: usize,
        limits: PolicySchemaLimits,
        charge: &'c mut PolicySchemaCharge,
    ) -> Self {
        Self {
            sink: Sink::Measure,
            cursor: 0,
            maximum,
            limits,
            charge,
        }
    }

    pub(super) fn output(
        exact_len: usize,
        maximum: usize,
        limits: PolicySchemaLimits,
        charge: &'c mut PolicySchemaCharge,
    ) -> Result<Self, CompletePolicyEncodingError> {
        if exact_len > maximum {
            return Err(CompletePolicyEncodingError::ByteLimit);
        }
        let work = u64::try_from(exact_len)
            .ok()
            .and_then(|n| n.checked_add(128))
            .ok_or(CompletePolicyEncodingError::Schema(
                PolicySchemaError::LengthOverflow,
            ))?;
        charge_schema(charge, work, 0).map_err(CompletePolicyEncodingError::Schema)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(exact_len)
            .map_err(|_| CompletePolicyEncodingError::AllocationFailure)?;
        Ok(Self {
            sink: Sink::Output(bytes),
            cursor: 0,
            maximum,
            limits,
            charge,
        })
    }

    pub(super) fn compare(
        expected: &'e [u8],
        maximum: usize,
        limits: PolicySchemaLimits,
        charge: &'c mut PolicySchemaCharge,
    ) -> Self {
        Self {
            sink: Sink::Compare(expected),
            cursor: 0,
            maximum,
            limits,
            charge,
        }
    }

    pub(super) const fn position(&self) -> usize {
        self.cursor
    }

    pub(super) fn write(&mut self, bytes: &[u8]) -> Result<(), CompletePolicyEncodingError> {
        let end =
            self.cursor
                .checked_add(bytes.len())
                .ok_or(CompletePolicyEncodingError::Schema(
                    PolicySchemaError::LengthOverflow,
                ))?;
        if end > self.maximum {
            return Err(match self.sink {
                Sink::Measure => {
                    CompletePolicyEncodingError::Schema(PolicySchemaError::SnapshotBytes)
                }
                Sink::Output(_) => CompletePolicyEncodingError::ByteLimit,
                Sink::Compare(_) => CompletePolicyEncodingError::EncodingMismatch,
            });
        }
        let n = u64::try_from(bytes.len())
            .map_err(|_| CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow))?;
        let expected = match self.sink {
            Sink::Compare(expected) => Some(
                expected
                    .get(self.cursor..end)
                    .ok_or(CompletePolicyEncodingError::EncodingMismatch)?,
            ),
            _ => None,
        };
        let work = if expected.is_some() {
            n.checked_mul(2).ok_or(CompletePolicyEncodingError::Schema(
                PolicySchemaError::LengthOverflow,
            ))?
        } else {
            n
        };
        charge_schema(self.charge, work, 0).map_err(CompletePolicyEncodingError::Schema)?;
        self.cursor = end;
        match &mut self.sink {
            Sink::Measure => Ok(()),
            Sink::Output(output) => {
                output.extend_from_slice(bytes);
                Ok(())
            }
            Sink::Compare(_) => {
                if expected != Some(bytes) {
                    return Err(CompletePolicyEncodingError::EncodingMismatch);
                }
                Ok(())
            }
        }
    }

    pub(super) fn byte(&mut self, value: u8) -> Result<(), CompletePolicyEncodingError> {
        self.write(&[value])
    }

    pub(super) fn u16(&mut self, value: u16) -> Result<(), CompletePolicyEncodingError> {
        self.write(&value.to_be_bytes())
    }

    pub(super) fn u32(&mut self, value: u32) -> Result<(), CompletePolicyEncodingError> {
        self.write(&value.to_be_bytes())
    }

    pub(super) fn u64(&mut self, value: u64) -> Result<(), CompletePolicyEncodingError> {
        self.write(&value.to_be_bytes())
    }

    pub(super) fn optional<T>(
        &mut self,
        value: Option<&T>,
        write: impl FnOnce(&T, &mut Self) -> Result<(), CompletePolicyEncodingError>,
    ) -> Result<(), CompletePolicyEncodingError> {
        match value {
            None => self.byte(0),
            Some(value) => {
                self.byte(1)?;
                self.node()?;
                write(value, self)
            }
        }
    }

    pub(super) fn raw(
        &mut self,
        bytes: &[u8],
        maximum: usize,
    ) -> Result<(), CompletePolicyEncodingError> {
        self.node()?;
        if bytes.len() > maximum {
            return Err(CompletePolicyEncodingError::ByteLimit);
        }
        let length = u32::try_from(bytes.len())
            .map_err(|_| CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow))?;
        self.u32(length)?;
        self.write(bytes)
    }

    pub(super) fn count(
        &mut self,
        count: usize,
        maximum: usize,
    ) -> Result<(), CompletePolicyEncodingError> {
        let count_u32 = u32::try_from(count)
            .map_err(|_| CompletePolicyEncodingError::Schema(PolicySchemaError::LengthOverflow))?;
        if count > maximum {
            return Err(CompletePolicyEncodingError::ItemLimit);
        }
        self.u32(count_u32)?;
        self.node()
    }

    pub(super) fn node(&mut self) -> Result<(), CompletePolicyEncodingError> {
        charge_schema(self.charge, 128, 0).map_err(CompletePolicyEncodingError::Schema)
    }

    pub(super) fn finish(&self, expected_len: usize) -> Result<(), CompletePolicyEncodingError> {
        if self.cursor != expected_len {
            return Err(CompletePolicyEncodingError::EncodingMismatch);
        }
        Ok(())
    }

    pub(super) fn into_output(self) -> Result<Vec<u8>, CompletePolicyEncodingError> {
        match self.sink {
            Sink::Output(bytes) => Ok(bytes),
            _ => Err(CompletePolicyEncodingError::Malformed),
        }
    }
}
