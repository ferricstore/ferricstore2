//! Reached immutable safe tables and per-decode function identity metadata.

use crate::value::original_flow::budget::{Budget, grow_scratch};
use crate::value::original_flow::equality::scalar::names_equal;
use crate::value::original_flow::scan::{SummaryKind, TermSummary};
use crate::value::original_flow::{OriginalFlowValueError as E, OriginalValueContext as C};

pub(in crate::value::original_flow) struct ContextState {
    checked: u8,
    bindings: Vec<FunctionBinding>,
}

#[derive(Clone, Copy)]
struct FunctionBinding {
    start: usize,
    end: usize,
    utf8: bool,
    index: i32,
    arity: u8,
    uniq: [u8; 16],
    old_uniq: i32,
}

impl ContextState {
    pub(in crate::value::original_flow) fn new() -> Self {
        Self {
            checked: 0,
            bindings: Vec::new(),
        }
    }
}

pub(in crate::value::original_flow) fn valid_name(
    name: &str,
    budget: &mut Budget,
) -> Result<bool, E> {
    budget.spend(name.len())?;
    Ok(name.chars().count() <= 255)
}

pub(in crate::value::original_flow) fn validate_atoms(
    context: &C<'_>,
    state: &mut ContextState,
    budget: &mut Budget,
) -> Result<bool, E> {
    let Some(rows) = context.safe_atoms else {
        return Ok(false);
    };
    for (index, name) in rows.iter().enumerate() {
        budget.spend(1)?;
        if !valid_name(name, budget)? {
            return Ok(false);
        }
        for prior in &rows[..index] {
            budget.spend(1)?;
            if names_equal((name.as_bytes(), true), (prior.as_bytes(), true), budget)? {
                return Ok(false);
            }
        }
    }
    state.checked |= 1;
    Ok(true)
}

pub(in crate::value::original_flow) fn validate_exports(
    context: &C<'_>,
    state: &mut ContextState,
    budget: &mut Budget,
) -> Result<bool, E> {
    let Some(rows) = context.active_exports else {
        return Ok(false);
    };
    for (index, row) in rows.iter().enumerate() {
        budget.spend(1)?;
        if !valid_name(row.0, budget)? || !valid_name(row.1, budget)? {
            return Ok(false);
        }
        for prior in &rows[..index] {
            budget.spend(1)?;
            if row.2 == prior.2
                && names_equal((row.0.as_bytes(), true), (prior.0.as_bytes(), true), budget)?
                && names_equal((row.1.as_bytes(), true), (prior.1.as_bytes(), true), budget)?
            {
                return Ok(false);
            }
        }
    }
    state.checked |= 2;
    Ok(true)
}

pub(in crate::value::original_flow) fn validate_functions(
    context: &C<'_>,
    state: &mut ContextState,
    budget: &mut Budget,
) -> Result<bool, E> {
    let Some(rows) = context.local_functions else {
        return Ok(false);
    };
    for (index, row) in rows.iter().enumerate() {
        budget.spend(1)?;
        if !valid_name(row.module, budget)? {
            return Ok(false);
        }
        for prior in &rows[..index] {
            // One row and both complete 16-byte entry-key digests.
            budget.spend(33)?;
            let same_arity = row.arity == prior.arity;
            let same_uniq = row.uniq == prior.uniq;
            let same_entry = same_arity && same_uniq;
            if row.index == prior.index
                && names_equal(
                    (row.module.as_bytes(), true),
                    (prior.module.as_bytes(), true),
                    budget,
                )?
                && (same_entry || row.old_uniq == prior.old_uniq)
            {
                return Ok(false);
            }
        }
    }
    state.checked |= 4;
    Ok(true)
}

pub(in crate::value::original_flow) fn validate_local_node(
    context: &C<'_>,
    state: &mut ContextState,
    budget: &mut Budget,
) -> Result<bool, E> {
    let Some((name, _)) = context.local_node else {
        return Ok(false);
    };
    budget.spend(1)?;
    if !valid_name(name, budget)? {
        return Ok(false);
    }
    state.checked |= 8;
    Ok(true)
}

pub(in crate::value::original_flow) fn check_atom(
    context: &C<'_>,
    state: &mut ContextState,
    input: &[u8],
    term: TermSummary,
    budget: &mut Budget,
) -> Result<(), E> {
    let unsupported = E::UnsupportedContext {
        tag: term.tag,
        offset: term.offset,
    };
    if state.checked & 1 == 0 && !validate_atoms(context, state, budget)? {
        return Err(unsupported);
    }
    let SummaryKind::Atom { start, end, utf8 } = term.kind else {
        return Err(E::InvalidFlowValue);
    };
    let bytes = input.get(start..end).ok_or(E::InvalidFlowValue)?;
    let rows = context.safe_atoms.ok_or(unsupported)?;
    for name in rows {
        budget.spend(1)?;
        if names_equal((bytes, utf8), (name.as_bytes(), true), budget)? {
            return Ok(());
        }
    }
    Err(E::InvalidFlowValue)
}

pub(in crate::value::original_flow) fn check_export(
    context: &C<'_>,
    state: &mut ContextState,
    input: &[u8],
    module: TermSummary,
    function: TermSummary,
    info: (u32, (u8, usize)),
    budget: &mut Budget,
) -> Result<(), E> {
    if state.checked & 2 == 0 && !validate_exports(context, state, budget)? {
        return Err(E::UnsupportedContext {
            tag: info.1.0,
            offset: info.1.1,
        });
    }
    let (
        SummaryKind::Atom {
            start: a,
            end: ae,
            utf8: au,
        },
        SummaryKind::Atom {
            start: b,
            end: be,
            utf8: bu,
        },
    ) = (module.kind, function.kind)
    else {
        return Err(E::InvalidFlowValue);
    };
    let left = input.get(a..ae).ok_or(E::InvalidFlowValue)?;
    let right = input.get(b..be).ok_or(E::InvalidFlowValue)?;
    let rows = context.active_exports.ok_or(E::UnsupportedContext {
        tag: info.1.0,
        offset: info.1.1,
    })?;
    for row in rows {
        budget.spend(1)?;
        if row.2 == info.0
            && names_equal((left, au), (row.0.as_bytes(), true), budget)?
            && names_equal((right, bu), (row.1.as_bytes(), true), budget)?
        {
            return Ok(());
        }
    }
    Err(E::InvalidFlowValue)
}

pub(in crate::value::original_flow) fn local_node(
    context: &C<'_>,
    state: &mut ContextState,
    input: &[u8],
    atom: TermSummary,
    info: (u32, (u8, usize)),
    budget: &mut Budget,
) -> Result<bool, E> {
    if state.checked & 8 == 0 && !validate_local_node(context, state, budget)? {
        return Err(E::UnsupportedContext {
            tag: info.1.0,
            offset: info.1.1,
        });
    }
    let (name, creation) = context.local_node.ok_or(E::UnsupportedContext {
        tag: info.1.0,
        offset: info.1.1,
    })?;
    let SummaryKind::Atom { start, end, utf8 } = atom.kind else {
        return Err(E::InvalidFlowValue);
    };
    let bytes = input.get(start..end).ok_or(E::InvalidFlowValue)?;
    Ok(creation == info.0 && names_equal((bytes, utf8), (name.as_bytes(), true), budget)?)
}

pub(in crate::value::original_flow) fn local_function(
    context: &C<'_>,
    state: &mut ContextState,
    input: &[u8],
    info: (TermSummary, i32, u8, [u8; 16], i32, u8, usize),
    budget: &mut Budget,
) -> Result<i32, E> {
    if state.checked & 4 == 0 && !validate_functions(context, state, budget)? {
        return Err(E::UnsupportedContext {
            tag: info.5,
            offset: info.6,
        });
    }
    let SummaryKind::Atom { start, end, utf8 } = info.0.kind else {
        return Err(E::InvalidFlowValue);
    };
    let module = input.get(start..end).ok_or(E::InvalidFlowValue)?;
    let (_, index, arity, uniq, incoming, _, _) = info;
    let rows = context.local_functions.ok_or(E::UnsupportedContext {
        tag: info.5,
        offset: info.6,
    })?;
    let mut canonical = incoming;
    let mut captured = false;
    for row in rows {
        budget.spend(33)?;
        let same_arity = row.arity == arity;
        let same_uniq = row.uniq == uniq;
        let same_entry = same_arity && same_uniq;
        if row.index == index
            && same_entry
            && names_equal((module, utf8), (row.module.as_bytes(), true), budget)?
        {
            canonical = row.old_uniq;
            captured = true;
            break;
        }
    }
    let mut existing = false;
    for row in &state.bindings {
        budget.spend(33)?;
        let same_arity = row.arity == arity;
        let same_uniq = row.uniq == uniq;
        let same_entry = same_arity && same_uniq;
        if row.index == index
            && same_entry
            && names_equal(
                (module, utf8),
                (
                    input.get(row.start..row.end).ok_or(E::InvalidFlowValue)?,
                    row.utf8,
                ),
                budget,
            )?
        {
            canonical = row.old_uniq;
            existing = true;
            break;
        }
    }
    check_function_collisions(
        context,
        state,
        input,
        (info.0, index, arity, uniq, canonical, info.5, info.6),
        budget,
    )?;
    if !captured && !existing {
        let needed = state.bindings.len().checked_add(1).ok_or(E::Limit)?;
        grow_scratch(
            &mut state.bindings,
            needed,
            budget.limits.max_nodes.min(budget.limits.max_work_units),
            budget,
        )?;
        budget.spend(1)?;
        state.bindings.push(FunctionBinding {
            start,
            end,
            utf8,
            index,
            arity,
            uniq,
            old_uniq: canonical,
        });
    }
    Ok(canonical)
}

fn check_function_collisions(
    context: &C<'_>,
    state: &ContextState,
    input: &[u8],
    info: (TermSummary, i32, u8, [u8; 16], i32, u8, usize),
    budget: &mut Budget,
) -> Result<(), E> {
    let (term, index, arity, uniq, canonical, tag, offset) = info;
    let SummaryKind::Atom { start, end, utf8 } = term.kind else {
        return Err(E::InvalidFlowValue);
    };
    let module = input.get(start..end).ok_or(E::InvalidFlowValue)?;
    let rows = context
        .local_functions
        .ok_or(E::UnsupportedContext { tag, offset })?;
    for row in rows {
        budget.spend(33)?;
        let same_arity = row.arity == arity;
        let same_uniq = row.uniq == uniq;
        let same_entry = same_arity && same_uniq;
        if row.index == index
            && row.old_uniq == canonical
            && !same_entry
            && names_equal((module, utf8), (row.module.as_bytes(), true), budget)?
        {
            return Err(E::UnsupportedProfile { tag, offset });
        }
    }
    for row in &state.bindings {
        budget.spend(33)?;
        let same_arity = row.arity == arity;
        let same_uniq = row.uniq == uniq;
        let same_entry = same_arity && same_uniq;
        if row.index == index
            && row.old_uniq == canonical
            && !same_entry
            && names_equal(
                (module, utf8),
                (
                    input.get(row.start..row.end).ok_or(E::InvalidFlowValue)?,
                    row.utf8,
                ),
                budget,
            )?
        {
            return Err(E::UnsupportedProfile { tag, offset });
        }
    }
    Ok(())
}
