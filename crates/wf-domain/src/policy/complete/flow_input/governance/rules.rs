//! Named rule maps and the original limit, budget and circuit field order.

use super::super::{PolicySchemaError, order};
use super::{Cell, Cursor, Error, Field, Gov, Layout, PolicySchemaCharge};
use super::{classify_map, enumeration, numeric, positive, select, visit};
use crate::policy::complete::{CircuitTrackingFailure as Tracking, GovernanceRuleKind as Kind};

pub(super) fn optional<'input>(
    selected: Option<Cursor<'input>>,
    kind: Kind,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let Some(value) = selected.filter(|value| !value.is_nil()) else {
        return Ok(Ok(usize::MAX));
    };
    let Some(map) = classify_map(value, charge)? else {
        visit(charge, 0)?;
        return Ok(Err(Gov::RuleMapShape(kind)));
    };
    let (start, len) = if matches!(
        map,
        super::Map::Direct(Cursor::Frame4(_) | Cursor::Merge(_))
    ) {
        visit(charge, 0)?;
        if map.len(charge)? > 1_000 {
            return Ok(Err(Gov::RuleCount(kind)));
        }
        order::prepare_map_order_rows(map, layout, charge)?
    } else {
        order::prepare_map_order(map, layout, charge)?
    };
    if len > 1_000 {
        return Ok(Err(Gov::RuleCount(kind)));
    }
    for offset in 0..len {
        let slot = layout.work[start + offset];
        let (name, value) = map.row_at(slot.index, charge)?;
        visit(charge, 0)?;
        let Some(name) = name.binary_bytes().filter(|bytes| !bytes.is_empty()) else {
            return Ok(Err(Gov::RuleName(kind)));
        };
        if name.len() > 65_535 {
            return Ok(Err(Gov::RuleNameBytes(kind)));
        }
        let child = match rule(value, kind, layout, charge)? {
            Ok(child) => child,
            Err(reason) => return Ok(Err(reason)),
        };
        let name = layout.push_cell(Cell::Binary(name), charge)?;
        visit(charge, 0)?;
        let slot = &mut layout.work[start + offset];
        slot.cell = child;
        slot.order =
            u64::try_from(name).map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?;
    }
    let edges = layout.edges.len();
    for offset in 0..len {
        let slot = layout.work[start + offset];
        let name = usize::try_from(slot.order)
            .map_err(|_| Error::Schema(PolicySchemaError::LengthOverflow))?;
        layout.push_edge(name, charge)?;
        layout.push_edge(slot.cell, charge)?;
    }
    let root = layout.push_cell(Cell::Map { start: edges, len }, charge)?;
    order::sort_output_map(layout, edges, len, charge)?;
    layout.work.truncate(start);
    Ok(Ok(root))
}

fn rule<'input>(
    input: Cursor<'input>,
    kind: Kind,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    visit(charge, 0)?;
    let Some(map) = classify_map(input, charge)? else {
        return Ok(Err(Gov::RuleShape(kind)));
    };
    match kind {
        Kind::Limits => limit(map, layout, charge),
        Kind::Budgets => budget(map, layout, charge),
        Kind::Circuits => circuit(map, layout, charge),
    }
}

fn limit<'input>(
    map: super::Map<'input>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let mut ids = [usize::MAX; 3];
    ids[2] = match numeric(
        map,
        "limit",
        Gov::Limit,
        (0, 9_007_199_254_740_991),
        true,
        layout,
        charge,
    )? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[0] = match enforcement(map, layout, charge)? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[1] = match positive(map, "lease_size", Field::LeaseSize, false, layout, charge)? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    Ok(Ok(layout.map(
        &["enforcement", "lease_size", "limit"],
        &ids,
        charge,
    )?))
}

fn budget<'input>(
    map: super::Map<'input>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let mut ids = [usize::MAX; 5];
    ids[2] = match numeric(
        map,
        "limit",
        Gov::Limit,
        (0, 9_007_199_254_740_991),
        true,
        layout,
        charge,
    )? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[4] = match positive(map, "window_ms", Field::WindowMillis, true, layout, charge)? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[0] = match enforcement(map, layout, charge)? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[1] = match positive(map, "lease_size", Field::LeaseSize, false, layout, charge)? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    if let Some(value) = select(map, b"unit", charge)?.filter(|value| !value.is_nil()) {
        visit(charge, 0)?;
        let Some(bytes) = value
            .binary_bytes()
            .filter(|b| !b.is_empty() && b.len() <= 65_535)
        else {
            return Ok(Err(Gov::Unit));
        };
        ids[3] = layout.push_cell(Cell::Binary(bytes), charge)?;
    }
    Ok(Ok(layout.map(
        &["enforcement", "lease_size", "limit", "unit", "window_ms"],
        &ids,
        charge,
    )?))
}

fn enforcement<'input>(
    map: super::Map<'input>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    enumeration(
        map,
        "enforcement",
        &["strict_global", "approximate_global"],
        Gov::Enforcement,
        layout,
        charge,
    )
}

fn circuit<'input>(
    map: super::Map<'input>,
    layout: &mut Layout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<usize, Gov>, Error> {
    let mut ids = [usize::MAX; 9];
    for (name, field, required, index) in [
        ("failure_threshold", Field::FailureThreshold, true, 2),
        ("open_ms", Field::OpenMillis, true, 7),
        ("window_ms", Field::WindowMillis, false, 8),
        ("min_calls", Field::MinCalls, false, 6),
    ] {
        ids[index] = match positive(map, name, field, required, layout, charge)? {
            Ok(id) => id,
            Err(reason) => return Ok(Err(reason)),
        };
    }
    ids[1] = match numeric(
        map,
        "failure_rate_pct",
        Gov::Percent,
        (1, 100),
        false,
        layout,
        charge,
    )? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[5] = match positive(
        map,
        "latency_threshold_ms",
        Field::LatencyMillis,
        false,
        layout,
        charge,
    )? {
        Ok(id) => id,
        Err(reason) => return Ok(Err(reason)),
    };
    ids[0] =
        match super::lists::optional(map, "error_classes", Field::ErrorClasses, layout, charge)? {
            Ok(id) => id,
            Err(reason) => return Ok(Err(reason)),
        };
    for (name, field, index) in [
        ("half_open_max_probes", Field::HalfOpenProbes, 3),
        ("half_open_success_threshold", Field::HalfOpenSuccesses, 4),
    ] {
        ids[index] = match positive(map, name, field, false, layout, charge)? {
            Ok(id) => id,
            Err(reason) => return Ok(Err(reason)),
        };
    }
    if let Some(reason) = tracking(&ids, layout)? {
        return Ok(Err(Gov::Tracking(reason)));
    }
    Ok(Ok(layout.map(
        &[
            "error_classes",
            "failure_rate_pct",
            "failure_threshold",
            "half_open_max_probes",
            "half_open_success_threshold",
            "latency_threshold_ms",
            "min_calls",
            "open_ms",
            "window_ms",
        ],
        &ids,
        charge,
    )?))
}

fn tracking(ids: &[usize; 9], layout: &Layout<'_>) -> Result<Option<Tracking>, Error> {
    let threshold = scalar(ids[2], layout)?.ok_or(Error::Malformed)?;
    let minimum = scalar(ids[6], layout)?;
    Ok(if minimum.is_some_and(|n| n > 64) {
        Some(Tracking::MinCallsAbove64)
    } else if threshold > 64 && ids[1] == usize::MAX {
        Some(Tracking::ThresholdAbove64WithoutRate)
    } else if threshold > 64 && minimum.is_none() {
        Some(Tracking::MissingMinCallsForLargeThreshold)
    } else {
        None
    })
}

fn scalar(index: usize, layout: &Layout<'_>) -> Result<Option<u128>, Error> {
    if index == usize::MAX {
        return Ok(None);
    }
    match layout.cells.get(index) {
        Some(Cell::Integer(value)) => Ok(Some(*value)),
        _ => Err(Error::Malformed),
    }
}
