//! Canonical original caller bytes, independent of generated or routing inputs.

use super::super::reader::Reader;
use super::super::{CodecError as E, CodecLimits, writer::Writer};
use wf_domain::claim::caller::{
    ClaimCallerError, ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput,
    ClaimValuesInput, normalize_claim_due,
};
use wf_domain::claim::caller::{
    ClaimPartitionMode as Partitions, ClaimReplyMode as Reply, ClaimValuesMode as Values,
    NormalizedClaimDue,
};

pub(super) struct AtomicClaimCaller<'a> {
    fields: ClaimDueInput<'a>,
    states: Vec<&'a [u8]>,
    partitions: Vec<&'a [u8]>,
    values: Vec<&'a [u8]>,
    partition_mode: Partitions,
    value_mode: Values,
}

impl AtomicClaimCaller<'_> {
    pub(super) fn normalize(&self, limits: ClaimCallerLimits) -> Result<NormalizedClaimDue<'_>, E> {
        let mut input: ClaimDueInput<'_> = self.fields;
        input.states = ClaimStatesInput::Many(&self.states);
        input.partitions = match self.partition_mode {
            Partitions::Auto => ClaimPartitionsInput::Auto,
            Partitions::Any => ClaimPartitionsInput::Any,
            Partitions::Global => ClaimPartitionsInput::Global,
            Partitions::Only => ClaimPartitionsInput::Many(&self.partitions),
        };
        input.values = match self.value_mode {
            Values::Omitted => ClaimValuesInput::Omitted,
            Values::All => ClaimValuesInput::All,
            Values::Only => ClaimValuesInput::Names(&self.values),
        };
        normalize_claim_due(input, limits).map_err(|error| match error {
            ClaimCallerError::Allocation => E::Allocation,
            ClaimCallerError::LengthOverflow => E::LengthOverflow,
            ClaimCallerError::ComponentCount => E::ItemLimit,
            ClaimCallerError::ValueTooLarge { .. } | ClaimCallerError::RawBytes { .. } => {
                E::ValueLimit
            }
            ClaimCallerError::InvalidLimits => E::Malformed,
        })
    }
}

pub(super) fn read<'a>(
    reader: &mut Reader<'a>,
    materialize: bool,
) -> Result<(AtomicClaimCaller<'a>, usize), E> {
    let mut fields = ClaimDueInput::new(reader.field()?, reader.field()?);
    let any = boolean(reader)?;
    let (states, state_count) =
        read_components(reader, materialize, any.then_some(b"ANY".as_slice()))?;
    let partition_mode = match reader.byte()? {
        0 => Partitions::Auto,
        1 => Partitions::Any,
        2 => Partitions::Global,
        3 => Partitions::Only,
        _ => return Err(E::UnsupportedTag),
    };
    let (partitions, part_count) = read_components(reader, materialize, None)?;
    let value_mode = match reader.byte()? {
        0 => Values::Omitted,
        1 => Values::All,
        2 => Values::Only,
        _ => return Err(E::UnsupportedTag),
    };
    let (values, value_count) = read_components(reader, materialize, None)?;
    fields.lease_ms = Some(reader.u64()?);
    fields.limit = Some(reader.u64()?);
    fields.priority = reader.optional(Reader::u64)?;
    fields.reclaim_expired = Some(boolean(reader)?);
    fields.reclaim_ratio = Some(reader.u64()?);
    fields.reply_mode = Some(match reader.byte()? {
        0 => Reply::Records,
        1 => Reply::Jobs,
        2 => Reply::Compact,
        3 => Reply::CompactAttrs,
        4 => Reply::CompactState,
        5 => Reply::CompactStateAttrs,
        _ => return Err(E::UnsupportedTag),
    });
    fields.payload = Some(boolean(reader)?);
    fields.payload_max_bytes = reader.optional(Reader::u64)?;
    fields.governance_limit_scope = reader.optional(Reader::field)?;
    fields.governance_shard_id = reader.optional(Reader::u64)?;
    fields.requested_now_ms = reader.optional(Reader::u64)?;
    let count = state_count
        .checked_add(part_count)
        .and_then(|n| n.checked_add(value_count))
        .ok_or(E::LengthOverflow)?;
    Ok((
        AtomicClaimCaller {
            fields,
            states,
            partitions,
            values,
            partition_mode,
            value_mode,
        },
        count,
    ))
}

fn boolean(reader: &mut Reader<'_>) -> Result<bool, E> {
    match reader.byte()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(E::UnsupportedTag),
    }
}

fn read_components<'a>(
    reader: &mut Reader<'a>,
    materialize: bool,
    alias: Option<&'static [u8]>,
) -> Result<(Vec<&'a [u8]>, usize), E> {
    let count = usize::try_from(reader.u32()?).map_err(|_| E::LengthOverflow)?;
    if count > 100_000 {
        return Err(E::ItemLimit);
    }
    if count > reader.remaining() / 4 {
        return Err(E::Malformed);
    }
    let total = count
        .checked_add(usize::from(alias.is_some()))
        .ok_or(E::LengthOverflow)?;
    let mut names = Vec::new();
    if materialize {
        names.try_reserve_exact(total).map_err(|_| E::Allocation)?;
        if let Some(alias) = alias {
            names.push(alias);
        }
    }
    for _ in 0..count {
        let name = reader.field()?;
        if materialize {
            names.push(name);
        }
    }
    Ok((names, total))
}

pub(super) fn write(
    out: &mut Writer<'_>,
    call: &NormalizedClaimDue<'_>,
    limits: CodecLimits,
) -> Result<(), E> {
    let cap = limits.admission().inline_bytes();
    out.raw(call.workflow_type(), cap)?;
    out.raw(call.worker(), cap)?;
    let (any, names) = call.states();
    out.byte(u8::from(any))?;
    components(out, names, cap)?;
    let (mode, names) = call.partitions();
    out.byte(match mode {
        Partitions::Auto => 0,
        Partitions::Any => 1,
        Partitions::Global => 2,
        Partitions::Only => 3,
    })?;
    components(out, names, cap)?;
    let (mode, names) = call.values();
    out.byte(match mode {
        Values::Omitted => 0,
        Values::All => 1,
        Values::Only => 2,
    })?;
    components(out, names, cap)?;
    out.u64(call.lease_ms())?;
    out.u64(call.limit())?;
    out.optional(call.priority().as_ref(), |w, n| w.u64(*n))?;
    out.byte(u8::from(call.reclaim_expired()))?;
    out.u64(call.reclaim_ratio())?;
    out.byte(match call.reply_mode() {
        Reply::Records => 0,
        Reply::Jobs => 1,
        Reply::Compact => 2,
        Reply::CompactAttrs => 3,
        Reply::CompactState => 4,
        Reply::CompactStateAttrs => 5,
    })?;
    out.byte(u8::from(call.payload_enabled()))?;
    out.optional(call.payload_max_bytes().as_ref(), |w, n| w.u64(*n))?;
    out.optional(call.governance_limit_scope(), |w, bytes| w.raw(bytes, cap))?;
    out.optional(call.governance_shard_id().as_ref(), |w, n| w.u64(*n))?;
    out.optional(call.requested_now_ms().as_ref(), |w, n| w.u64(*n))
}

fn components(out: &mut Writer<'_>, names: &[&[u8]], cap: usize) -> Result<(), E> {
    if names.len() > 100_000 {
        return Err(E::ItemLimit);
    }
    out.u32(u32::try_from(names.len()).map_err(|_| E::LengthOverflow)?)?;
    for name in names {
        out.raw(name, cap)?;
    }
    Ok(())
}
