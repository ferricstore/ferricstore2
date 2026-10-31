//! Fixed-size canonical private ACK responses; no payload map or heap decoder.

use crate::log::{CodecError as E, CodecLimits, reader::Reader};
use wf_domain::governance::catalog::{
    CreditCatalogAckRejection as R, CreditCatalogAckReply as Reply, CreditCatalogMeta,
};

pub(super) fn bytes(reply: Reply) -> Result<[u8; 25], E> {
    let (tag, meta, deleted) = match reply {
        Reply::Accepted { meta, deleted } => {
            if meta.head() <= 1 || deleted > 256 || u64::from(deleted) >= meta.head() {
                return Err(E::Malformed);
            }
            (0, meta, deleted)
        }
        Reply::Rejected { meta, reason } => (
            match reason {
                R::StaleCursor => 1,
                R::InvalidRange => 2,
            },
            meta,
            0,
        ),
    };
    if meta.tail() + 1 - meta.head() > 65_536 {
        return Err(E::Malformed);
    }
    let mut out = [0; 25];
    out[..6].copy_from_slice(b"FWCA\0\x01");
    out[6] = tag;
    out[7..15].copy_from_slice(&meta.head().to_be_bytes());
    out[15..23].copy_from_slice(&meta.tail().to_be_bytes());
    out[23..].copy_from_slice(&deleted.to_be_bytes());
    Ok(out)
}

pub(super) fn encode(
    reply: Reply,
    maximum_wire: usize,
    maximum_owned: usize,
) -> Result<Vec<u8>, E> {
    if maximum_wire < 25 {
        return Err(E::ByteLimit);
    }
    if maximum_owned < 1024 {
        return Err(E::OwnedLimit);
    }
    let bytes = bytes(reply)?;
    let mut out = Vec::new();
    out.try_reserve_exact(25).map_err(|_| E::Allocation)?;
    out.extend_from_slice(&bytes);
    Ok(out)
}

pub(super) fn decode(
    encoded: &[u8],
    maximum_wire: usize,
    maximum_owned: usize,
) -> Result<Reply, E> {
    if encoded.len() > maximum_wire {
        return Err(E::ByteLimit);
    }
    if maximum_owned < 1024 {
        return Err(E::OwnedLimit);
    }
    let mut reader = Reader::new(encoded, CodecLimits::default());
    if reader.take(4)? != b"FWCA" {
        return Err(E::Malformed);
    }
    if reader.u16()? != 1 {
        return Err(E::UnsupportedVersion);
    }
    let tag = reader.byte()?;
    let meta =
        CreditCatalogMeta::restore(reader.u64()?, reader.u64()?).map_err(|_| E::Malformed)?;
    let deleted = reader.u16()?;
    reader.finish()?;
    let reply = match tag {
        0 => Reply::Accepted { meta, deleted },
        1 | 2 if deleted == 0 => Reply::Rejected {
            meta,
            reason: if tag == 1 {
                R::StaleCursor
            } else {
                R::InvalidRange
            },
        },
        _ => return Err(E::Malformed),
    };
    bytes(reply)?;
    Ok(reply)
}
