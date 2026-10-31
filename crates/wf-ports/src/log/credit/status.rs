//! Exact private business tags, excluding operational failures.

use crate::log::{CodecError as E, reader::Reader, writer::Writer};
use wf_domain::governance::credit::CreditRejection as C;
use wf_domain::governance::identified::{
    IdentifiedCreditDecision as D, IdentifiedCreditRejection as R,
};
use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;

pub(super) fn write(out: &mut Writer<'_>, decision: D) -> Result<(), E> {
    match decision {
        D::Granted { shard_id } | D::Spent { shard_id } | D::Renewed { shard_id } => {
            out.byte(match decision {
                D::Granted { .. } => 1,
                D::Spent { .. } => 2,
                _ => 3,
            })?;
            out.u64(shard_id)?;
        }
        D::Released => out.byte(4)?,
        D::Owner => out.byte(5)?,
        D::Cleaned { deleted, pending } => {
            if deleted > 256 {
                return Err(E::Malformed);
            }
            out.byte(6)?;
            out.u16(deleted)?;
            out.byte(u8::from(pending))?;
        }
        D::Rejected(reason) => {
            out.byte(255)?;
            rejection(out, reason)?;
        }
    }
    Ok(())
}

fn rejection(out: &mut Writer<'_>, reason: R) -> Result<(), E> {
    out.byte(match reason {
        R::Credit(_) => 0,
        R::NotFound => 1,
        R::ConfigurationRequired => 2,
        R::ConfigurationVersionRequired => 3,
        R::LeaseGenerationChanged => 4,
        R::ReservationHistoryFull => 5,
        R::CleanupBacklogFull => 6,
        R::CatalogBacklogFull => 7,
        R::CatalogSequenceExhausted => 8,
    })?;
    let R::Credit(reason) = reason else {
        return Ok(());
    };
    out.byte(match reason {
        C::LimitExceeded { .. } => 1,
        C::GenerationExhausted => 2,
        C::ConfigurationConflict => 3,
        C::ReservationIdsMismatch => 4,
        C::ReservationConflict => 5,
        C::BackwardTime => 6,
        C::LeaseExpired => 7,
    })?;
    if let C::LimitExceeded {
        requested,
        free,
        limit,
    } = reason
    {
        if requested == 0 || requested > MAX || free > MAX || limit > MAX {
            return Err(E::Malformed);
        }
        for value in [requested, free, limit] {
            out.u64(value)?;
        }
    }
    Ok(())
}

pub(super) fn read(reader: &mut Reader<'_>) -> Result<D, E> {
    Ok(match reader.byte()? {
        tag @ 1..=3 => {
            let shard_id = reader.u64()?;
            if shard_id > MAX {
                return Err(E::Malformed);
            }
            match tag {
                1 => D::Granted { shard_id },
                2 => D::Spent { shard_id },
                _ => D::Renewed { shard_id },
            }
        }
        4 => D::Released,
        5 => D::Owner,
        6 => {
            let deleted = reader.u16()?;
            if deleted > 256 {
                return Err(E::Malformed);
            }
            let pending = match reader.byte()? {
                0 => false,
                1 => true,
                _ => return Err(E::InvalidOptionality),
            };
            D::Cleaned { deleted, pending }
        }
        255 => D::Rejected(read_rejection(reader)?),
        _ => return Err(E::UnsupportedTag),
    })
}

fn read_rejection(reader: &mut Reader<'_>) -> Result<R, E> {
    Ok(match reader.byte()? {
        0 => R::Credit(match reader.byte()? {
            1 => {
                let (requested, free, limit) = (reader.u64()?, reader.u64()?, reader.u64()?);
                if requested == 0 || requested > MAX || free > MAX || limit > MAX {
                    return Err(E::Malformed);
                }
                C::LimitExceeded {
                    requested,
                    free,
                    limit,
                }
            }
            2 => C::GenerationExhausted,
            3 => C::ConfigurationConflict,
            4 => C::ReservationIdsMismatch,
            5 => C::ReservationConflict,
            6 => C::BackwardTime,
            7 => C::LeaseExpired,
            _ => return Err(E::UnsupportedTag),
        }),
        1 => R::NotFound,
        2 => R::ConfigurationRequired,
        3 => R::ConfigurationVersionRequired,
        4 => R::LeaseGenerationChanged,
        5 => R::ReservationHistoryFull,
        6 => R::CleanupBacklogFull,
        7 => R::CatalogBacklogFull,
        8 => R::CatalogSequenceExhausted,
        _ => return Err(E::UnsupportedTag),
    })
}
