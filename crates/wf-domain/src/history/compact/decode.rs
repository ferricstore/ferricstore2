//! Complete borrowed validation precedes all rendered field owners.

use super::{
    DecodedOriginalCompactHistory as D, OriginalCompactHistoryContext as C,
    OriginalCompactHistoryError as E, OriginalCompactHistoryLimits as L,
    budget::{Budget, add},
    primitives::{DecodeError, Reader},
    project,
};

pub(super) struct Parsed<'a> {
    pub(super) event: Option<&'a [u8]>,
    pub(super) version: Option<u128>,
    pub(super) at: Option<u128>,
    pub(super) state: Option<&'a [u8]>,
    pub(super) numbers: [Option<u128>; 7],
    pub(super) references: [Option<&'a [u8]>; 5],
    pub(super) metadata_start: usize,
    pub(super) metadata_count: usize,
    pub(super) metadata_valid: usize,
}

pub(super) fn run<'a>(bytes: &'a [u8], context: C<'a>, limits: L) -> Result<D<'a>, E> {
    let mut budget = Budget::new(limits)?;
    budget.encoded(bytes.len())?;
    budget.input(bytes.len())?;
    budget.context(context)?;
    let parsed = match parse(&mut Reader::new(bytes), &mut budget) {
        Ok(parsed) => parsed,
        Err(DecodeError::Malformed) => {
            return Ok(D {
                fields: Vec::new(),
                work: budget.work,
                temporary: budget.peak,
            });
        }
        Err(DecodeError::Operational(error)) => return Err(error),
    };
    let fields = project::run(bytes, &parsed, context, &mut budget)?;
    Ok(D {
        fields,
        work: budget.work,
        temporary: budget.peak,
    })
}

fn parse<'a>(reader: &mut Reader<'a>, budget: &mut Budget) -> Result<Parsed<'a>, DecodeError> {
    if reader.take(4, budget)? != b"FSH2" {
        return Err(DecodeError::Malformed);
    }
    let flags = reader.integer(budget)?.ok_or(DecodeError::Malformed)?;
    let event = reader.binary(budget)?;
    let version = reader.integer(budget)?;
    let at = reader.integer(budget)?;
    let state = reader.binary(budget)?;
    let mut numbers = [Some(0), Some(0), Some(0), at, at, None, None];
    for (bit, number) in numbers.iter_mut().enumerate() {
        budget.spend(1)?;
        if flags & (1 << bit) != 0 {
            *number = reader.integer(budget)?;
        }
    }
    let mut references = [None; 5];
    for (bit, value) in references.iter_mut().enumerate() {
        budget.spend(1)?;
        if flags & (1 << (bit + 7)) != 0 {
            *value = reader.binary(budget)?;
        }
    }
    let metadata_count = if flags & (1 << 12) == 0 {
        0
    } else {
        let count = reader.integer(budget)?.ok_or(DecodeError::Malformed)?;
        let count = usize::try_from(count).map_err(|_| E::Limit)?;
        budget.pairs(count)?;
        if count > reader.remaining() / 2 {
            return Err(DecodeError::Malformed);
        }
        count
    };
    let metadata_start = reader.position();
    let mut metadata_valid = 0;
    for _ in 0..metadata_count {
        budget.spend(1)?;
        let key = reader.binary(budget)?;
        let value = reader.binary(budget)?;
        if key.is_some() && value.is_some() {
            metadata_valid = add(metadata_valid, 1)?;
        }
    }
    if reader.remaining() != 0 {
        return Err(DecodeError::Malformed);
    }
    Ok(Parsed {
        event,
        version,
        at,
        state,
        numbers,
        references,
        metadata_start,
        metadata_count,
        metadata_valid,
    })
}
