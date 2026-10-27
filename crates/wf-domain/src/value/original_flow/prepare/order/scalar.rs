//! Source exact scalar order; identity observations never grant capabilities.

use std::cmp::Ordering;

use crate::value::original_flow::OriginalValueNode as N;
use crate::value::original_flow::prepare::OriginalValueEncodeError as E;
use crate::value::original_flow::prepare::budget::{Budget, add, mul};
use crate::value::original_flow::prepare::layout::TermView;
use crate::value::original_flow::prepare::scalar::{bytes, magnitude_length};

pub(in crate::value::original_flow::prepare) enum NameChars<'a> {
    Utf8(std::str::Chars<'a>),
    Latin1(std::slice::Iter<'a, u8>),
}

impl Iterator for NameChars<'_> {
    type Item = char;
    fn next(&mut self) -> Option<char> {
        match self {
            Self::Utf8(iter) => iter.next(),
            Self::Latin1(iter) => iter.next().copied().map(char::from),
        }
    }
}

fn name<'a>(view: TermView<'a, '_>, id: usize, budget: &mut Budget) -> Result<NameChars<'a>, E> {
    let N::Atom { bytes: range, utf8 } = view.node(id)? else {
        return Err(E::InvalidInput);
    };
    let value = bytes(view.input, range)?;
    budget.spend(add(value.len(), 1)?)?;
    if *utf8 {
        Ok(NameChars::Utf8(
            std::str::from_utf8(value)
                .map_err(|_| E::InvalidInput)?
                .chars(),
        ))
    } else {
        Ok(NameChars::Latin1(value.iter()))
    }
}

pub(in crate::value::original_flow::prepare) fn atoms(
    view: TermView<'_, '_>,
    a: usize,
    b: usize,
    budget: &mut Budget,
) -> Result<Ordering, E> {
    Ok(name(view, a, budget)?.cmp(name(view, b, budget)?))
}

pub(in crate::value::original_flow::prepare) fn rank(node: &N) -> u8 {
    match node {
        N::Integer(_) | N::BigInteger { .. } => 0,
        N::FloatBits(_) => 1,
        N::Atom { .. } => 2,
        N::ForeignReference { .. } => 3,
        N::ForeignLocalFun { .. } => 4,
        N::ForeignExport { .. } => 5,
        N::ForeignPort { .. } => 6,
        N::ForeignPid { .. } => 7,
        N::Tuple { .. } => 8,
        N::Map { .. } => 9,
        N::EmptyList => 10,
        N::List { .. } => 11,
        N::Binary { .. } | N::BitString { .. } => 12,
    }
}

pub(in crate::value::original_flow::prepare) fn compare(
    view: TermView<'_, '_>,
    a: usize,
    b: usize,
    budget: &mut Budget,
) -> Result<Option<Ordering>, E> {
    let (left, right) = (view.node(a)?, view.node(b)?);
    let class = rank(left).cmp(&rank(right));
    if class != Ordering::Equal {
        return Ok(Some(class));
    }
    let result = match (left, right) {
        (N::Integer(_) | N::BigInteger { .. }, _) => integers(view, a, b, budget)?,
        (N::FloatBits(a), N::FloatBits(b)) => f64::from_bits(*a).total_cmp(&f64::from_bits(*b)),
        (N::Atom { .. }, N::Atom { .. }) => atoms(view, a, b, budget)?,
        (N::EmptyList, N::EmptyList) => Ordering::Equal,
        (N::Binary { .. } | N::BitString { .. }, _) => bitstrings(view, left, right, budget)?,
        (N::Tuple { .. } | N::List { .. } | N::Map { .. }, _) => return Ok(None),
        _ => return identities(view, a, b, budget),
    };
    Ok(Some(result))
}

fn parts(view: TermView<'_, '_>, id: usize) -> Result<(bool, usize), E> {
    match view.node(id)? {
        N::Integer(value) => Ok((*value < 0, magnitude_length(value.unsigned_abs()))),
        N::BigInteger { negative, digits } => Ok((*negative, digits.len())),
        _ => Err(E::InvalidInput),
    }
}

fn digit(view: TermView<'_, '_>, id: usize, index: usize) -> Result<u8, E> {
    match view.node(id)? {
        N::Integer(value) => value
            .unsigned_abs()
            .to_le_bytes()
            .get(index)
            .copied()
            .ok_or(E::InvalidInput),
        N::BigInteger { digits, .. } => bytes(view.input, digits)?
            .get(index)
            .copied()
            .ok_or(E::InvalidInput),
        _ => Err(E::InvalidInput),
    }
}

fn integers(
    view: TermView<'_, '_>,
    a: usize,
    b: usize,
    budget: &mut Budget,
) -> Result<Ordering, E> {
    let ((an, al), (bn, bl)) = (parts(view, a)?, parts(view, b)?);
    if an != bn {
        return Ok(if an {
            Ordering::Less
        } else {
            Ordering::Greater
        });
    }
    let mut order = al.cmp(&bl);
    if order == Ordering::Equal {
        budget.spend(add(mul(al, 2)?, 1)?)?;
        for index in (0..al).rev() {
            order = digit(view, a, index)?.cmp(&digit(view, b, index)?);
            if order != Ordering::Equal {
                break;
            }
        }
    }
    Ok(if an { order.reverse() } else { order })
}

fn bit_info<'a>(view: TermView<'a, '_>, node: &N) -> Result<(&'a [u8], usize), E> {
    let (range, last) = match node {
        N::Binary { bytes } => (bytes, 8),
        N::BitString { bytes, last_bits } => (bytes, *last_bits),
        _ => return Err(E::InvalidInput),
    };
    let value = bytes(view.input, range)?;
    let bits = mul(value.len(), 8)?
        .checked_sub(usize::from(8 - last))
        .ok_or(E::InvalidInput)?;
    Ok((value, bits))
}

fn bitstrings(
    view: TermView<'_, '_>,
    left: &N,
    right: &N,
    budget: &mut Budget,
) -> Result<Ordering, E> {
    let ((a, ab), (b, bb)) = (bit_info(view, left)?, bit_info(view, right)?);
    budget.spend(add(add(a.len(), b.len())?, 1)?)?;
    let common = ab.min(bb);
    let full = common / 8;
    let mut order = a
        .get(..full)
        .ok_or(E::InvalidInput)?
        .cmp(b.get(..full).ok_or(E::InvalidInput)?);
    if order == Ordering::Equal && common % 8 != 0 {
        let mask = u8::MAX << (8 - common % 8);
        order = (a.get(full).ok_or(E::InvalidInput)? & mask)
            .cmp(&(b.get(full).ok_or(E::InvalidInput)? & mask));
    }
    Ok(order.then(ab.cmp(&bb)))
}

fn node_order(
    view: TermView<'_, '_>,
    a: usize,
    b: usize,
    ac: u32,
    bc: u32,
    budget: &mut Budget,
) -> Result<Ordering, E> {
    Ok(atoms(view, a, b, budget)?.then(ac.cmp(&bc)))
}

fn identities(
    view: TermView<'_, '_>,
    a: usize,
    b: usize,
    budget: &mut Budget,
) -> Result<Option<Ordering>, E> {
    let order = match (view.node(a)?, view.node(b)?) {
        (
            N::ForeignPid {
                node: an,
                id: ai,
                serial: aser,
                creation: ac,
                ..
            },
            N::ForeignPid {
                node: bn,
                id: bi,
                serial: bser,
                creation: bc,
                ..
            },
        ) => {
            let fields = aser.cmp(bser).then(ai.cmp(bi));
            if fields == Ordering::Equal {
                node_order(view, *an, *bn, *ac, *bc, budget)?
            } else {
                fields
            }
        }
        (
            N::ForeignPort {
                node: an,
                id: ai,
                creation: ac,
                ..
            },
            N::ForeignPort {
                node: bn,
                id: bi,
                creation: bc,
                ..
            },
        ) => node_order(view, *an, *bn, *ac, *bc, budget)?.then(ai.cmp(bi)),
        (
            N::ForeignReference {
                node: an,
                creation: ac,
                words: aw,
                ..
            },
            N::ForeignReference {
                node: bn,
                creation: bc,
                words: bw,
                ..
            },
        ) => {
            let nodes = node_order(view, *an, *bn, *ac, *bc, budget)?;
            if nodes == Ordering::Equal {
                references(view, aw, bw, budget)?
            } else {
                nodes
            }
        }
        _ => return functions(view, a, b, budget),
    };
    Ok(Some(order))
}

fn functions(
    view: TermView<'_, '_>,
    a: usize,
    b: usize,
    budget: &mut Budget,
) -> Result<Option<Ordering>, E> {
    let order = match (view.node(a)?, view.node(b)?) {
        (
            N::ForeignExport {
                module: am,
                function: af,
                arity: aa,
            },
            N::ForeignExport {
                module: bm,
                function: bf,
                arity: ba,
            },
        ) => {
            let modules = atoms(view, *am, *bm, budget)?;
            if modules == Ordering::Equal {
                atoms(view, *af, *bf, budget)?.then(aa.cmp(ba))
            } else {
                modules
            }
        }
        (
            N::ForeignLocalFun {
                module: am,
                index: ai,
                free: af,
                ..
            },
            N::ForeignLocalFun {
                module: bm,
                index: bi,
                free: bf,
                ..
            },
        ) => {
            let modules = atoms(view, *am, *bm, budget)?;
            let value = modules
                .then(ai.cmp(bi))
                .then(view.plan(a)?.fun_uniq.cmp(&view.plan(b)?.fun_uniq))
                .then(af.len().cmp(&bf.len()));
            if value == Ordering::Equal {
                return Ok(None);
            }
            value
        }
        _ => return Err(E::InvalidInput),
    };
    Ok(Some(order))
}

fn references(
    view: TermView<'_, '_>,
    a: &std::ops::Range<usize>,
    b: &std::ops::Range<usize>,
    budget: &mut Budget,
) -> Result<Ordering, E> {
    let (a, b) = (bytes(view.input, a)?, bytes(view.input, b)?);
    budget.spend(add(add(a.len(), b.len())?, 1)?)?;
    for index in (0..a.len().max(b.len()) / 4).rev() {
        let word = |bytes: &[u8]| -> Result<u32, E> {
            let Some(slice) = bytes.get(index * 4..index * 4 + 4) else {
                return Ok(0);
            };
            Ok(u32::from_be_bytes(
                slice.try_into().map_err(|_| E::InvalidInput)?,
            ))
        };
        let order = word(a)?.cmp(&word(b)?);
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(Ordering::Equal)
}
