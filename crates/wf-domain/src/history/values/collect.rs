//! Stable captured-binary collection, without key parsing or storage access.

use super::budget::{HistoryValueCharge, add, mul, reserve};
use super::model::{
    HistoryValueEntry, HistoryValueError as E, HistoryValueLimits as L, HistoryValuePlan,
    HistoryValueReference as R, HistoryValueReturn,
};

pub(super) fn prepare<'a, T: ?Sized>(
    history: &'a [HistoryValueEntry<'a, T>],
    value_return: HistoryValueReturn,
    limits: L,
) -> Result<HistoryValuePlan<'a, T>, E> {
    let mut charge = HistoryValueCharge::new::<T>(&limits)?;
    charge.events(&limits, 1)?;
    let (references, threshold) = match value_return {
        HistoryValueReturn::Disabled => (Vec::new(), None),
        HistoryValueReturn::Enabled { .. } if history.is_empty() => (Vec::new(), None),
        HistoryValueReturn::Enabled { max_bytes } => {
            if history.len() > limits.maximum_entries {
                return Err(E::Limit);
            }
            let threshold = threshold(max_bytes)?;
            let references = collect(history, &limits, &mut charge)?;
            (references, Some(threshold))
        }
    };
    Ok(HistoryValuePlan {
        history,
        references,
        threshold,
        value_return,
        limits,
        charge,
    })
}

fn threshold(maximum: i128) -> Result<usize, E> {
    if maximum < 1 {
        return Ok(1);
    }
    usize::try_from(maximum)
        .map_err(|_| E::Limit)?
        .checked_add(1)
        .ok_or(E::Limit)
}

fn collect<'a, T: ?Sized>(
    history: &[HistoryValueEntry<'a, T>],
    limits: &L,
    charge: &mut HistoryValueCharge,
) -> Result<Vec<&'a [u8]>, E> {
    let mut seen = Vec::new();
    // Entry/three reference dispatches are funded before iterator entry.
    charge.events(limits, mul(history.len(), 4)?)?;
    for entry in history {
        for reference in entry.references {
            if let R::Binary(bytes) = reference {
                remember(&mut seen, bytes, limits, charge)?;
            }
        }
    }
    let mut count = 0;
    charge.events(limits, seen.len())?;
    for reference in &seen {
        if eligible(reference) {
            count = add(count, 1)?;
            if count > limits.maximum_references {
                return Err(E::Limit);
            }
        }
    }
    let mut references = Vec::new();
    reserve(&mut references, count, limits, charge)?;
    charge.events(limits, seen.len())?;
    for reference in seen {
        if eligible(reference) {
            references.push(reference);
        }
    }
    Ok(references)
}

fn remember<'a>(
    seen: &mut Vec<&'a [u8]>,
    bytes: &'a [u8],
    limits: &L,
    charge: &mut HistoryValueCharge,
) -> Result<(), E> {
    charge.events(limits, add(1, seen.len())?)?;
    for prior in seen.iter() {
        charge.compare(limits, prior.len(), bytes.len())?;
        if *prior == bytes {
            return Ok(());
        }
    }
    let target = add(seen.len(), 1)?;
    reserve(seen, target, limits, charge)?;
    seen.push(bytes);
    Ok(())
}

fn eligible(reference: &[u8]) -> bool {
    !reference.is_empty() && reference.len() <= 65_535
}
