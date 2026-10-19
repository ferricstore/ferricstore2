//! Every underlying I/O attempt is charged before the caller is invoked.

use super::super::store::RedbStateTransaction;
use super::{SnapshotError, SnapshotLimits, admission};

fn attempt(tx: &RedbStateTransaction<'_>, limits: SnapshotLimits) -> Result<(), SnapshotError> {
    admission::debit(&tx.io_calls, 1, limits.maximum_io_calls()).map_err(|_| SnapshotError::Limit)
}

pub(super) fn read(
    tx: &RedbStateTransaction<'_>,
    input: &mut impl std::io::Read,
    mut bytes: &mut [u8],
    limits: SnapshotLimits,
) -> Result<(), SnapshotError> {
    while !bytes.is_empty() {
        attempt(tx, limits)?;
        let n = bytes.len().min(limits.maximum_io_bytes());
        match input.read(&mut bytes[..n]) {
            Ok(0) => return Err(SnapshotError::Malformed),
            Ok(count) if count <= n => {
                bytes = &mut bytes[count..];
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => (),
            Ok(_) | Err(_) => return Err(SnapshotError::InputUnavailable),
        }
    }
    Ok(())
}

pub(super) fn eof(
    tx: &RedbStateTransaction<'_>,
    input: &mut impl std::io::Read,
    limits: SnapshotLimits,
) -> Result<(), SnapshotError> {
    loop {
        attempt(tx, limits)?;
        match input.read(&mut [0_u8; 1]) {
            Ok(0) => return Ok(()),
            Ok(_) => return Err(SnapshotError::Malformed),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => (),
            Err(_) => return Err(SnapshotError::InputUnavailable),
        }
    }
}

pub(super) fn write(
    tx: &RedbStateTransaction<'_>,
    output: &mut impl std::io::Write,
    mut bytes: &[u8],
    limits: SnapshotLimits,
) -> Result<(), SnapshotError> {
    while !bytes.is_empty() {
        attempt(tx, limits)?;
        let n = bytes.len().min(limits.maximum_io_bytes());
        match output.write(&bytes[..n]) {
            Ok(0) => return Err(SnapshotError::OutputUnavailable),
            Ok(count) if count <= n => {
                bytes = &bytes[count..];
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => (),
            Ok(_) | Err(_) => return Err(SnapshotError::OutputUnavailable),
        }
    }
    Ok(())
}
