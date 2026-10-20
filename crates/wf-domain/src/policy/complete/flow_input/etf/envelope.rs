//! Direct original envelope from the same immutable, privately measured layout.

use crate::policy::installation::MAX_POLICY_GENERATION;

use super::{
    Error, FlowPolicyLayout, PolicySchemaCharge, block, charge_schema, integer, overflow, visit,
};

pub(in crate::policy::complete::flow_input) fn emit_envelope(
    layout: &FlowPolicyLayout<'_>,
    root: usize,
    snapshot_length: usize,
    generation: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, Error> {
    emit_envelope_bounded(layout, root, snapshot_length, generation, 263_168, charge)
}

pub(in crate::policy::complete::flow_input) fn emit_envelope_bounded(
    layout: &FlowPolicyLayout<'_>,
    root: usize,
    snapshot_length: usize,
    generation: u64,
    maximum_encoded_bytes: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, Error> {
    if generation > MAX_POLICY_GENERATION {
        return Err(Error::Malformed);
    }
    let generation_width = if u8::try_from(generation).is_ok() {
        2
    } else if i32::try_from(generation).is_ok() {
        5
    } else {
        let digits = generation.bit_width().div_ceil(8);
        3 + usize::try_from(digits).map_err(|_| overflow())?
    };
    let length = snapshot_length
        .checked_add(18)
        .and_then(|bytes| bytes.checked_add(generation_width))
        .ok_or_else(overflow)?;
    if length > maximum_encoded_bytes || u32::try_from(length).is_err() {
        return Err(Error::ByteLimit);
    }
    let fee = u64::try_from(length)
        .ok()
        .and_then(|bytes| bytes.checked_add(128))
        .ok_or_else(overflow)?;
    charge_schema(charge, fee, 0).map_err(Error::Schema)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| Error::AllocationFailure)?;
    let mut output = Some(&mut bytes);
    block(&[131], &[], false, &mut output, length, charge)?;
    block(&[104, 3], &[], true, &mut output, length, charge)?;
    block(
        &[119, 14],
        b"flow_policy_v1",
        true,
        &mut output,
        length,
        charge,
    )?;
    integer(u128::from(generation), &mut output, length, charge)?;
    visit(layout, root, charge, &mut output, length)?;
    if bytes.len() != length {
        return Err(Error::EncodingMismatch);
    }
    Ok(bytes)
}
