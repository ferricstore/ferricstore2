//! Exact typed private publication records, independent of native ETF profiles.

use super::{StateValueError as E, finish, raw, read_u16, read_u64, take};
use wf_domain::governance::catalog::CreditCatalogMeta;

pub(crate) fn meta(bytes: &[u8]) -> Result<CreditCatalogMeta, E> {
    let mut at = header(bytes, b"FWCM")?;
    let head = read_u64(bytes, &mut at)?;
    let tail = read_u64(bytes, &mut at)?;
    finish(bytes, at)?;
    CreditCatalogMeta::restore(head, tail).map_err(|_| E::InvalidValue)
}

pub(crate) fn encode_meta(meta: CreditCatalogMeta) -> [u8; 22] {
    let mut out = [0; 22];
    out[..6].copy_from_slice(b"FWCM\0\x01");
    out[6..14].copy_from_slice(&meta.head().to_be_bytes());
    out[14..].copy_from_slice(&meta.tail().to_be_bytes());
    out
}

pub(crate) fn intent(bytes: &[u8]) -> Result<&[u8], E> {
    let mut at = header(bytes, b"FWCI")?;
    let scope = raw(bytes, &mut at)?;
    valid(scope)?;
    finish(bytes, at)?;
    Ok(scope)
}

pub(crate) fn encode_intent(scope: &[u8]) -> Result<Vec<u8>, E> {
    valid(scope)?;
    let mut out = Vec::new();
    out.try_reserve_exact(scope.len().checked_add(10).ok_or(E::LengthOverflow)?)
        .map_err(|_| E::Allocation)?;
    out.extend_from_slice(b"FWCI\0\x01");
    out.extend_from_slice(
        &u32::try_from(scope.len())
            .map_err(|_| E::LengthOverflow)?
            .to_be_bytes(),
    );
    out.extend_from_slice(scope);
    Ok(out)
}

fn valid(scope: &[u8]) -> Result<(), E> {
    if scope.is_empty() || scope.len() > 65_535 {
        Err(E::InvalidValue)
    } else {
        Ok(())
    }
}

fn header(bytes: &[u8], expected: &[u8]) -> Result<usize, E> {
    let mut at = 0;
    if take(bytes, &mut at, 4)? != expected {
        return Err(E::InvalidValue);
    }
    if read_u16(bytes, &mut at)? != 1 {
        return Err(E::UnsupportedVersion);
    }
    Ok(at)
}
