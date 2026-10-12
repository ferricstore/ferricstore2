//! Version-one source token bytes from committed time and checked authority.

use wf_types::version::{Fence, TimestampMillis};

use crate::lease::LeaseToken;
use crate::workflow::batch::ApplyError;

use super::ClaimRejection;
use super::admission::ClaimPreflightError;

pub(super) fn source_token_length(
    worker: &[u8],
    now: TimestampMillis,
    fence: Fence,
) -> Result<usize, ClaimPreflightError> {
    let mut bytes = worker
        .len()
        .checked_add(2)
        .ok_or(ClaimPreflightError::Business(
            ClaimRejection::LengthOverflow,
        ))?;
    for value in [now.get(), fence.get()] {
        let digits = value.checked_ilog10().map_or(1, |log| log + 1);
        bytes = bytes
            .checked_add(
                usize::try_from(digits)
                    .map_err(|_| ClaimPreflightError::Business(ClaimRejection::LengthOverflow))?,
            )
            .ok_or(ClaimPreflightError::Business(
                ClaimRejection::LengthOverflow,
            ))?;
    }
    Ok(bytes)
}

pub(super) fn source_token(
    worker: &[u8],
    now: TimestampMillis,
    fence: Fence,
) -> Result<LeaseToken, ClaimPreflightError> {
    let length = source_token_length(worker, now, fence)?;
    let mut token = Vec::new();
    token
        .try_reserve_exact(length)
        .map_err(|_| ClaimPreflightError::Apply(ApplyError::AllocationFailed))?;
    token.extend_from_slice(worker);
    for mut value in [now.get(), fence.get()] {
        token.push(b':');
        let mut decimal = [0_u8; 20];
        let mut start = decimal.len();
        loop {
            start -= 1;
            decimal[start] = b'0'
                + u8::try_from(value % 10)
                    .map_err(|_| ClaimPreflightError::Apply(ApplyError::InvalidAuthority))?;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        token.extend_from_slice(&decimal[start..]);
    }
    LeaseToken::new(token).map_err(|_| ClaimPreflightError::Apply(ApplyError::InvalidAuthority))
}
