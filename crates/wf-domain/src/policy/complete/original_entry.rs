//! Bounded original OTP external-policy entries, without normalized admission.

use super::admission::charge_schema;
use super::{
    CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError, PolicySchemaLimits,
};

mod callsite;
mod compound;
mod identity;
pub(in crate::policy::complete) mod indexed;
mod runtime;
mod safety;
mod scalar;
mod scan;
mod visit;

pub use callsite::{
    OriginalPolicyReadError, OriginalPolicyReadValue,
    prepare_original_complete_policy_reference_bytes, read_original_complete_policy_entry,
};

/// A captured mathematical canonical local-function entry, without runtime attestation.
#[derive(Clone, Copy, Debug)]
pub struct OriginalEtfLocalFun<'context> {
    module: &'context str,
    index: i32,
    arity: u8,
    uniq: [u8; 16],
    old_uniq: i32,
}

impl<'context> OriginalEtfLocalFun<'context> {
    /// Preserves the five captured source entry fields; admission is fresh on each call.
    #[must_use]
    pub const fn new(
        module: &'context str,
        index: i32,
        arity: u8,
        uniq: [u8; 16],
        old_uniq: i32,
    ) -> Self {
        Self {
            module,
            index,
            arity,
            uniq,
            old_uniq,
        }
    }
}

/// Borrowed captured atom, export, node and canonical local-function tables.
///
/// These mathematical inputs supply no runtime coherence, current authority or credit.
#[derive(Clone, Copy, Debug)]
pub struct OriginalEtfSafety<'context> {
    atoms: &'context [&'context str],
    active_exports: &'context [(&'context str, &'context str, u32)],
    local_node: Option<(&'context str, u32)>,
    local_funs: &'context [OriginalEtfLocalFun<'context>],
}

impl<'context> OriginalEtfSafety<'context> {
    /// Captures four immutable borrowed tables without validating or paying them.
    #[must_use]
    pub const fn new(
        atoms: &'context [&'context str],
        active_exports: &'context [(&'context str, &'context str, u32)],
        local_node: Option<(&'context str, u32)>,
        local_funs: &'context [OriginalEtfLocalFun<'context>],
    ) -> Self {
        Self {
            atoms,
            active_exports,
            local_node,
            local_funs,
        }
    }
}

/// Original immutable bytes and source envelope facts after temporary indexes drop.
#[derive(Debug)]
pub struct OriginalPolicyEntry<'encoded> {
    encoded: &'encoded [u8],
    policy_range: (usize, usize),
    generation: u64,
    workflow_type: Option<&'encoded [u8]>,
}

impl<'encoded> OriginalPolicyEntry<'encoded> {
    /// Borrows the exact received encoding, without canonical re-emission.
    #[must_use]
    pub const fn encoded(&self) -> &'encoded [u8] {
        self.encoded
    }
    /// Borrows the complete original encoded policy subterm, including aliases.
    #[must_use]
    pub fn policy_bytes(&self) -> &'encoded [u8] {
        &self.encoded[self.policy_range.0..self.policy_range.1]
    }
    /// Reads the original generation, including zero.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Borrows only an atom-keyed binary type; other arbitrary map content is retained.
    #[must_use]
    pub const fn workflow_type(&self) -> Option<&'encoded [u8]> {
        self.workflow_type
    }
}

#[derive(Clone, Copy, Default)]
struct TermSpan {
    start: u32,
    end: u32,
    next: u32,
    canonical: u32,
    symbol: u32,
    keys: u32,
}

impl TermSpan {
    fn tag(self) -> u8 {
        self.symbol.to_be_bytes()[0]
    }
    fn datum(self) -> u32 {
        self.symbol & 0x00ff_ffff
    }
    fn set_datum(&mut self, value: u32) {
        self.symbol = (self.symbol & 0xff00_0000) | value;
    }
}

#[derive(Clone, Copy)]
struct VirtualLocalFun {
    module: u32,
    index: i32,
    arity: u8,
    uniq: [u8; 16],
    old_uniq: i32,
}

#[derive(Clone, Copy)]
enum TermPosition {
    Encoded(u32),
    StringCell { span: u32, offset: u32 },
    Byte(u8),
    Nil,
}

#[derive(Clone, Copy)]
struct EtfMap<'raw, 'index, 'context> {
    raw: &'raw [u8],
    spans: &'index [TermSpan],
    safety: OriginalEtfSafety<'context>,
    root: u32,
}

fn pay(charge: &mut PolicySchemaCharge, amount: u64) -> Result<(), Error> {
    charge_schema(charge, amount, 0).map_err(Error::Schema)
}

fn overflow() -> Error {
    Error::Schema(PolicySchemaError::LengthOverflow)
}

/// Decodes the source original three-tuple and arbitrary map without business admission.
///
/// The portable OTP28.3.1 profile excludes LOCAL/internal runtime-affine encodings,
/// depth above64 and more than4096 encoded spans operationally. Captured tables
/// are freshly admitted; they do not attest actual VM runtime state.
///
/// # Errors
/// Preserves work/profile/reservation and unsupported-runtime errors. Source
/// syntax, safety, envelope, generation and deprecated-key refusals return None.
pub fn decode_original_complete_policy_entry<'encoded>(
    raw: &'encoded [u8],
    safety: &OriginalEtfSafety<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<OriginalPolicyEntry<'encoded>>, Error> {
    decode_with_span_limit(raw, safety, 4_096, limits, charge)
}

/// Reads original current bytes under the whole-flow parent's encoded-span profile.
pub(super) fn decode_original_flow_policy_current_entry<'encoded>(
    raw: &'encoded [u8],
    safety: &OriginalEtfSafety<'_>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<OriginalPolicyEntry<'encoded>>, Error> {
    decode_with_span_limit(raw, safety, 263_168, limits, charge)
}

fn decode_with_span_limit<'encoded>(
    raw: &'encoded [u8],
    safety: &OriginalEtfSafety<'_>,
    span_limit: usize,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<OriginalPolicyEntry<'encoded>>, Error> {
    Ok(
        decode_indexed(raw, safety, (span_limit, 4_096), 0, limits, charge)?
            .map(|value| value.entry),
    )
}

pub(in crate::policy::complete) fn decode_retained<'encoded, 'context>(
    raw: &'encoded [u8],
    safety: &OriginalEtfSafety<'context>,
    span_limit: usize,
    live: usize,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<indexed::OriginalPolicyCurrentIndex<'encoded, 'context>>, Error> {
    decode_indexed(raw, safety, (span_limit, span_limit), live, limits, charge)
}

fn decode_indexed<'encoded, 'context>(
    raw: &'encoded [u8],
    safety: &OriginalEtfSafety<'context>,
    bounds: (usize, usize),
    live: usize,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<indexed::OriginalPolicyCurrentIndex<'encoded, 'context>>, Error> {
    let (span_limit, map_limit) = bounds;
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    pay(charge, 128)?;
    if raw.len() > 263_168 || raw.len() < 2 {
        return Ok(None);
    }
    pay(charge, 1)?;
    if raw[0] != 131 {
        return Ok(None);
    }
    pay(charge, 1)?;
    if raw[1] == 80 {
        return Ok(None);
    }
    safety::admit(safety, charge)?;
    let mut measured = (0_usize, 0_usize, 0_usize);
    let mut depth = 0;
    let first = scan::run(raw, safety, span_limit, &mut measured, &mut None, charge);
    if source_result(first)?.is_none() {
        return Ok(None);
    }
    reserve_peak(measured, live, charge)?;
    let mut spans = reserved(measured.0, 32, charge)?;
    let mut keys = reserved(measured.1, 4, charge)?;
    let mut virtual_funs = reserved(measured.2, 64, charge)?;
    let mut output = Some((&mut spans, &mut virtual_funs));
    measured = (0, 0, 0);
    let second = scan::run_with_depth(
        raw,
        safety,
        span_limit,
        &mut measured,
        &mut depth,
        &mut output,
        charge,
    );
    if source_result(second)?.is_none() {
        return Ok(None);
    }
    if source_result(identity::maps(
        raw,
        &mut spans,
        &mut keys,
        safety,
        &virtual_funs,
        charge,
    ))?
    .is_none()
    {
        return Ok(None);
    }
    let Some((generation, policy)) = compound::envelope(raw, &spans, safety, charge)? else {
        return Ok(None);
    };
    let map = EtfMap {
        raw,
        spans: &spans,
        safety: *safety,
        root: compound::resolve(&spans, policy),
    };
    if if map_limit == 4_096 {
        visit::old_keys(map, limits, charge)?
    } else {
        visit::old_keys_with_node_limit(map, limits, spans.len().min(map_limit), charge)?
    } {
        return Ok(None);
    }
    let workflow_type = visit::workflow_type(map, charge)?;
    pay(charge, 128)?;
    let policy_span = spans[usize::try_from(policy).map_err(|_| overflow())?];
    let entry = OriginalPolicyEntry {
        encoded: raw,
        policy_range: (
            usize::try_from(policy_span.start).map_err(|_| overflow())?,
            usize::try_from(policy_span.end).map_err(|_| overflow())?,
        ),
        generation,
        workflow_type,
    };
    Ok(Some(indexed::OriginalPolicyCurrentIndex {
        raw,
        spans,
        keys,
        virtual_funs,
        safety: *safety,
        entry,
        catalog: Vec::new(),
        policy,
        depth: depth.saturating_sub(1),
    }))
}

fn reserve_peak(
    counts: (usize, usize, usize),
    live: usize,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let (n, k, f) = counts;
    let peak = n
        .checked_add(k)
        .and_then(|v| v.checked_add(f))
        .and_then(|v| v.checked_mul(64))
        .and_then(|v| v.checked_add(live))
        .ok_or_else(overflow)?;
    charge_schema(charge, 0, u64::try_from(peak).map_err(|_| overflow())?)
        .map_err(Error::Schema)?;
    Ok(())
}

fn reserved<T>(count: usize, width: u64, charge: &mut PolicySchemaCharge) -> Result<Vec<T>, Error> {
    let mut owner = Vec::new();
    reserve(&mut owner, count, width, charge)?;
    Ok(owner)
}

fn source_result<T>(result: Result<T, Error>) -> Result<Option<T>, Error> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(Error::Malformed | Error::TrailingBytes) => Ok(None),
        Err(error) => Err(error),
    }
}

fn reserve<T>(
    vector: &mut Vec<T>,
    count: usize,
    width: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let fee = u64::try_from(count)
        .map_err(|_| overflow())?
        .checked_mul(width)
        .and_then(|n| n.checked_add(128))
        .ok_or_else(overflow)?;
    pay(charge, fee)?;
    vector
        .try_reserve_exact(count)
        .map_err(|_| Error::AllocationFailure)
}
