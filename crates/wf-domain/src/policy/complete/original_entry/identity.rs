//! Fallible exact cell comparisons and bottom-up auxiliary map-key ordering.

use std::cmp::Ordering;

use super::{
    Error, OriginalEtfSafety, PolicySchemaCharge, TermPosition as Position, TermSpan,
    VirtualLocalFun, compound, overflow, pay, runtime, scalar,
};

pub(super) type IdentityInput<'raw, 'index, 'context> = (
    &'raw [u8],
    &'index [TermSpan],
    &'index [u32],
    &'context OriginalEtfSafety<'context>,
    &'index [VirtualLocalFun],
);

pub(super) fn maps(
    raw: &[u8],
    spans: &mut [TermSpan],
    keys: &mut Vec<u32>,
    safety: &OriginalEtfSafety<'_>,
    funs: &[VirtualLocalFun],
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    for index in (0..spans.len()).rev() {
        if spans[index].tag() != 116 {
            continue;
        }
        pay(charge, 128)?;
        let start = keys.len();
        let count = spans[index].datum() as usize;
        spans[index].keys = u32::try_from(start).map_err(|_| overflow())?;
        let mut key = u32::try_from(index + 1).map_err(|_| overflow())?;
        for _ in 0..count {
            pay(charge, 128)?;
            keys.push(key);
            key = spans[spans[key as usize].next as usize].next;
        }
        for root in (0..count / 2).rev() {
            sift(
                (raw, spans, safety, funs),
                keys,
                (start, root, count),
                charge,
            )?;
        }
        for end in (1..count).rev() {
            keys.swap(start, start + end);
            sift((raw, spans, safety, funs), keys, (start, 0, end), charge)?;
        }
        for pair in 1..count {
            if compare(
                (raw, spans, keys, safety, funs),
                Position::Encoded(keys[start + pair - 1]),
                Position::Encoded(keys[start + pair]),
                charge,
            )? == Ordering::Equal
            {
                return Err(Error::Malformed);
            }
        }
    }
    Ok(())
}

fn sift(
    input: (
        &[u8],
        &[TermSpan],
        &OriginalEtfSafety<'_>,
        &[VirtualLocalFun],
    ),
    keys: &mut [u32],
    range: (usize, usize, usize),
    charge: &mut PolicySchemaCharge,
) -> Result<(), Error> {
    let (raw, spans, safety, funs) = input;
    let (start, mut root, count) = range;
    while root * 2 + 1 < count {
        let mut child = root * 2 + 1;
        if child + 1 < count
            && compare(
                (raw, spans, keys, safety, funs),
                Position::Encoded(keys[start + child]),
                Position::Encoded(keys[start + child + 1]),
                charge,
            )? == Ordering::Less
        {
            child += 1;
        }
        if compare(
            (raw, spans, keys, safety, funs),
            Position::Encoded(keys[start + root]),
            Position::Encoded(keys[start + child]),
            charge,
        )? != Ordering::Less
        {
            break;
        }
        keys.swap(start + root, start + child);
        root = child;
    }
    Ok(())
}

fn normalized(input: IdentityInput<'_, '_, '_>, position: Position) -> Position {
    if let Position::Encoded(index) = position {
        let index = compound::resolve(input.1, index);
        let span = input.1[index as usize];
        match span.tag() {
            106 => Position::Nil,
            107 if span.datum() == 0 => Position::Nil,
            107 => Position::StringCell {
                span: index,
                offset: 0,
            },
            108 => Position::StringCell {
                span: index,
                offset: index + 1,
            },
            _ => Position::Encoded(index),
        }
    } else {
        position
    }
}

fn width(input: IdentityInput<'_, '_, '_>, position: Position) -> usize {
    match position {
        Position::Nil | Position::StringCell { .. } => 0,
        Position::Byte(_) => 2,
        Position::Encoded(index) => {
            let span = input.1[index as usize];
            match scalar::kind(input.1, position) {
                0 => input.3.atoms[span.datum() as usize].len(),
                1 => scalar::integer_length(input.0, span) + 1,
                2 => 8,
                3 => 4 + span.keys.div_ceil(8) as usize,
                6 | 7 => 4,
                8 | 9 => input.3.atoms[span.datum() as usize].len() + 12,
                10 => {
                    input.3.atoms[(span.datum() & 0xfff) as usize].len()
                        + 8
                        + ((span.datum() >> 12) & 15) as usize * 4
                }
                11 => fun_fields(input, span).0.len() + 12,
                12 => {
                    input.3.atoms[span.datum() as usize].len()
                        + input.3.atoms[span.canonical as usize].len()
                        + 4
                }
                _ => 0,
            }
        }
    }
}

pub(super) fn compare(
    input: IdentityInput<'_, '_, '_>,
    mut left: Position,
    mut right: Position,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    loop {
        left = normalized(input, left);
        right = normalized(input, right);
        let size = width(input, left)
            .checked_add(width(input, right))
            .ok_or_else(overflow)?;
        pay(charge, 128 + u64::try_from(size).map_err(|_| overflow())?)?;
        let left_kind = scalar::kind(input.1, left);
        let right_kind = scalar::kind(input.1, right);
        if left_kind != right_kind {
            return Ok(left_kind.cmp(&right_kind));
        }
        if left_kind == 5 {
            let (head_left, tail_left) = cons(input, left);
            let (head_right, tail_right) = cons(input, right);
            let order = compare(input, head_left, head_right, charge)?;
            if order != Ordering::Equal {
                return Ok(order);
            }
            left = tail_left;
            right = tail_right;
            continue;
        }
        if left_kind == 4 {
            return Ok(Ordering::Equal);
        }
        if left_kind == 1 {
            return Ok(scalar::integer_cmp(input.0, input.1, left, right));
        }
        let (Position::Encoded(a), Position::Encoded(b)) = (left, right) else {
            return Err(Error::Malformed);
        };
        let (a, b) = (input.1[a as usize], input.1[b as usize]);
        return match left_kind {
            0 => Ok(input.3.atoms[a.datum() as usize].cmp(input.3.atoms[b.datum() as usize])),
            2 => Ok(
                f64::from_bits((u64::from(a.canonical) << 32) | u64::from(a.keys)).total_cmp(
                    &f64::from_bits((u64::from(b.canonical) << 32) | u64::from(b.keys)),
                ),
            ),
            3 => Ok(scalar::binary_cmp(input.0, a, b)),
            6 => sequence(input, a, b, false, charge),
            7 => sequence(input, a, b, true, charge),
            8 | 9 | 10 | 12 => Ok(runtime_cmp(input, a, b, left_kind)),
            11 => local_fun_cmp(input, a, b, charge),
            _ => Err(Error::Malformed),
        };
    }
}

fn cons(input: IdentityInput<'_, '_, '_>, position: Position) -> (Position, Position) {
    let Position::StringCell { span, offset } = position else {
        return (Position::Nil, Position::Nil);
    };
    let node = input.1[span as usize];
    if node.tag() == 107 {
        let head = Position::Byte(input.0[(node.canonical + offset) as usize]);
        let tail = if offset + 1 == node.datum() {
            Position::Nil
        } else {
            Position::StringCell {
                span,
                offset: offset + 1,
            }
        };
        (head, tail)
    } else {
        let next = input.1[offset as usize].next;
        let tail = if next == node.keys {
            Position::Encoded(next)
        } else {
            Position::StringCell { span, offset: next }
        };
        (Position::Encoded(offset), tail)
    }
}

fn sequence(
    input: IdentityInput<'_, '_, '_>,
    left: TermSpan,
    right: TermSpan,
    maps: bool,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    let count_a = if maps { left.datum() } else { left.keys };
    let count_b = if maps { right.datum() } else { right.keys };
    if count_a != count_b {
        return Ok(count_a.cmp(&count_b));
    }
    let mut a = if maps { left.keys } else { left.canonical };
    let mut b = if maps { right.keys } else { right.canonical };
    for _ in 0..count_a {
        let (key_a, key_b) = if maps {
            (input.2[a as usize], input.2[b as usize])
        } else {
            (a, b)
        };
        let order = compare(
            input,
            Position::Encoded(key_a),
            Position::Encoded(key_b),
            charge,
        )?;
        if order != Ordering::Equal {
            return Ok(order);
        }
        if maps {
            let order = compare(
                input,
                Position::Encoded(input.1[key_a as usize].next),
                Position::Encoded(input.1[key_b as usize].next),
                charge,
            )?;
            if order != Ordering::Equal {
                return Ok(order);
            }
            a += 1;
            b += 1;
        } else {
            a = input.1[a as usize].next;
            b = input.1[b as usize].next;
        }
    }
    Ok(Ordering::Equal)
}

fn fun_fields<'context>(
    input: IdentityInput<'_, '_, 'context>,
    span: TermSpan,
) -> (&'context str, i32, i32) {
    if span.datum() & 0x0080_0000 != 0 {
        let value = input.3.local_funs[(span.datum() & 0x007f_ffff) as usize];
        (value.module, value.index, value.old_uniq)
    } else {
        let value = input.4[span.datum() as usize];
        (
            input.3.atoms[value.module as usize],
            value.index,
            value.old_uniq,
        )
    }
}

fn local_fun_cmp(
    input: IdentityInput<'_, '_, '_>,
    left: TermSpan,
    right: TermSpan,
    charge: &mut PolicySchemaCharge,
) -> Result<Ordering, Error> {
    let order = fun_fields(input, left)
        .cmp(&fun_fields(input, right))
        .then(left.keys.cmp(&right.keys));
    if order != Ordering::Equal {
        return Ok(order);
    }
    let (mut a, mut b) = (left.canonical, right.canonical);
    for _ in 0..left.keys {
        let order = compare(input, Position::Encoded(a), Position::Encoded(b), charge)?;
        if order != Ordering::Equal {
            return Ok(order);
        }
        a = input.1[a as usize].next;
        b = input.1[b as usize].next;
    }
    Ok(Ordering::Equal)
}

fn runtime_cmp(
    input: IdentityInput<'_, '_, '_>,
    left: TermSpan,
    right: TermSpan,
    kind: u8,
) -> Ordering {
    if kind == 12 {
        return (
            input.3.atoms[left.datum() as usize],
            input.3.atoms[left.canonical as usize],
            left.keys,
        )
            .cmp(&(
                input.3.atoms[right.datum() as usize],
                input.3.atoms[right.canonical as usize],
                right.keys,
            ));
    }
    let node = |s: TermSpan| {
        input.3.atoms[(if kind == 10 {
            s.datum() & 0xfff
        } else {
            s.datum()
        }) as usize]
    };
    let order = node(left).cmp(node(right)).then(left.keys.cmp(&right.keys));
    if order != Ordering::Equal {
        return order;
    }
    if kind == 10 {
        let words_a = (left.datum() >> 12) & 15;
        let words_b = (right.datum() >> 12) & 15;
        let order = words_a.cmp(&words_b);
        if order != Ordering::Equal {
            return order;
        }
        for word in (0..words_a).rev() {
            let order = runtime::ref_word(input.0, left, word)
                .cmp(&runtime::ref_word(input.0, right, word));
            if order != Ordering::Equal {
                return order;
            }
        }
        Ordering::Equal
    } else if kind == 8 {
        input.0[left.canonical as usize..left.canonical as usize + 8]
            .cmp(&input.0[right.canonical as usize..right.canonical as usize + 8])
    } else {
        runtime::port_id(input.0, left).cmp(&runtime::port_id(input.0, right))
    }
}
