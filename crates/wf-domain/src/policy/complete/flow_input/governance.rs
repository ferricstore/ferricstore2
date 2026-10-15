//! Seven original governance groups followed by their independent durable cap.

use super::layout::{FlowPolicyCell as Cell, FlowPolicyLayout as Layout};
use super::value::{
    PolicyInputCursor as Cursor, PolicyInputMap as Map, classify_map, select, unsigned, visit,
};
use super::{Error, GovernancePolicyInputRejection as Gov, PolicySchemaCharge};
use crate::policy::complete::GovernanceField as Field;

mod groups;
mod lists;
mod rules;

pub(super) fn optional<'input>(
    selected: Option<Cursor<'input>>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let Some(input) = selected.filter(|value| !value.is_nil()) else {
        return Ok(Ok(usize::MAX));
    };
    visit(charge, 0)?;
    let Some(input) = classify_map(input, charge)? else {
        return Ok(Err(Gov::Shape));
    };
    let mut ids = [usize::MAX; 7];
    ids[6] = match enumeration(
        input,
        "mode",
        &["minimal", "ledger", "full"],
        Gov::Mode,
        layout,
        charge,
    )? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[4] = match groups::effects(select(input, b"effects", charge)?, layout, charge)? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[1] = match groups::audit(select(input, b"audit", charge)?, layout, charge)? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    for (name, slot, kind) in [
        (
            "limits",
            5,
            crate::policy::complete::GovernanceRuleKind::Limits,
        ),
        (
            "budgets",
            2,
            crate::policy::complete::GovernanceRuleKind::Budgets,
        ),
        (
            "circuits",
            3,
            crate::policy::complete::GovernanceRuleKind::Circuits,
        ),
    ] {
        ids[slot] = match rules::optional(
            select(input, name.as_bytes(), charge)?,
            kind,
            layout,
            charge,
        )? {
            Ok(id) => id,
            Err(reason) => return Ok(Err(reason)),
        };
    }
    ids[0] = match groups::approvals(select(input, b"approvals", charge)?, layout, charge)? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    let root = layout.map(
        &[
            "approvals",
            "audit",
            "budgets",
            "circuits",
            "effects",
            "limits",
            "mode",
        ],
        &ids,
        charge,
    )?;
    if super::etf::measure_snapshot(layout, root, charge)? > 1_048_576 {
        return Ok(Err(Gov::PolicyBytes));
    }
    Ok(Ok(root))
}

pub(super) fn enumeration<'input>(
    input: Map<'input>,
    name: &str,
    accepted: &[&'static str],
    reason: Gov,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let Some(value) = select(input, name.as_bytes(), charge)?.filter(|value| !value.is_nil())
    else {
        return Ok(Ok(usize::MAX));
    };
    visit(charge, 0)?;
    let Some(bytes) = value.key_bytes() else {
        return Ok(Err(reason));
    };
    for &token in accepted {
        visit(
            charge,
            bytes
                .len()
                .checked_add(token.len())
                .ok_or(Error::Malformed)?,
        )?;
        if if value.is_atom() {
            bytes == token.as_bytes()
        } else {
            bytes.eq_ignore_ascii_case(token.as_bytes())
        } {
            return Ok(Ok(layout.push_cell(Cell::Atom(token), charge)?));
        }
    }
    Ok(Err(reason))
}

pub(super) fn numeric<'input>(
    input: Map<'input>,
    name: &str,
    reason: Gov,
    bounds: (u64, u64),
    required: bool,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let selected = select(input, name.as_bytes(), charge)?.filter(|value| !value.is_nil());
    if selected.is_none() && !required {
        return Ok(Ok(usize::MAX));
    }
    visit(charge, 0)?;
    let Some(value) = selected else {
        return Ok(Err(reason));
    };
    let Some(value) = unsigned(value, bounds.1, charge)?.filter(|&n| n >= bounds.0) else {
        return Ok(Err(reason));
    };
    Ok(Ok(
        layout.push_cell(Cell::Integer(u128::from(value)), charge)?
    ))
}

pub(super) fn positive<'input>(
    input: Map<'input>,
    name: &str,
    field: Field,
    required: bool,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    numeric(
        input,
        name,
        Gov::Positive(field),
        (1, 9_007_199_254_740_991),
        required,
        layout,
        charge,
    )
}
