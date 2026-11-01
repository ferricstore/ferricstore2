//! Fixed borrowed descriptor views feed the existing complete metadata plan.
use super::super::{
    I,
    budget::{Budget, add},
    model::OriginalQueryRecordError as E,
};
use wf_domain::{
    metadata::{OriginalAttributeValue as V, OriginalMetadataScalar as S},
    workflow::caller::create::OriginalCreateCaller as Caller,
};

fn scalar(value: S<'_>) -> I<'_, '_> {
    match value {
        S::Nil => I::Nil,
        S::Boolean(v) => I::Boolean(v),
        S::Integer(v) => I::Integer(i128::from(v)),
        S::Float(v) => I::Float(v),
        S::Binary(v) => I::Binary(v),
    }
}
pub(super) fn validate(
    caller: &Caller<'_, '_>,
    partition: &[u8],
    key: &[u8],
    b: &mut Budget,
) -> Result<usize, E> {
    let before = b.live();
    b.admit(
        std::mem::size_of::<[[I<'_, '_>; 16]; 16]>()
            + std::mem::size_of::<[(I<'_, '_>, I<'_, '_>); 64]>()
            + 4096,
    )?;
    let mut lists = [[I::Nil; 16]; 16];
    for (index, (_, value)) in caller.attributes().entries.iter().enumerate() {
        if let V::List(values) = value {
            for (slot, bytes) in lists[index].iter_mut().zip(values) {
                *slot = I::Binary(bytes);
            }
        }
    }
    let mut attrs = [(I::Nil, I::Nil); 16];
    for (index, ((name, value), slot)) in caller
        .attributes()
        .entries
        .iter()
        .zip(&mut attrs)
        .enumerate()
    {
        let value = match value {
            V::Scalar(s) => scalar(*s),
            V::List(values) => I::List(lists[index].get(..values.len()).ok_or(E::Limit)?),
        };
        *slot = (I::Binary(name.as_bytes()), value);
    }
    let mut entries = [(I::Nil, I::Nil); 16];
    for (slot, (name, value)) in entries.iter_mut().zip(caller.state_metadata_update()) {
        *slot = (I::Binary(name.as_bytes()), scalar(*value));
    }
    let state = caller.field("state_meta_state").unwrap_or(I::Nil);
    let meta = [(
        state,
        I::Map(
            entries
                .get(..caller.state_metadata_update().len())
                .ok_or(E::Limit)?,
        ),
    )];
    let mut names = [[b'a'; 64]; 3];
    for (i, name) in names.iter_mut().enumerate() {
        name[63] = b'1' + u8::try_from(i).map_err(|_| E::Limit)?;
    }
    let names = [
        I::Binary(&names[0]),
        I::Binary(&names[1]),
        I::Binary(&names[2]),
    ];
    let state_name = [b'm'; 64];
    let mut fields = [(I::Nil, I::Nil); 25];
    let base = base_fields(
        caller,
        partition,
        I::Map(
            attrs
                .get(..caller.attributes().entries.len())
                .ok_or(E::Limit)?,
        ),
    )?;
    for (slot, (name, value)) in fields.iter_mut().zip(base) {
        *slot = (I::Atom(name), value);
    }
    let mut length = base.len();
    if !caller.attributes().entries.is_empty() {
        fields[length] = (I::Atom("indexed_attributes"), I::List(&names));
        length += 1;
    }
    if !caller.state_metadata_update().is_empty() {
        fields[length] = (I::Atom("state_meta"), I::Map(&meta));
        length += 1;
        fields[length] = (I::Atom("indexed_state_meta"), I::Binary(&state_name));
        length += 1;
    }
    let plan = super::super::record::plan(
        I::Binary(key),
        I::Map(fields.get(..length).ok_or(E::Limit)?),
        b,
    )?;
    let length = plan.metadata_bytes;
    drop(plan);
    b.spend(add(length, 1)?)?;
    b.release_to(before);
    Ok(length)
}

fn base_fields<'r, 'a>(
    caller: &Caller<'r, 'a>,
    partition: &'a [u8],
    attributes: I<'r, 'a>,
) -> Result<[(&'static str, I<'r, 'a>); 20], E> {
    let state = caller.field("state").ok_or(E::Limit)?;
    let now = caller.field("now_ms").unwrap_or(I::Integer(0));
    let id = caller.field("id").ok_or(E::Limit)?;
    let max_active = match caller.field("max_active_ms") {
        Some(I::Atom("infinity")) | None => I::Nil,
        Some(v) => v,
    };
    let base = [
        ("id", id),
        ("type", caller.field("type").ok_or(E::Limit)?),
        ("state", state),
        ("version", I::Integer(1)),
        (
            "priority",
            caller.field("priority").unwrap_or(I::Integer(0)),
        ),
        ("partition_key", I::Binary(partition)),
        ("created_at_ms", now),
        ("updated_at_ms", now),
        ("next_run_at_ms", caller.field("run_at_ms").unwrap_or(now)),
        ("lease_deadline_ms", I::Integer(0)),
        ("attempts", I::Integer(0)),
        ("run_state", caller.field("run_state").unwrap_or(I::Nil)),
        ("max_active_ms", max_active),
        ("state_enter_seq", I::Integer(0)),
        ("history_max_events", I::Integer(0)),
        ("history_hot_max_events", I::Integer(0)),
        (
            "parent_flow_id",
            caller.field("parent_flow_id").unwrap_or(I::Nil),
        ),
        ("root_flow_id", caller.field("root_flow_id").unwrap_or(id)),
        (
            "correlation_id",
            caller.field("correlation_id").unwrap_or(I::Nil),
        ),
        ("attributes", attributes),
    ];
    Ok(base)
}
