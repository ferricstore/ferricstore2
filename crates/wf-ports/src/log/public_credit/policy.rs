//! Source ETF metadata streams into SHA256 without an intermediate payload owner.
use super::{CreditPolicyInput, E, PreparedDigest};
use sha2::{Digest, Sha256};
use wf_domain::workflow::MAX_EXACT_MILLIS as MAX;
pub(super) fn fingerprint(value: CreditPolicyInput<'_>) -> Result<PreparedDigest, E> {
    let mut hash = Sha256::new();
    hash.update([131]);
    match value {
        CreditPolicyInput::Bytes(bytes) => {
            if bytes.is_empty() || bytes.len() > 262_144 {
                return Err(E::Malformed);
            }
            hash.update([109]);
            hash.update(
                u32::try_from(bytes.len())
                    .map_err(|_| E::LengthOverflow)?
                    .to_be_bytes(),
            );
            hash.update(bytes);
        }
        CreditPolicyInput::Number(n) if n <= 255 => {
            hash.update([97, u8::try_from(n).map_err(|_| E::Malformed)?]);
        }
        CreditPolicyInput::Number(n) if n <= 2_147_483_647 => {
            hash.update([98]);
            hash.update(u32::try_from(n).map_err(|_| E::Malformed)?.to_be_bytes());
        }
        CreditPolicyInput::Number(n) if n <= MAX => {
            let bytes = n.to_le_bytes();
            let end = bytes.iter().rposition(|b| *b != 0).ok_or(E::Malformed)? + 1;
            hash.update([110, u8::try_from(end).map_err(|_| E::LengthOverflow)?, 0]);
            hash.update(&bytes[..end]);
        }
        CreditPolicyInput::Number(_) => return Err(E::Malformed),
    }
    Ok(PreparedDigest::new(hash.finalize().into()))
}
