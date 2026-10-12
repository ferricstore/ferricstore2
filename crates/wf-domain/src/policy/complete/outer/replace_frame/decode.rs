//! Freshly paid original Data reconstruction with consumed header identities.

use crate::workflow::admission::AdmissionLimits;

use super::super::super::encoding::{decode::decode_data_fields, reader::Reader};
use super::super::super::{
    CompletePolicyData, CompletePolicyEncodingError, PolicySchemaCharge, PolicySchemaError,
    PolicySchemaLimits,
};
use super::super::CompletePolicyReplaceInput;
use super::{CompletePolicyReplaceFrameHeader, CompletePolicyRequestError};

pub(super) fn decode_input(
    header: CompletePolicyReplaceFrameHeader<'_>,
    limits: PolicySchemaLimits,
    admission: AdmissionLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<CompletePolicyReplaceInput, CompletePolicyRequestError> {
    if charge.profile() != limits {
        return Err(PolicySchemaError::ProfileMismatch.into());
    }
    if header.encoded.len() > admission.decision_bytes() {
        return Err(CompletePolicyEncodingError::ByteLimit.into());
    }
    let CompletePolicyReplaceFrameHeader {
        original_intent,
        scope,
        request,
        stamp,
        ..
    } = header;
    let (expected_generation, data) = {
        let mut reader = Reader::new(original_intent, limits, charge);
        let fields = read_input(&mut reader)?;
        reader.finish()?;
        fields
    };
    Ok(CompletePolicyReplaceInput::new(
        scope,
        request,
        stamp,
        expected_generation,
        data,
    ))
}

fn read_input(
    reader: &mut Reader<'_, '_>,
) -> Result<(Option<u64>, CompletePolicyData), CompletePolicyEncodingError> {
    reader.node()?; // Structural original intent.
    if reader.take(30)? != b"WF-INTENT\0COMPLETE-REPLACE\0V2\0" {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    if reader.raw(8_192)?.is_empty() {
        return Err(CompletePolicyEncodingError::Malformed);
    }
    let expected_generation = reader.optional(Reader::u64)?;
    reader.node()?; // Structural original Data.
    reader.node()?; // Owning original Data.
    Ok((expected_generation, decode_data_fields(reader)?))
}
