//! Logical value preparation, distinct from already-stored byte ingress.

mod budget;
mod layout;
mod model;
mod order;
mod scalar;
mod write;

pub use model::{
    OriginalValueEncodeError, OriginalValueEncodeLimits, OriginalValueInput,
    PreparedOriginalFlowValue,
};

/// Prepare a logical value as complete source-compatible stored bytes.
///
/// The input borrows a flat postorder arena rather than encoded ETF. Successful
/// output owns its bytes and carries no storage, runtime or admission authority.
///
/// # Errors
///
/// Invalid logical data, invalid profiles, resource refusal and actual failed
/// reservations remain distinct. Limits cover all supplied arenas, continuous
/// work and prospectively requested ownership, including serialized expansion.
pub fn prepare_original_flow_value(
    input: OriginalValueInput<'_>,
    limits: OriginalValueEncodeLimits,
) -> Result<PreparedOriginalFlowValue, OriginalValueEncodeError> {
    let mut budget = budget::Budget::new(limits)?;
    let fixed = fixed_bytes()?;
    budget.spend(fixed)?;
    budget.admit(fixed)?;
    let layout = layout::build(input, &mut budget)?;
    let bytes = write::run(&layout, &mut budget)?;
    Ok(PreparedOriginalFlowValue {
        bytes,
        work_units: budget.work,
        temporary_bytes: budget.peak,
    })
}

/// Prepare with an explicitly captured encoder initialization PID node.
///
/// The PID belongs to the borrowed input arena and must precede reached local
/// functions. It supplies data, never a runtime capability or global lookup.
///
/// # Errors
///
/// This rejecting TDD facade reports unavailable context until implemented.
pub fn prepare_original_flow_value_with_init_pid(
    input: OriginalValueInput<'_>,
    _init_pid: usize,
    _limits: OriginalValueEncodeLimits,
) -> Result<PreparedOriginalFlowValue, OriginalValueEncodeError> {
    Err(OriginalValueEncodeError::UnsupportedContext { node: input.root })
}

fn fixed_bytes() -> Result<usize, OriginalValueEncodeError> {
    use std::mem::size_of;
    let parts = [
        (size_of::<OriginalValueInput<'static>>(), 6),
        (size_of::<OriginalValueEncodeLimits>(), 3),
        (size_of::<budget::Budget>(), 1),
        (size_of::<layout::Layout<'static>>(), 1),
        (size_of::<layout::TermView<'static, 'static>>(), 8),
        (size_of::<layout::NodePlan>(), 4),
        (size_of::<layout::ListInfo>(), 3),
        (size_of::<order::CompareFrame>(), 4),
        (size_of::<write::EmitFrame>(), 4),
        (size_of::<order::sequence::ListCursor>(), 4),
        (size_of::<order::sequence::ListStep>(), 2),
        (size_of::<order::scalar::NameChars<'static>>(), 2),
        (size_of::<write::Output>(), 1),
        (size_of::<Vec<order::CompareFrame>>(), 1),
        (size_of::<Vec<write::EmitFrame>>(), 1),
        (size_of::<PreparedOriginalFlowValue>(), 1),
        (
            size_of::<Result<PreparedOriginalFlowValue, OriginalValueEncodeError>>(),
            1,
        ),
        (size_of::<OriginalValueEncodeError>(), 4),
        (size_of::<std::ops::Range<usize>>(), 8),
        (size_of::<[u8; 32]>(), 4),
        (size_of::<usize>(), 32),
    ];
    parts.into_iter().try_fold(0, |total, (size, count)| {
        budget::add(total, budget::mul(size, count)?)
    })
}
