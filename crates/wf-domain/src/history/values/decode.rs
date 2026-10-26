//! Independent per-occurrence copies and full, nonrefundable decoder cohorts.

use std::mem::size_of;

use super::budget::{HistoryValueCharge, add, copy, mul};
use super::model::{HistoryValueError as E, HistoryValueLimits as L};
use crate::value::{
    OriginalFlowValueError, OriginalValueContext, OriginalValueDocument, decode_fixed_bytes,
    decode_original_flow_value,
};

pub(super) fn value(
    stored: &[u8],
    context: &OriginalValueContext<'_>,
    limits: &L,
    charge: &mut HistoryValueCharge,
) -> Result<Option<OriginalValueDocument>, E> {
    // Fund both fixed-helper calculations BEFORE their arrays/folds are entered:
    // 20 term visits, checked arithmetic and dispatch fit four scalar cohorts.
    charge.events(limits, 4)?;
    charge.pay(limits, 0, helper_headers()?)?;
    let fixed = decode_fixed_bytes().map_err(E::Decode)?;
    charge.pay(limits, 0, fixed)?;
    let work = limits.decoder.max_work_units;
    charge.pay(limits, work, work)?;
    let encoded = copy(stored, limits, charge)?;
    match decode_original_flow_value(encoded, context, limits.decoder) {
        Ok(document) => Ok(Some(document)),
        Err(OriginalFlowValueError::InvalidFlowValue) => Ok(None),
        Err(error) => Err(E::Decode(error)),
    }
}

fn helper_headers() -> Result<usize, E> {
    let arrays = add(
        size_of::<[(usize, usize); 14]>(),
        size_of::<[(usize, usize); 6]>(),
    )?;
    let iterators = add(
        size_of::<std::array::IntoIter<(usize, usize), 14>>(),
        size_of::<std::array::IntoIter<(usize, usize), 6>>(),
    )?;
    add(
        add(arrays, iterators)?,
        add(
            mul(8, size_of::<usize>())?,
            mul(2, size_of::<Result<usize, OriginalFlowValueError>>())?,
        )?,
    )
}
