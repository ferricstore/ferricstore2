//! Scalar and foreign DATA equality without encoded-tag or float coercion.

use crate::value::original_flow::budget::Budget;
use crate::value::original_flow::{
    OriginalFlowValueError as E, OriginalValueDocument as D, OriginalValueNode as N,
};

pub(in crate::value::original_flow) fn scalar_equal(
    document: &D,
    left: &N,
    right: &N,
    budget: &mut Budget,
) -> Result<bool, E> {
    match (left, right) {
        (N::EmptyList, N::EmptyList) => Ok(true),
        (N::Integer(_) | N::BigInteger { .. }, N::Integer(_) | N::BigInteger { .. }) => {
            integers_equal(document, left, right, budget)
        }
        (N::FloatBits(a), N::FloatBits(b)) => Ok(a == b),
        (N::Atom { bytes: a, utf8: au }, N::Atom { bytes: b, utf8: bu }) => names_equal(
            (
                document.encoded.get(a.clone()).ok_or(E::InvalidFlowValue)?,
                *au,
            ),
            (
                document.encoded.get(b.clone()).ok_or(E::InvalidFlowValue)?,
                *bu,
            ),
            budget,
        ),
        (N::Binary { .. } | N::BitString { .. }, N::Binary { .. } | N::BitString { .. }) => {
            bitstrings_equal(document, left, right, budget)
        }
        _ => foreign_equal(document, left, right, budget),
    }
}

pub(in crate::value::original_flow) fn names_equal(
    left: (&[u8], bool),
    right: (&[u8], bool),
    budget: &mut Budget,
) -> Result<bool, E> {
    let a = if left.1 {
        budget.spend(left.0.len())?;
        Some(std::str::from_utf8(left.0).map_err(|_| E::InvalidFlowValue)?)
    } else {
        None
    };
    let b = if right.1 {
        budget.spend(right.0.len())?;
        Some(std::str::from_utf8(right.0).map_err(|_| E::InvalidFlowValue)?)
    } else {
        None
    };
    budget.spend(left.0.len().checked_add(right.0.len()).ok_or(E::Limit)?)?;
    let (mut au, mut bu) = (a.map(str::chars), b.map(str::chars));
    let (mut al, mut bl) = (left.0.iter(), right.0.iter());
    let mut equal = true;
    loop {
        let a = if let Some(iter) = &mut au {
            iter.next()
        } else {
            al.next().map(|n| char::from(*n))
        };
        let b = if let Some(iter) = &mut bu {
            iter.next()
        } else {
            bl.next().map(|n| char::from(*n))
        };
        match (a, b) {
            (None, None) => return Ok(equal),
            (Some(a), Some(b)) => equal &= a == b,
            _ => equal = false,
        }
    }
}

pub(in crate::value::original_flow) fn integers_equal(
    document: &D,
    left: &N,
    right: &N,
    budget: &mut Budget,
) -> Result<bool, E> {
    match (left, right) {
        (N::Integer(a), N::Integer(b)) => Ok(a == b),
        (
            N::BigInteger {
                negative: an,
                digits: a,
            },
            N::BigInteger {
                negative: bn,
                digits: b,
            },
        ) => {
            if an != bn || a.len() != b.len() {
                return Ok(false);
            }
            let a = document.encoded.get(a.clone()).ok_or(E::InvalidFlowValue)?;
            let b = document.encoded.get(b.clone()).ok_or(E::InvalidFlowValue)?;
            budget.spend(a.len().checked_add(b.len()).ok_or(E::Limit)?)?;
            let mut equal = true;
            for (a, b) in a.iter().zip(b) {
                equal &= a == b;
            }
            Ok(equal)
        }
        _ => Ok(false),
    }
}

pub(in crate::value::original_flow) fn bitstrings_equal(
    document: &D,
    left: &N,
    right: &N,
    budget: &mut Budget,
) -> Result<bool, E> {
    let (a, ab) = match left {
        N::Binary { bytes } => (bytes, 8),
        N::BitString { bytes, last_bits } => (bytes, *last_bits),
        _ => return Ok(false),
    };
    let (b, bb) = match right {
        N::Binary { bytes } => (bytes, 8),
        N::BitString { bytes, last_bits } => (bytes, *last_bits),
        _ => return Ok(false),
    };
    if a.len() != b.len() || ab != bb {
        return Ok(false);
    }
    if ab == 0 || ab > 8 {
        return Err(E::InvalidFlowValue);
    }
    let a = document.encoded.get(a.clone()).ok_or(E::InvalidFlowValue)?;
    let b = document.encoded.get(b.clone()).ok_or(E::InvalidFlowValue)?;
    budget.spend(a.len().checked_add(b.len()).ok_or(E::Limit)?)?;
    let mut equal = true;
    let length = a.len();
    for (index, (a, b)) in a.iter().zip(b).enumerate() {
        let mask = if index + 1 == length {
            u8::MAX << (8 - ab)
        } else {
            u8::MAX
        };
        equal &= a & mask == b & mask;
    }
    Ok(equal)
}

pub(in crate::value::original_flow) fn foreign_equal(
    document: &D,
    left: &N,
    right: &N,
    budget: &mut Budget,
) -> Result<bool, E> {
    let (a, b, fields) = match (left, right) {
        (
            N::ForeignPid {
                node: a,
                id: ai,
                serial: aser,
                creation: ac,
                ..
            },
            N::ForeignPid {
                node: b,
                id: bi,
                serial: bser,
                creation: bc,
                ..
            },
        ) => (*a, *b, (ai, aser, ac) == (bi, bser, bc)),
        (
            N::ForeignPort {
                node: a,
                id: ai,
                creation: ac,
                ..
            },
            N::ForeignPort {
                node: b,
                id: bi,
                creation: bc,
                ..
            },
        ) => (*a, *b, (ai, ac) == (bi, bc)),
        (N::ForeignReference { node: a, .. }, N::ForeignReference { node: b, .. }) => {
            (*a, *b, references_equal(document, left, right, budget)?)
        }
        (N::ForeignExport { .. }, N::ForeignExport { .. })
        | (N::ForeignLocalFun { .. }, N::ForeignLocalFun { .. }) => {
            return functions_equal(document, left, right, budget);
        }
        _ => return Ok(false),
    };
    if !fields {
        return Ok(false);
    }
    budget.spend(2)?;
    match (document.nodes.get(a), document.nodes.get(b)) {
        (Some(N::Atom { bytes: a, utf8: au }), Some(N::Atom { bytes: b, utf8: bu })) => {
            names_equal(
                (
                    document.encoded.get(a.clone()).ok_or(E::InvalidFlowValue)?,
                    *au,
                ),
                (
                    document.encoded.get(b.clone()).ok_or(E::InvalidFlowValue)?,
                    *bu,
                ),
                budget,
            )
        }
        _ => Err(E::InvalidFlowValue),
    }
}

pub(in crate::value::original_flow) fn references_equal(
    document: &D,
    left: &N,
    right: &N,
    budget: &mut Budget,
) -> Result<bool, E> {
    let (
        N::ForeignReference {
            creation: ac,
            words: a,
            ..
        },
        N::ForeignReference {
            creation: bc,
            words: b,
            ..
        },
    ) = (left, right)
    else {
        return Ok(false);
    };
    if ac != bc {
        return Ok(false);
    }
    let mut a = document.encoded.get(a.clone()).ok_or(E::InvalidFlowValue)?;
    let mut b = document.encoded.get(b.clone()).ok_or(E::InvalidFlowValue)?;
    if a.is_empty() || b.is_empty() || a.len() % 4 != 0 || b.len() % 4 != 0 {
        return Err(E::InvalidFlowValue);
    }
    for words in [&mut a, &mut b] {
        while words.len() > 4 {
            budget.spend(4)?;
            let last = words.get(words.len() - 4..).ok_or(E::InvalidFlowValue)?;
            if last.iter().any(|byte| *byte != 0) {
                break;
            }
            let current = *words;
            *words = current
                .get(..current.len() - 4)
                .ok_or(E::InvalidFlowValue)?;
        }
    }
    if a.len() != b.len() {
        return Ok(false);
    }
    budget.spend(a.len().checked_add(b.len()).ok_or(E::Limit)?)?;
    let mut equal = true;
    for (a, b) in a.iter().zip(b) {
        equal &= a == b;
    }
    Ok(equal)
}

pub(in crate::value::original_flow) fn functions_equal(
    document: &D,
    left: &N,
    right: &N,
    budget: &mut Budget,
) -> Result<bool, E> {
    let pairs = match (left, right) {
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
            if aa != ba {
                return Ok(false);
            }
            [(*am, *bm), (*af, *bf)]
        }
        (
            N::ForeignLocalFun {
                module: a,
                index: ai,
                old_uniq: au,
                free: af,
                ..
            },
            N::ForeignLocalFun {
                module: b,
                index: bi,
                old_uniq: bu,
                free: bf,
                ..
            },
        ) => {
            if (ai, au, af.len()) != (bi, bu, bf.len()) {
                return Ok(false);
            }
            [(*a, *b), (*a, *b)]
        }
        _ => return Ok(false),
    };
    for (index, (a, b)) in pairs.into_iter().enumerate() {
        if index == 1 && matches!(left, N::ForeignLocalFun { .. }) {
            break;
        }
        budget.spend(2)?;
        let (Some(N::Atom { bytes: a, utf8: au }), Some(N::Atom { bytes: b, utf8: bu })) =
            (document.nodes.get(a), document.nodes.get(b))
        else {
            return Err(E::InvalidFlowValue);
        };
        if !names_equal(
            (
                document.encoded.get(a.clone()).ok_or(E::InvalidFlowValue)?,
                *au,
            ),
            (
                document.encoded.get(b.clone()).ok_or(E::InvalidFlowValue)?,
                *bu,
            ),
            budget,
        )? {
            return Ok(false);
        }
    }
    Ok(true)
}
