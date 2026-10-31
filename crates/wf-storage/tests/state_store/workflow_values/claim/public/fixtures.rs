use super::super::{atomic::fixtures as a, fixtures as leaf, *};
use wf_domain::{
    claim::caller::{
        ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, ClaimStatesInput,
        normalize_claim_due,
    },
    governance::credit::CreditLimits,
    policy::complete::RawFlowPolicyValue as Raw,
};
use wf_ports::log::{CodecLimits, PublicClaimPreparation, encode_public_claim_request};
use wf_storage::transaction::PublicClaimTransactionLimits;
use wf_types::identity::RequestId;

pub(super) const BATCH: [u8; 16] = [0x7a; 16];
pub(super) fn limits() -> TestResult<PublicClaimTransactionLimits> {
    let mut base = a::limits();
    base.credit = CreditLimits::new(
        base.credit.max_leases(),
        base.credit.max_reservations(),
        8 * 1_048_576,
        base.credit.max_work(),
    )?;
    Ok(PublicClaimTransactionLimits {
        base,
        maximum_credit_work: 128 * 1_048_576,
        maximum_credit_temporary_bytes: 32 * 1_048_576,
    })
}
pub(super) fn filled(path: &std::path::Path, governed: bool) -> TestResult<RedbStateStore> {
    let mut store = a::filled(path)?;
    if governed {
        let rule = [
            (Raw::Atom("limit"), Raw::Integer(8)),
            (Raw::Atom("lease_size"), Raw::Integer(6)),
        ];
        let running = [(Raw::Binary(b"running"), Raw::Map(&rule))];
        let governance = [(Raw::Atom("limits"), Raw::Map(&running))];
        let attrs = [
            (Raw::Atom("version"), Raw::Integer(17)),
            (Raw::Atom("governance"), Raw::Map(&governance)),
        ];
        leaf::policy(&mut store, 5, 2500, Raw::Map(&attrs))?;
    }
    Ok(store)
}
pub(super) fn frame(reclaim: bool, batch: [u8; 16]) -> TestResult<PreparedBytes> {
    let keys: [&[u8]; 1] = [b"p"];
    let mut input = ClaimDueInput::new(b"r", b"new");
    input.states = ClaimStatesInput::One(b"q");
    input.partitions = ClaimPartitionsInput::Many(&keys);
    input.limit = Some(4);
    input.lease_ms = Some(100);
    input.reclaim_expired = Some(reclaim);
    encode(input, batch, 100)
}
pub(super) fn encode(
    input: ClaimDueInput<'_>,
    batch: [u8; 16],
    scans: usize,
) -> TestResult<PreparedBytes> {
    let caller = normalize_claim_due(input, ClaimCallerLimits::default())?;
    Ok(encode_public_claim_request(
        &f::scope()?,
        &RequestId::new(b"public-parent".to_vec())?,
        f::clock(3000)?,
        &caller,
        PublicClaimPreparation {
            token_version: 1,
            first_ordinal: 0,
            scan_budget: scans,
            head_budget: 100,
            batch,
        },
        CodecLimits::default(),
        1_048_576,
    )?)
}
