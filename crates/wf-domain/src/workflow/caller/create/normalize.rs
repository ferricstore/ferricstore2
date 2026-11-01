//! Source create option order, omission rules and bounded normalized metadata.
use super::{I, OriginalCreateCaller as Caller, OriginalCreateContext as Context};
use super::{
    budget::{Budget, Failure, add, mul, source},
    number as n, options as o, schedule,
};
use crate::metadata::{
    OriginalAttributeValue, OriginalMetadataScalar, normalize_original_attributes,
    normalize_original_state_metadata_update,
};
use std::{cmp::Ordering, mem::size_of};
mod start;

pub(super) fn run<'r, 'a>(
    id: I<'r, 'a>,
    options: I<'r, 'a>,
    initial: Option<(I<'r, 'a>, I<'r, 'a>)>,
    context: Context<'r, 'a>,
    b: &mut Budget,
) -> Result<Caller<'r, 'a>, Failure> {
    let rows = if initial.is_some() {
        o::rows_with_return(options, true)?
    } else {
        o::rows(options)?
    };
    b.spend(mul(rows.len(), 128)?)?;
    if initial.is_some() {
        start::reject_create_options(rows)?;
    }
    let [id, workflow_type, state] = identity(id, rows, initial, context.internal_allowed)?;
    let mut fields = [None; 24];
    fields[0] = Some(I::Binary(id));
    fields[1] = Some(I::Binary(workflow_type));
    fields[2] = Some(I::Binary(state));
    if initial.is_some() {
        fields[2] = Some(I::Binary(b"running"));
        fields[21] = Some(I::Binary(state));
        fields[22] = Some(I::Binary(o::binary(
            o::get(rows, "worker"),
            "worker",
            true,
        )?));
        fields[23] = Some(start::lease(rows)?);
    }
    fields[3] = o::reference(rows, "parent_flow_id")?;
    let root = o::reference(rows, "root_flow_id")?;
    fields[4] = match root {
        Some(I::Binary(v)) if v == id => None,
        other => other,
    };
    fields[5] = o::reference(rows, "correlation_id")?;
    times_and_limits(rows, context, &mut fields, b, initial.is_some())?;
    let partition = partition(rows)?;
    let attributes_value = o::get(rows, "attributes").unwrap_or(I::Map(&[]));
    b.admit(attribute_bound(attributes_value)?)?;
    let attributes = normalize_original_attributes(attributes_value)?;
    let state_value = o::get(rows, "state_meta").unwrap_or(I::Map(&[]));
    let state_count = match state_value {
        I::Map(rows) if rows.len() <= 16 => rows.len(),
        _ => 0,
    };
    b.admit(mul(
        state_count,
        size_of::<(&str, OriginalMetadataScalar<'_>)>(),
    )?)?;
    let state_update = normalize_original_state_metadata_update(state_value)?;
    let schedule = if initial.is_none() {
        schedule::optional(rows, workflow_type, context.internal_allowed)?
    } else {
        None
    };
    for (index, name) in super::model::NAMES.iter().enumerate().take(20).skip(14) {
        fields[index] = o::get(rows, name);
    }
    if partition.is_none() {
        fields[20] = Some(I::Boolean(false));
    }
    let mut owned = size_of::<Caller<'_, '_>>();
    owned = add(
        owned,
        mul(
            attributes.entries.len(),
            size_of::<(&str, OriginalAttributeValue<'_>)>(),
        )?,
    )?;
    for (_, v) in &attributes.entries {
        if let OriginalAttributeValue::List(values) = v {
            owned = add(owned, mul(values.len(), size_of::<&[u8]>())?)?;
        }
    }
    owned = add(
        owned,
        mul(
            state_update.len(),
            size_of::<(&str, OriginalMetadataScalar<'_>)>(),
        )?,
    )?;
    b.output(owned)?;
    Ok(Caller {
        fields,
        partition,
        attributes,
        state_update,
        schedule,
        owned,
    })
}
fn identity<'a>(
    id: I<'_, 'a>,
    rows: &[I<'_, 'a>],
    initial: Option<(I<'_, 'a>, I<'_, 'a>)>,
    internal_allowed: bool,
) -> Result<[&'a [u8]; 3], Failure> {
    let I::Binary(id) = id else {
        return Err(source("ERR flow id must be a non-empty string"));
    };
    if id.is_empty() {
        return Err(source("ERR flow id must be a non-empty string"));
    }
    if id.starts_with(b"__ferricstore_schedule__:") && !internal_allowed {
        return Err(source("ERR flow id is reserved for internal use"));
    }
    let kind = initial.map_or_else(|| o::get(rows, "type"), |(kind, _)| Some(kind));
    let kind = o::binary(kind, "type", initial.is_none())?;
    if kind == b"__ferricstore_schedule" && !internal_allowed {
        return Err(source("ERR flow type is reserved for internal use"));
    }
    let state = initial.map_or_else(
        || o::get(rows, "state").or(Some(I::Binary(b"queued"))),
        |(_, state)| Some(state),
    );
    let state = o::binary(state, "state", false)?;
    if state == b"running" {
        return Err(source(
            "ERR flow running state is only entered by FLOW.CLAIM_DUE",
        ));
    }
    Ok([id, kind, state])
}
fn times_and_limits<'r, 'a>(
    rows: &[I<'r, 'a>],
    context: Context<'r, 'a>,
    fields: &mut [Option<I<'r, 'a>>; 24],
    b: &mut Budget,
    starting: bool,
) -> Result<(), Failure> {
    if !starting {
        fields[6] = match o::get(rows, "idempotent").unwrap_or(I::Boolean(false)) {
            I::Boolean(true) | I::Atom("true") => Some(I::Boolean(true)),
            I::Boolean(false) | I::Atom("false") => None,
            _ => return Err(source("ERR flow idempotent must be a boolean")),
        };
    }
    fields[7] = n::time(rows, "now_ms", None)?;
    if starting {
        start::deadline(fields[7], fields[23])?;
    } else {
        fields[8] = n::time(rows, "run_at_ms", fields[7])?;
    }
    fields[9] = retention(rows)?;
    fields[10] = active(rows)?;
    let hot = if n::positive(context.history_hot_maximum) {
        context.history_hot_maximum
    } else {
        I::Integer(10_000)
    };
    let total = if n::positive(context.history_maximum)
        && n::compare(context.history_maximum, I::Integer(1_000_000)) == Some(Ordering::Less)
    {
        context.history_maximum
    } else {
        I::Integer(1_000_000)
    };
    fields[11] = n::history(rows, "history_hot_max_events", hot, true, b)?;
    fields[12] = n::history(rows, "history_max_events", total, false, b)?;
    if let (Some(hot), Some(total)) = (fields[11], fields[12])
        && n::compare(hot, total) == Some(Ordering::Greater)
    {
        return Err(source(
            "ERR flow history_max_events must be greater than or equal to history_hot_max_events",
        ));
    }
    let priority = o::get(rows, "priority").unwrap_or(I::Integer(0));
    let Some(priority) = n::uint(priority).filter(|n| *n <= 2) else {
        return Err(source("ERR flow priority must be between 0 and 2"));
    };
    if priority != 0 {
        fields[13] = Some(I::Integer(i128::from(priority)));
    }
    Ok(())
}

fn retention<'r, 'a>(rows: &[I<'r, 'a>]) -> Result<Option<I<'r, 'a>>, Failure> {
    if o::get(rows, "ttl_ms").is_some() {
        return Err(source("ERR flow ttl_ms was renamed to retention_ttl_ms"));
    }
    match o::get(rows, "retention_ttl_ms") {
        None => Ok(None),
        Some(v) if n::positive(v) => Ok(Some(v)),
        _ => Err(source(
            "ERR flow retention_ttl_ms must be a positive integer",
        )),
    }
}
fn active<'r, 'a>(rows: &[I<'r, 'a>]) -> Result<Option<I<'r, 'a>>, Failure> {
    match o::get(rows, "max_active_ms") {
        None => Ok(None),
        Some(v) if o::nil(v) => Ok(Some(I::Atom("infinity"))),
        Some(I::Atom("infinity") | I::Binary(b"infinity" | b"INFINITY")) => {
            Ok(Some(I::Atom("infinity")))
        }
        Some(v)
            if n::positive(v)
                && n::compare(v, I::Integer(31_536_000_000)) != Some(Ordering::Greater) =>
        {
            Ok(Some(v))
        }
        _ => Err(source(
            "ERR flow max_active_ms must be between 1 and 31536000000 or infinity",
        )),
    }
}
fn partition<'a>(rows: &[I<'_, 'a>]) -> Result<Option<&'a [u8]>, Failure> {
    match o::get(rows, "partition_key") {
        None | Some(I::Nil | I::Atom("nil" | "global")) => Ok(None),
        Some(I::Binary(v)) if !v.is_empty() => Ok(Some(v)),
        _ => Err(source(
            "ERR flow partition_key must be a non-empty string or :global",
        )),
    }
}
fn attribute_bound(value: I<'_, '_>) -> Result<usize, super::OriginalCreateError> {
    let I::Map(rows) = value else {
        return Ok(0);
    };
    if rows.len() > 16 {
        return Ok(0);
    }
    let mut n = mul(rows.len(), size_of::<(&str, OriginalAttributeValue<'_>)>())?;
    for &(_, v) in rows {
        if let I::List(values) = v {
            n = add(n, mul(values.len().min(16), size_of::<&[u8]>())?)?;
        }
    }
    Ok(n)
}
