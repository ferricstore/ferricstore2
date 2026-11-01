//! Original indexed-name normalization, with iterative nested state-key lists.
use super::{
    I,
    budget::{Budget, add},
    model::OriginalQueryRecordError as E,
    primitive,
    writer::Writer,
};
use wf_domain::metadata::{
    normalize_original_attribute_name, normalize_original_state_metadata_name,
};

pub(super) fn emit(
    attributes: I<'_, '_>,
    state: I<'_, '_>,
    out: &mut Writer,
    budget: &mut Budget,
) -> Result<(), E> {
    let mut names: [Option<&str>; 3] = [None; 3];
    let mut count = 0;
    if !primitive::nil(attributes) {
        let I::List(values) = attributes else {
            return Err(E::InvalidRecord);
        };
        for &value in values {
            pay_name(value, budget)?;
            let name = normalize_original_attribute_name(value).map_err(|_| E::InvalidRecord)?;
            budget.spend(192)?;
            if !names.contains(&Some(name)) {
                *names.get_mut(count).ok_or(E::InvalidRecord)? = Some(name);
                count += 1;
            }
        }
    }
    budget.spend(192)?;
    names.sort_unstable();
    let start = out.length;
    out.byte(u8::try_from(count).map_err(|_| E::Limit)?, budget)?;
    for name in names.into_iter().flatten() {
        name_bytes(name, out, budget)?;
    }
    let name = state_key(state, budget)?.unwrap_or("");
    name_bytes(name, out, budget)?;
    if out.length - start > 512 {
        return Err(E::InvalidRecord);
    }
    Ok(())
}
fn name_bytes(name: &str, out: &mut Writer, budget: &mut Budget) -> Result<(), E> {
    out.byte(
        u8::try_from(name.len()).map_err(|_| E::InvalidRecord)?,
        budget,
    )?;
    out.put(name.as_bytes(), budget)
}
fn pay_name(value: I<'_, '_>, budget: &mut Budget) -> Result<(), E> {
    let bytes = match value {
        I::Atom(name) => name.len(),
        I::Binary(bytes) => bytes.len(),
        _ => 5,
    };
    budget.spend(add(bytes, 1)?)
}
fn state_key<'r, 'a>(value: I<'r, 'a>, budget: &mut Budget) -> Result<Option<&'a str>, E> {
    let before = budget.live();
    let mut stack: Vec<std::slice::Iter<'r, I<'r, 'a>>> = Vec::new();
    let mut pending = Some(value);
    let mut selected = None;
    loop {
        let Some(value) = pending
            .take()
            .or_else(|| stack.last_mut().and_then(Iterator::next).copied())
        else {
            if stack.pop().is_none() {
                break;
            }
            continue;
        };
        budget.spend(1)?;
        if primitive::nil(value) || matches!(value, I::Binary(b"")) {
            continue;
        }
        if let I::List(values) = value {
            if values.is_empty() {
                continue;
            }
            budget.grow(&mut stack)?;
            stack.push(values.iter());
            continue;
        }
        pay_name(value, budget)?;
        let name = normalize_original_state_metadata_name(value).map_err(|_| E::InvalidRecord)?;
        budget.spend(64)?;
        if selected.is_some_and(|old| old != name) {
            return Err(E::InvalidRecord);
        }
        selected = Some(name);
    }
    drop(stack);
    budget.release_to(before);
    Ok(selected)
}
