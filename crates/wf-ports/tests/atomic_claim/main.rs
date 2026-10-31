//! Full original caller identity and exact atomic-parent committed framing.
#![forbid(unsafe_code)]

use wf_domain::{
    claim::caller::{ClaimCallerLimits, ClaimDueInput, normalize_claim_due},
    workflow::{admission::AdmissionLimits, request::RequestScope},
};
use wf_ports::log::{AtomicClaimOptions, CodecLimits, PreparedBytes, encode_atomic_claim_request};
use wf_types::{
    identity::{GroupId, RecoveryId, RequestId, TenantId},
    version::{CommittedTime, HlcTimestamp},
};

mod caller;
mod framing;
mod policy;
mod response;
type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn limits() -> CodecLimits {
    CodecLimits::new(AdmissionLimits::default())
}
fn options() -> AtomicClaimOptions<'static> {
    AtomicClaimOptions {
        token_version: 1,
        first_ordinal: 0,
        scan_budget: 20,
        head_budget: 10,
        payment: None,
    }
}
fn encode(
    input: ClaimDueInput<'_>,
    options: AtomicClaimOptions<'_>,
    now: u64,
) -> TestResult<PreparedBytes> {
    let scope = RequestScope::new(
        TenantId::new(b"t".to_vec())?,
        GroupId::new(b"g".to_vec())?,
        RecoveryId::new(b"r".to_vec())?,
    );
    let call = normalize_claim_due(input, ClaimCallerLimits::default())?;
    Ok(encode_atomic_claim_request(
        &scope,
        &RequestId::new(b"claim".to_vec())?,
        CommittedTime::new(HlcTimestamp::new(now, 0)?, now)?,
        &call,
        options,
        limits(),
        1024 * 1024,
    )?)
}
fn intent(frame: &[u8]) -> TestResult<&[u8]> {
    assert_eq!(frame.get(..7), Some(b"FWCP\0\x01\x12".as_slice()));
    let mut at = 7;
    for _ in 0..4 {
        let n = usize::try_from(u32::from_be_bytes(
            frame.get(at..at + 4).ok_or("length")?.try_into()?,
        ))?;
        at += 4 + n;
    }
    let result = frame.get(at..at + 34).ok_or("intent")?;
    assert_eq!(&result[..2], &[0, 20]);
    Ok(result)
}
