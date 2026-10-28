//! Foreign ETF data records; no lookup, invocation or executable ownership.

use crate::value::original_flow::OriginalValueNode as N;
use crate::value::original_flow::prepare::OriginalValueEncodeError as E;
use crate::value::original_flow::prepare::budget::{Budget, wire_length};
use crate::value::original_flow::prepare::layout::TermView;
use crate::value::original_flow::prepare::scalar::bytes;
use crate::value::original_flow::prepare::write::{Output, scalar};

fn identity(
    view: TermView<'_, '_>,
    id: usize,
    output: &mut Output,
    budget: &mut Budget,
) -> Result<bool, E> {
    match view.node(id)? {
        N::ForeignPid {
            node,
            id,
            serial,
            creation,
            ..
        } => {
            output.byte(88, budget)?;
            scalar::atom(view, *node, output, budget)?;
            output.u32(*id, budget)?;
            output.u32(*serial, budget)?;
            output.u32(*creation, budget)?;
        }
        N::ForeignPort {
            node, id, creation, ..
        } => {
            output.byte(120, budget)?;
            scalar::atom(view, *node, output, budget)?;
            output.put(&id.to_be_bytes(), budget)?;
            output.u32(*creation, budget)?;
        }
        N::ForeignReference {
            node,
            creation,
            words,
            ..
        } => {
            let value = bytes(view.input, words)?;
            output.byte(90, budget)?;
            output.put(
                &u16::try_from(value.len() / 4)
                    .map_err(|_| E::Limit)?
                    .to_be_bytes(),
                budget,
            )?;
            scalar::atom(view, *node, output, budget)?;
            output.u32(*creation, budget)?;
            output.put(value, budget)?;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

pub(in crate::value::original_flow::prepare) fn node(
    view: TermView<'_, '_>,
    id: usize,
    output: &mut Output,
    budget: &mut Budget,
) -> Result<bool, E> {
    if identity(view, id, output, budget)? {
        return Ok(true);
    }
    match view.node(id)? {
        N::ForeignExport {
            module,
            function,
            arity,
        } => {
            output.byte(113, budget)?;
            scalar::atom(view, *module, output, budget)?;
            scalar::atom(view, *function, output, budget)?;
            scalar::integer(output, i64::from(*arity), budget)?;
        }
        N::ForeignLocalFun {
            module,
            arity,
            index,
            uniq,
            old_index,
            free,
            ..
        } => {
            output.byte(112, budget)?;
            output.u32(
                wire_length(view.plan(id)?.size.checked_sub(1).ok_or(E::Limit)?)?,
                budget,
            )?;
            output.byte(*arity, budget)?;
            output.put(uniq, budget)?;
            output.put(&index.to_be_bytes(), budget)?;
            output.u32(wire_length(free.len())?, budget)?;
            scalar::atom(view, *module, output, budget)?;
            scalar::integer(output, *old_index, budget)?;
            scalar::integer(output, i64::from(view.plan(id)?.fun_uniq), budget)?;
            let init_pid = view.init_pid.ok_or(E::UnsupportedContext { node: id })?;
            if !identity(view, init_pid, output, budget)? {
                return Err(E::InvalidInput);
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}
