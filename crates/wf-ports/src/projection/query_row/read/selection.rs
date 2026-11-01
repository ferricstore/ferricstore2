//! Original decode-plan field/cardinality validation, independent of row names.
use super::super::{
    budget::{Budget, add},
    model::OriginalQueryRecordError as E,
};
use super::model::{
    OriginalQueryDecodePlan as Plan, OriginalQuerySelection as A, OriginalQueryStateSelection as S,
};
pub(super) fn validate(plan: Plan<'_>, b: &mut Budget) -> Result<(), E> {
    if let A::Names(names) = plan.attributes {
        names_valid(names, b)?;
    }
    if let S::Names(states) = plan.state_meta {
        if states.is_empty() || states.len() > 78 {
            return Err(E::InvalidRecord);
        }
        let mut total = 0;
        for (index, &(state, names)) in states.iter().enumerate() {
            name(state, true, 512, b)?;
            names_valid(names, b)?;
            total = add(total, names.len())?;
            b.spend(state.len().checked_mul(states.len()).ok_or(E::Limit)?)?;
            if total > 78
                || states
                    .get(..index)
                    .ok_or(E::Limit)?
                    .iter()
                    .any(|r| r.0 == state)
            {
                return Err(E::InvalidRecord);
            }
        }
    }
    Ok(())
}
fn names_valid(names: &[&[u8]], b: &mut Budget) -> Result<(), E> {
    if names.is_empty() || names.len() > 78 {
        return Err(E::InvalidRecord);
    }
    for (index, &n) in names.iter().enumerate() {
        name(n, false, 512, b)?;
        b.spend(n.len().checked_mul(names.len()).ok_or(E::Limit)?)?;
        if names.get(..index).ok_or(E::Limit)?.contains(&n) {
            return Err(E::InvalidRecord);
        }
    }
    Ok(())
}
pub(super) fn name(n: &[u8], reserved: bool, maximum: usize, b: &mut Budget) -> Result<(), E> {
    if n.is_empty() || n.len() > maximum {
        return Err(E::InvalidRecord);
    }
    b.spend(n.len().checked_mul(3).ok_or(E::Limit)?)?;
    let s = std::str::from_utf8(n).map_err(|_| E::InvalidRecord)?;
    // Reuse the exact source trim set through the existing name normalizers.
    let value = super::super::I::Binary(n);
    if maximum == 64 {
        let normalized = if reserved {
            wf_domain::metadata::normalize_original_state_metadata_name(value)
        } else {
            wf_domain::metadata::normalize_original_attribute_name(value)
        };
        if normalized.map_err(|_| E::InvalidRecord)? != s {
            return Err(E::InvalidRecord);
        }
    } else if s.trim() != s || (!reserved && n.starts_with(b"__")) {
        return Err(E::InvalidRecord);
    }
    Ok(())
}
pub(super) fn attribute(plan: A<'_>, name: &[u8], b: &mut Budget) -> Result<bool, E> {
    match plan {
        A::None => Ok(false),
        A::All => Ok(true),
        A::Names(names) => {
            b.spend(name.len().checked_mul(names.len()).ok_or(E::Limit)?)?;
            Ok(names.contains(&name))
        }
    }
}
pub(super) fn state<'a>(plan: S<'a>, state: &[u8], b: &mut Budget) -> Result<A<'a>, E> {
    match plan {
        S::None => Ok(A::None),
        S::All => Ok(A::All),
        S::Names(states) => {
            b.spend(state.len().checked_mul(states.len()).ok_or(E::Limit)?)?;
            Ok(states
                .iter()
                .find(|r| r.0 == state)
                .map_or(A::None, |r| A::Names(r.1)))
        }
    }
}
