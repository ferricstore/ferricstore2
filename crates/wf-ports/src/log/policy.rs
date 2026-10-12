//! A-N14 cohesive private policy grammar, verified bytes and single value ownership.

use wf_domain::policy::installation::{
    InstallCommand, MAX_ENCODED_POLICY_BYTES, PolicyCandidate, PolicyValue,
};

use super::{CodecError, CodecLimits, reader::Reader, writer::Writer};

mod decode;
mod encode;
mod scan;

pub(super) fn prepare(
    candidate: PolicyCandidate,
    limits: CodecLimits,
) -> Result<PolicyValue, CodecError> {
    let mut measured = Writer::measure(MAX_ENCODED_POLICY_BYTES);
    encode::envelope(
        candidate.generation(),
        candidate.policy(),
        &mut measured,
        limits,
    )?;
    let mut writer = Writer::allocate(measured.len(), MAX_ENCODED_POLICY_BYTES)?;
    encode::envelope(
        candidate.generation(),
        candidate.policy(),
        &mut writer,
        limits,
    )?;
    let encoded = writer.finish()?;
    let end = encoded.len();
    candidate
        .bind_encoded_snapshot(18..end, encoded, limits.policy_limits())
        .map_err(|_| CodecError::Malformed)
}

pub(super) fn verify(value: &PolicyValue, limits: CodecLimits) -> Result<(), CodecError> {
    let mut writer = Writer::verify(value.encoded(), MAX_ENCODED_POLICY_BYTES)?;
    encode::envelope(value.generation(), value.policy(), &mut writer, limits)?;
    writer.finish_verification()
}

pub(super) fn write(
    command: &InstallCommand,
    value: &PolicyValue,
    writer: &mut Writer<'_>,
    limits: CodecLimits,
) -> Result<(), CodecError> {
    encode::command(command, writer, limits)?;
    writer.raw(value.encoded(), MAX_ENCODED_POLICY_BYTES)
}

pub(super) fn read(
    reader: &mut Reader<'_>,
) -> Result<crate::consensus::PreparedOperation, CodecError> {
    let command = decode::command(reader)?;
    let value = decode::value(reader)?;
    Ok(crate::consensus::PreparedOperation::InstallPolicy {
        command,
        value: Box::new(value),
    })
}

pub(super) fn preflight(reader: &mut Reader<'_>) -> Result<(), CodecError> {
    scan::command(reader)?;
    scan::value(reader)
}
