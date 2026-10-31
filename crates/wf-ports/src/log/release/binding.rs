//! Native record extension with a completely inspected borrowed intent.
use super::{E, ReleaseIntentRef, intent};
use crate::log::writer::Writer;
use wf_domain::workflow::MAX_EXACT_MILLIS;

/// Original paid generation and captured reservation; no current spend authority.
#[derive(Clone, Copy)]
pub struct ReleaseBindingRef<'a> {
    /// Positive finite generation of the original identified reservation.
    pub epoch: u64,
    /// Complete original record identity, credit identity and binding time.
    pub intent: ReleaseIntentRef<'a>,
}

/// Encodes one exact native binding without an intermediate intent allocation.
/// # Errors
/// Refuses invalid generation/intent, byte limits or output reservation failure.
pub fn encode_release_binding(
    binding: ReleaseBindingRef<'_>,
    maximum: usize,
) -> Result<Vec<u8>, E> {
    if !(1..=MAX_EXACT_MILLIS).contains(&binding.epoch) {
        return Err(E::Malformed);
    }
    wf_domain::governance::release::validate_release_intent(binding.intent)
        .map_err(|_| E::Malformed)?;
    let mut measure = Writer::measure(maximum);
    write(binding, &mut measure)?;
    let mut output = Writer::allocate(measure.len(), maximum)?;
    write(binding, &mut output)?;
    output.finish()
}

/// Inspects the complete original binding before any field is copied.
/// # Errors
/// Refuses unsupported framing, invalid fields, complete EOF or byte limits.
pub fn inspect_release_binding(encoded: &[u8], maximum: usize) -> Result<ReleaseBindingRef<'_>, E> {
    if encoded.len() > maximum {
        return Err(E::ByteLimit);
    }
    if encoded.get(..4) != Some(b"FWGB") {
        return Err(E::UnsupportedTag);
    }
    if encoded.get(4..6) != Some(b"\0\x01") {
        return Err(E::UnsupportedVersion);
    }
    let epoch = u64::from_be_bytes(
        encoded
            .get(6..14)
            .ok_or(E::Malformed)?
            .try_into()
            .map_err(|_| E::Malformed)?,
    );
    if !(1..=MAX_EXACT_MILLIS).contains(&epoch) {
        return Err(E::Malformed);
    }
    Ok(ReleaseBindingRef {
        epoch,
        intent: intent::inspect(&encoded[14..], maximum)?,
    })
}

fn write(binding: ReleaseBindingRef<'_>, output: &mut Writer<'_>) -> Result<(), E> {
    output.write(b"FWGB\0\x01")?;
    output.u64(binding.epoch)?;
    intent::write(binding.intent, output)
}
