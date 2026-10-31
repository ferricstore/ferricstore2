//! Fixed-size original maintenance response; no mutable or retained owner clones.
use super::{E, R};
use wf_domain::governance::release::{
    ReleaseMaintenanceRejection as J, ReleaseOutboxState, validate_release_state,
};

pub(super) fn bytes(reply: R) -> Result<[u8; 25], E> {
    let (tag, meta, affected) = match reply {
        R::Accepted { meta, affected } if affected <= 256 => (0, meta, affected),
        R::Accepted { .. } => return Err(E::Malformed),
        R::Rejected { meta, reason } => (
            match reason {
                J::StaleCursor => 1,
                J::InvalidAcknowledgment => 2,
                J::InvalidCompletion => 3,
            },
            meta,
            0,
        ),
    };
    validate_release_state(meta).map_err(|_| E::Malformed)?;
    let mut bytes = [0_u8; 25];
    bytes[..6].copy_from_slice(b"FWRM\0\x01");
    bytes[6] = tag;
    bytes[7..15].copy_from_slice(&meta.head.to_be_bytes());
    bytes[15..23].copy_from_slice(&meta.tail.to_be_bytes());
    bytes[23..25].copy_from_slice(&affected.to_be_bytes());
    Ok(bytes)
}
pub(super) fn encode(reply: R, maximum: usize, maximum_owned: usize) -> Result<Vec<u8>, E> {
    let bytes = bytes(reply)?;
    limits(bytes.len(), maximum, maximum_owned)?;
    crate::log::reader::copy(&bytes)
}
pub(super) fn decode(encoded: &[u8], maximum: usize, maximum_owned: usize) -> Result<R, E> {
    limits(encoded.len(), maximum, maximum_owned)?;
    if encoded.len() != 25 || encoded.get(..6) != Some(b"FWRM\0\x01") {
        return Err(E::Malformed);
    }
    let meta = ReleaseOutboxState {
        head: u64::from_be_bytes(encoded[7..15].try_into().map_err(|_| E::Malformed)?),
        tail: u64::from_be_bytes(encoded[15..23].try_into().map_err(|_| E::Malformed)?),
    };
    validate_release_state(meta).map_err(|_| E::Malformed)?;
    let affected = u16::from_be_bytes(encoded[23..25].try_into().map_err(|_| E::Malformed)?);
    if affected > 256 {
        return Err(E::Malformed);
    }
    let reply = match encoded[6] {
        0 => R::Accepted { meta, affected },
        tag if affected == 0 => R::Rejected {
            meta,
            reason: match tag {
                1 => J::StaleCursor,
                2 => J::InvalidAcknowledgment,
                3 => J::InvalidCompletion,
                _ => return Err(E::UnsupportedTag),
            },
        },
        _ => return Err(E::Malformed),
    };
    Ok(reply)
}
fn limits(length: usize, maximum: usize, maximum_owned: usize) -> Result<(), E> {
    if length > maximum {
        return Err(E::ByteLimit);
    }
    if maximum_owned < 1024 {
        return Err(E::OwnedLimit);
    }
    Ok(())
}
