//! Pinned OTP portable runtime terms and reserved local-function normalization.

use super::{
    Error, OriginalEtfSafety, PolicySchemaCharge, TermSpan, VirtualLocalFun, overflow, pay, safety,
    scalar, scan,
};

pub(super) fn parse(
    raw: &[u8],
    context: &OriginalEtfSafety<'_>,
    state: &mut scan::State,
    output: &mut scan::Output<'_>,
    frame: (usize, &mut TermSpan),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let (depth, span) = frame;
    if state.6 {
        state.7 = true;
    }
    match span.tag() {
        113 => exported(raw, context, state, output, (depth, span), charge),
        112 => local_fun(raw, context, state, output, (depth, span), charge),
        _ => node_container(raw, context, state, output, (depth, span), charge),
    }
}

fn atom_name<'context>(
    span: TermSpan,
    context: &OriginalEtfSafety<'context>,
) -> Result<&'context str, Error> {
    if !matches!(span.tag(), 100 | 115 | 118 | 119) {
        return Err(Error::Malformed);
    }
    context
        .atoms
        .get(span.datum() as usize)
        .copied()
        .ok_or(Error::Malformed)
}

fn node_container(
    raw: &[u8],
    context: &OriginalEtfSafety<'_>,
    state: &mut scan::State,
    output: &mut scan::Output<'_>,
    frame: (usize, &mut TermSpan),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let (depth, span) = frame;
    let tag = span.tag();
    let count = if matches!(tag, 90 | 114) {
        u32::from(scan::short(raw, &mut state.0, charge)?)
    } else {
        1
    };
    if count == 0 && !state.6 {
        return Err(Error::UnsupportedTag);
    }
    if count > 5 {
        return Err(Error::Malformed);
    }
    let (_, node) = scan::term(raw, context, state, output, depth + 1, charge)?;
    let name = if state.6 {
        if !matches!(node.tag(), 100 | 115 | 118 | 119) {
            return Err(Error::Malformed);
        }
        ""
    } else {
        atom_name(node, context)?
    };
    span.set_datum(node.datum());
    span.canonical = u32::try_from(state.0).map_err(|_| overflow())?;
    let creation = match tag {
        103 | 88 => {
            scan::word(raw, &mut state.0, charge)?;
            scan::word(raw, &mut state.0, charge)?;
            creation(raw, &mut state.0, tag == 103, charge)?
        }
        102 | 89 | 120 => {
            scan::take(raw, &mut state.0, if tag == 120 { 8 } else { 4 }, charge)?;
            creation(raw, &mut state.0, tag == 102, charge)?
        }
        101 => {
            scan::word(raw, &mut state.0, charge)?;
            creation(raw, &mut state.0, true, charge)?
        }
        114 | 90 => {
            let creation = creation(raw, &mut state.0, tag == 114, charge)?;
            span.canonical = u32::try_from(state.0).map_err(|_| overflow())?;
            scan::take(
                raw,
                &mut state.0,
                usize::try_from(count).map_err(|_| overflow())? * 4,
                charge,
            )?;
            creation
        }
        _ => return Err(Error::Malformed),
    };
    span.end = u32::try_from(state.0).map_err(|_| overflow())?;
    if matches!(tag, 101 | 114) && ref_word(raw, *span, 0) >= (1 << 18) {
        return Err(Error::Malformed);
    }
    let local = if state.6 {
        false
    } else {
        safety::node(context, name, creation, charge)?
    };
    if local {
        validate_local_container(raw, *span)?;
    }
    span.keys = creation;
    if matches!(tag, 101 | 114 | 90) {
        let mut retained = count;
        if local {
            let first = ref_word(raw, *span, 0);
            if first >= (1 << 18) {
                return Err(Error::Malformed);
            }
            let pid = count == 5 && ref_word(raw, *span, 1) & (1 << 16) != 0;
            if pid && ref_word(raw, *span, 3) & 0xf != 3 {
                return Err(Error::Malformed);
            }
            retained = if pid { 5 } else { 3 };
        }
        let mut significant = retained;
        while significant > 1 && ref_word(raw, *span, significant - 1) == 0 {
            significant -= 1;
        }
        span.set_datum(node.datum() | (significant << 12) | (retained << 16));
    }
    Ok(())
}

fn validate_local_container(raw: &[u8], span: TermSpan) -> Result<(), Error> {
    let invalid = match span.tag() {
        103 | 88 => ref_word(raw, span, 0) > 0x0fff_ffff,
        120 => port_id(raw, span) >= (1 << 60),
        _ => false,
    };
    if invalid {
        return Err(Error::Malformed);
    }
    Ok(())
}

fn creation(
    raw: &[u8],
    cursor: &mut usize,
    old: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<u32, Error> {
    if old {
        let value = scan::byte(raw, cursor, charge)?;
        if value > 3 {
            return Err(Error::Malformed);
        }
        Ok(u32::from(value))
    } else {
        scan::word(raw, cursor, charge)
    }
}

fn exported(
    raw: &[u8],
    context: &OriginalEtfSafety<'_>,
    state: &mut scan::State,
    output: &mut scan::Output<'_>,
    frame: (usize, &mut TermSpan),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let (depth, span) = frame;
    let (_, module) = scan::term(raw, context, state, output, depth + 1, charge)?;
    let (_, function) = scan::term(raw, context, state, output, depth + 1, charge)?;
    let (_, arity) = scan::term(raw, context, state, output, depth + 1, charge)?;
    let arity = scalar::small(raw, arity)?;
    if arity < 0 {
        return Err(Error::Malformed);
    }
    let arity = u32::from_le_bytes(
        arity.to_le_bytes()[..4]
            .try_into()
            .map_err(|_| Error::Malformed)?,
    );
    if state.6 {
        if !matches!(module.tag(), 100 | 115 | 118 | 119)
            || !matches!(function.tag(), 100 | 115 | 118 | 119)
        {
            return Err(Error::Malformed);
        }
        return Ok(());
    }
    safety::exported(
        context,
        atom_name(module, context)?,
        atom_name(function, context)?,
        arity,
        charge,
    )?;
    span.set_datum(module.datum());
    span.canonical = function.datum();
    span.keys = arity;
    Ok(())
}

fn local_fun(
    raw: &[u8],
    context: &OriginalEtfSafety<'_>,
    state: &mut scan::State,
    output: &mut scan::Output<'_>,
    frame: (usize, &mut TermSpan),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let (depth, span) = frame;
    state.3 = state.3.checked_add(1).ok_or_else(overflow)?;
    scan::word(raw, &mut state.0, charge)?; // Source compatibility ignores Size.
    let arity = scan::byte(raw, &mut state.0, charge)?;
    let uniq = scan::take(raw, &mut state.0, 16, charge)?
        .try_into()
        .map_err(|_| Error::Malformed)?;
    let index = i32::from_be_bytes(scan::word(raw, &mut state.0, charge)?.to_be_bytes());
    let free = scan::word(raw, &mut state.0, charge)?;
    if free > 255 {
        return Err(Error::Malformed);
    }
    let (_, module) = scan::term(raw, context, state, output, depth + 1, charge)?;
    if state.6 {
        if !matches!(module.tag(), 100 | 115 | 118 | 119) {
            return Err(Error::Malformed);
        }
    } else {
        atom_name(module, context)?;
    }
    let (_, old_index) = scan::term(raw, context, state, output, depth + 1, charge)?;
    scalar::small(raw, old_index)?;
    let (_, old_uniq) = scan::term(raw, context, state, output, depth + 1, charge)?;
    let old_uniq = scalar::small(raw, old_uniq)?;
    let old_uniq = i32::from_le_bytes(
        old_uniq.to_le_bytes()[..4]
            .try_into()
            .map_err(|_| Error::Malformed)?,
    );
    let (_, creator) = scan::term(raw, context, state, output, depth + 1, charge)?;
    if !matches!(creator.tag(), 103 | 88) {
        return Err(Error::Malformed);
    }
    if let Some((_, virtuals)) = output
        && !state.6
    {
        let value = VirtualLocalFun {
            module: module.datum(),
            index,
            arity,
            uniq,
            old_uniq,
        };
        span.set_datum(normalize(value, context, virtuals, charge)?);
    }
    span.keys = free;
    span.canonical = u32::try_from(state.1).map_err(|_| overflow())?;
    for _ in 0..free {
        scan::term(raw, context, state, output, depth + 1, charge)?;
    }
    Ok(())
}

fn normalize(
    value: VirtualLocalFun,
    context: &OriginalEtfSafety<'_>,
    virtuals: &mut Vec<VirtualLocalFun>,
    charge: &mut PolicySchemaCharge,
) -> Result<u32, Error> {
    pay(charge, 128)?;
    let module = context.atoms[value.module as usize];
    for (index, old) in context.local_funs.iter().enumerate() {
        safety::comparison(module.len() + 21, old.module.len() + 21, charge)?;
        if module == old.module
            && value.index == old.index
            && value.arity == old.arity
            && value.uniq == old.uniq
        {
            return Ok(0x0080_0000 | u32::try_from(index).map_err(|_| overflow())?);
        }
    }
    for (index, old) in virtuals.iter().enumerate() {
        safety::comparison(
            module.len() + 21,
            context.atoms[old.module as usize].len() + 21,
            charge,
        )?;
        if value.module == old.module
            && value.index == old.index
            && value.arity == old.arity
            && value.uniq == old.uniq
        {
            return u32::try_from(index).map_err(|_| overflow());
        }
    }
    pay(charge, 128)?;
    let index = u32::try_from(virtuals.len()).map_err(|_| overflow())?;
    virtuals.push(value);
    Ok(index)
}

pub(super) fn ref_word(raw: &[u8], span: TermSpan, word: u32) -> u32 {
    let offset = span.canonical as usize + word as usize * 4;
    if span.tag() == 101 && word != 0 {
        return 0;
    }
    if offset + 4 > span.end as usize {
        return 0;
    }
    raw.get(offset..offset + 4)
        .and_then(|v| v.try_into().ok())
        .map_or(0, u32::from_be_bytes)
}

pub(super) fn port_id(raw: &[u8], span: TermSpan) -> u64 {
    let start = span.canonical as usize;
    if span.tag() == 120 {
        u64::from_be_bytes(raw[start..start + 8].try_into().unwrap_or([0; 8]))
    } else {
        u64::from(u32::from_be_bytes(
            raw[start..start + 4].try_into().unwrap_or([0; 4]),
        ))
    }
}
