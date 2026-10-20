//! Compound spans, transparent aliases and source envelope classification.

use super::{Error, OriginalEtfSafety, PolicySchemaCharge, TermSpan, overflow, pay, scan};

pub(super) fn parse(
    raw: &[u8],
    safety: &OriginalEtfSafety<'_>,
    state: &mut scan::State,
    output: &mut scan::Output<'_>,
    frame: (usize, &mut TermSpan),
    charge: &mut PolicySchemaCharge,
) -> Result<Option<TermSpan>, Error> {
    let (depth, span) = frame;
    match span.tag() {
        106 => {}
        107 => {
            let length = scan::short(raw, &mut state.0, charge)?;
            span.set_datum(u32::from(length));
            span.canonical = u32::try_from(state.0).map_err(|_| overflow())?;
            scan::take(raw, &mut state.0, usize::from(length), charge)?;
        }
        104 | 105 | 116 | 108 => {
            let count = if span.tag() == 104 {
                u32::from(scan::byte(raw, &mut state.0, charge)?)
            } else {
                scan::word(raw, &mut state.0, charge)?
            };
            if span.tag() == 116 {
                state.2 = state
                    .2
                    .checked_add(usize::try_from(count).map_err(|_| overflow())?)
                    .ok_or_else(overflow)?;
                span.set_datum(count);
            }
            span.keys = count;
            if matches!(span.tag(), 104 | 105) {
                span.canonical = u32::try_from(state.1).map_err(|_| overflow())?;
            }
            if span.tag() == 108 {
                span.set_datum(count);
            }
            let terms = if span.tag() == 116 {
                u64::from(count) * 2
            } else {
                u64::from(count)
            };
            let root = state.6 && usize::try_from(span.start).ok() == Some(state.8.0);
            for ordinal in 0..terms {
                let (_, child) = scan::term(raw, safety, state, output, depth + 1, charge)?;
                if root && matches!(span.tag(), 104 | 105) && count == 3 {
                    state.8.1[usize::try_from(ordinal).map_err(|_| overflow())? + 1] = child;
                }
            }
            if span.tag() == 108 {
                if root && count == 0 {
                    state.8.0 = state.0;
                }
                let (tail, meaning) = scan::term(raw, safety, state, output, depth + 1, charge)?;
                span.keys = tail;
                if count == 0 {
                    span.canonical = tail;
                    return Ok(Some(meaning));
                }
            }
        }
        _ => return Err(Error::Malformed),
    }
    Ok(None)
}

pub(super) fn resolve(spans: &[TermSpan], mut index: u32) -> u32 {
    while spans[index as usize].tag() == 108 && spans[index as usize].datum() == 0 {
        index = spans[index as usize].canonical;
    }
    index
}

pub(super) fn envelope(
    raw: &[u8],
    spans: &[TermSpan],
    safety: &OriginalEtfSafety<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<(u64, u32)>, Error> {
    let root = resolve(spans, 0);
    let tuple = spans[root as usize];
    let mut facts = [
        tuple,
        TermSpan::default(),
        TermSpan::default(),
        TermSpan::default(),
    ];
    let mut policy = 0;
    if matches!(tuple.tag(), 104 | 105) && tuple.keys == 3 {
        facts[1] = spans[resolve(spans, root + 1) as usize];
        let generation = spans[root as usize + 1].next;
        facts[2] = spans[resolve(spans, generation) as usize];
        policy = spans[generation as usize].next;
        facts[3] = spans[resolve(spans, policy) as usize];
    }
    Ok(envelope_facts(raw, facts, safety, charge)?.map(|generation| (generation, policy)))
}

pub(super) fn envelope_facts(
    raw: &[u8],
    facts: [TermSpan; 4],
    safety: &OriginalEtfSafety<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<u64>, Error> {
    pay(charge, 128)?;
    let [tuple, name, generation, policy] = facts;
    if !matches!(tuple.tag(), 104 | 105) || tuple.keys != 3 {
        return Ok(None);
    }
    let actual = if matches!(name.tag(), 100 | 115 | 118 | 119) {
        safety.atoms.get(name.datum() as usize).copied()
    } else {
        None
    };
    pay(
        charge,
        128 + u64::try_from(actual.map_or(0, str::len)).map_err(|_| overflow())? + 14,
    )?;
    if actual != Some("flow_policy_v1") {
        return Ok(None);
    }
    pay(charge, 128)?;
    let Some(generation) =
        super::scalar::signed(raw, generation).and_then(|n| u64::try_from(n).ok())
    else {
        return Ok(None);
    };
    if generation > 9_007_199_254_740_991 {
        return Ok(None);
    }
    pay(charge, 128)?;
    if policy.tag() != 116 {
        return Ok(None);
    }
    Ok(Some(generation))
}
