//! Neutral complete member identities; encoded bytes never grant routing authority.

use crate::log::CodecError;
use wf_domain::workflow::request::RequestScope;

/// Borrowed original identity; tenant is supplied by the verified catalog scope.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CreditCatalogMemberRef<'a> {
    /// Original group bytes, requiring authoritative routing verification.
    pub group: &'a [u8],
    /// Original recovery bytes, not a current-epoch certificate.
    pub recovery: &'a [u8],
    /// Exact original credit scope.
    pub credit_scope: &'a [u8],
}

/// One bounded flat byte owner with ranges; no per-member payload allocation.
pub struct EncodedCreditCatalogMembers {
    bytes: Vec<u8>,
    ranges: Vec<(usize, usize)>,
    owned_bytes: usize,
}
impl EncodedCreditCatalogMembers {
    /// Visits complete encoded members, borrowing their one original byte owner.
    pub fn members(&self) -> impl ExactSizeIterator<Item = &[u8]> {
        self.ranges.iter().map(|(a, b)| &self.bytes[*a..*b])
    }
    /// Logical prospective flat/range/reference cohort size, without payment proof.
    #[must_use]
    pub const fn owned_bytes(&self) -> usize {
        self.owned_bytes
    }
}

/// Encodes at most256 original identities in one prospectively bounded cohort.
/// # Errors
/// Refuses invalid fields, wire/owner ceilings or allocation failure.
pub fn encode_credit_catalog_members(
    source: &RequestScope,
    scopes: &[&[u8]],
    maximum_wire_bytes: usize,
    maximum_owned_bytes: usize,
) -> Result<EncodedCreditCatalogMembers, CodecError> {
    if maximum_wire_bytes == 0 {
        return Err(CodecError::ByteLimit);
    }
    if scopes.len() > 256 {
        return Err(CodecError::ItemLimit);
    }
    let group = source.group().as_bytes();
    let recovery = source.recovery().as_bytes();
    let mut total = 0_usize;
    for scope in scopes {
        if scope.is_empty() {
            return Err(CodecError::Malformed);
        }
        let size = 14_usize
            .checked_add(scope.len())
            .and_then(|n| n.checked_add(group.len()))
            .and_then(|n| n.checked_add(recovery.len()))
            .ok_or(CodecError::LengthOverflow)?;
        if size > 65535 {
            return Err(CodecError::KeyLimit);
        }
        total = total.checked_add(size).ok_or(CodecError::LengthOverflow)?;
    }
    if total > maximum_wire_bytes {
        return Err(CodecError::ByteLimit);
    }
    let slots = scopes
        .len()
        .checked_mul(std::mem::size_of::<(usize, usize)>() + std::mem::size_of::<&[u8]>())
        .ok_or(CodecError::LengthOverflow)?;
    let owned_bytes = total
        .checked_add(slots)
        .and_then(|n| n.checked_add(128))
        .ok_or(CodecError::LengthOverflow)?;
    if owned_bytes > maximum_owned_bytes {
        return Err(CodecError::OwnedLimit);
    }
    let mut bytes = Vec::new();
    let mut ranges = Vec::new();
    bytes
        .try_reserve_exact(total)
        .map_err(|_| CodecError::Allocation)?;
    ranges
        .try_reserve_exact(scopes.len())
        .map_err(|_| CodecError::Allocation)?;
    for scope in scopes {
        let start = bytes.len();
        bytes.extend_from_slice(b"FWGL\0\x01");
        bytes.extend_from_slice(
            &u32::try_from(scope.len())
                .map_err(|_| CodecError::LengthOverflow)?
                .to_be_bytes(),
        );
        bytes.extend_from_slice(scope);
        for field in [group, recovery] {
            bytes.extend_from_slice(
                &u16::try_from(field.len())
                    .map_err(|_| CodecError::LengthOverflow)?
                    .to_be_bytes(),
            );
            bytes.extend_from_slice(field);
        }
        ranges.push((start, bytes.len()));
    }
    Ok(EncodedCreditCatalogMembers {
        bytes,
        ranges,
        owned_bytes,
    })
}

/// Fully inspects one member and borrows its original fields without allocation.
/// # Errors
/// Refuses unsupported/noncanonical/truncated/trailing or oversized bytes.
pub fn decode_credit_catalog_member(
    encoded: &[u8],
    maximum_wire_bytes: usize,
) -> Result<CreditCatalogMemberRef<'_>, CodecError> {
    if maximum_wire_bytes == 0 || encoded.len() > maximum_wire_bytes || encoded.len() > 65535 {
        return Err(CodecError::ByteLimit);
    }
    let mut at = 0;
    if take(encoded, &mut at, 4)? != b"FWGL" {
        return Err(CodecError::Malformed);
    }
    if take(encoded, &mut at, 2)? != [0, 1] {
        return Err(CodecError::UnsupportedVersion);
    }
    let n = u32::from_be_bytes(
        take(encoded, &mut at, 4)?
            .try_into()
            .map_err(|_| CodecError::Malformed)?,
    );
    let credit_scope = field(
        encoded,
        &mut at,
        usize::try_from(n).map_err(|_| CodecError::LengthOverflow)?,
        65535,
    )?;
    let n = u16::from_be_bytes(
        take(encoded, &mut at, 2)?
            .try_into()
            .map_err(|_| CodecError::Malformed)?,
    );
    let group = field(encoded, &mut at, usize::from(n), 256)?;
    let n = u16::from_be_bytes(
        take(encoded, &mut at, 2)?
            .try_into()
            .map_err(|_| CodecError::Malformed)?,
    );
    let recovery = field(encoded, &mut at, usize::from(n), 256)?;
    if at != encoded.len() {
        return Err(CodecError::TrailingBytes);
    }
    Ok(CreditCatalogMemberRef {
        group,
        recovery,
        credit_scope,
    })
}

fn take<'a>(encoded: &'a [u8], at: &mut usize, len: usize) -> Result<&'a [u8], CodecError> {
    let end = at.checked_add(len).ok_or(CodecError::LengthOverflow)?;
    let value = encoded.get(*at..end).ok_or(CodecError::Malformed)?;
    *at = end;
    Ok(value)
}
fn field<'a>(
    encoded: &'a [u8],
    at: &mut usize,
    len: usize,
    maximum: usize,
) -> Result<&'a [u8], CodecError> {
    if len == 0 {
        return Err(CodecError::Malformed);
    }
    if len > maximum {
        return Err(CodecError::ValueLimit);
    }
    take(encoded, at, len)
}
