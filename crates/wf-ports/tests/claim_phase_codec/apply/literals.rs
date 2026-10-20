//! Independent complete wire literals; no product response encoder.

use wf_domain::claim::apply::ClaimRejection;

use super::super::TestResult;

fn put_raw(out: &mut Vec<u8>, value: &[u8]) -> TestResult {
    out.extend_from_slice(&u32::try_from(value.len())?.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

// Manually packed FWCR1, independent of product response/reducer encoders.
pub(super) fn receipt(worker: &[u8], token: &[u8], attempts: u64) -> TestResult<Vec<u8>> {
    let mut out = b"FWCR\0\x01\0\0\0".to_vec();
    for number in [1_u32, 0, 1] {
        out.extend_from_slice(&number.to_be_bytes());
    }
    for field in [
        b"p".as_slice(),
        &[0, 255],
        &[b't', 255],
        b"waiting",
        worker,
        token,
    ] {
        put_raw(&mut out, field)?;
    }
    for number in [2_u64, 1, 61, 5, 11, attempts] {
        out.extend_from_slice(&number.to_be_bytes());
    }
    out.push(2);
    out.extend_from_slice(&11_u128.to_be_bytes());
    out.push(1);
    put_raw(&mut out, &[0, 1, 255])?;
    out.extend_from_slice(&[0, 0]);
    assert_eq!(out.len(), 132 + worker.len() + token.len());
    Ok(out)
}

pub(super) fn original_receipt(attempts: u64) -> TestResult<Vec<u8>> {
    receipt(&[b'w', 255], b"w\xFF:11:1", attempts)
}

pub(super) fn all_rejections() -> [ClaimRejection; 16] {
    [
        ClaimRejection::InvalidScope,
        ClaimRejection::InvalidPriority,
        ClaimRejection::InvalidBudget,
        ClaimRejection::HeadBudget,
        ClaimRejection::StaleContinuation,
        ClaimRejection::EmptyType,
        ClaimRejection::EmptyWorker,
        ClaimRejection::ZeroLease,
        ClaimRejection::LeaseTooLarge,
        ClaimRejection::LeaseDeadlineTooLarge,
        ClaimRejection::VersionOverflow,
        ClaimRejection::FenceOverflow,
        ClaimRejection::ValueTooLarge {
            bytes: 513,
            maximum: 512,
        },
        ClaimRejection::DecisionBytes,
        ClaimRejection::LengthOverflow,
        ClaimRejection::WorkBytes {
            bytes: 1_025,
            maximum: 1_024,
        },
    ]
}
