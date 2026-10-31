//! Bind positions derive from the sealed plan and actual immutable payment owner.
use super::{E, StagedAtomicClaim, codec};
use wf_domain::claim::apply::ClaimPlan;

pub(super) fn positions(
    staged: &StagedAtomicClaim<'_, '_, '_>,
    plan: &ClaimPlan<'_>,
    out: &mut [u16],
) -> Result<(usize, usize), E> {
    let count = plan.batch().map_or(0, |batch| batch.batch().items().len());
    let public = staged.prepared.intent().version() == 21;
    let source = plan.source_bindings();
    if public != source.is_some() {
        return Err(E::InvalidChanges);
    }
    let attempts = source.map_or(count, |(attempts, _)| attempts);
    if let Some((_, positions)) = source
        && (positions.len() != count
            || positions.windows(2).any(|p| p[0] >= p[1])
            || positions
                .last()
                .is_some_and(|position| *position >= attempts))
    {
        return Err(E::InvalidChanges);
    }
    let Some(paid) = staged.reply.payment().map_err(codec)? else {
        return Ok((staged.reservation_cursor, 0));
    };
    let end = staged
        .reservation_cursor
        .checked_add(attempts)
        .ok_or(E::Limit)?;
    if end > paid.count() {
        return Err(E::InvalidChanges);
    }
    for (index, target) in out.get_mut(..count).ok_or(E::Limit)?.iter_mut().enumerate() {
        let offset = source.map_or(index, |(_, positions)| positions[index]);
        *target = u16::try_from(
            staged
                .reservation_cursor
                .checked_add(offset)
                .ok_or(E::Limit)?,
        )
        .map_err(|_| E::Limit)?;
    }
    Ok((end, count))
}
