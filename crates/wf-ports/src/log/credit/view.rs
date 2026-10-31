//! Fresh bounded restoration followed by a borrowed public reply visit.

use crate::log::{CodecError as E, decode_credit_reply};
use wf_domain::governance::credit::CreditLimits;
use wf_domain::governance::view::{CreditReplyView, project_credit_reply};

const VIEW_BYTES: usize = 1024;
const VIEW_WORK: u64 = 1024;

pub(super) fn visit<R>(
    encoded: &[u8],
    credit: CreditLimits,
    maximum_wire_bytes: usize,
    maximum_owned_bytes: usize,
    visit: impl for<'reply> FnOnce(CreditReplyView<'reply>) -> R,
) -> Result<R, E> {
    // Reserve the inline projection/hash cohort before any decoder allocation.
    // These are fresh invocation charges, never a cached admission certificate.
    let owned = maximum_owned_bytes
        .min(credit.max_owned_bytes())
        .checked_sub(VIEW_BYTES)
        .filter(|n| *n > 0)
        .ok_or(E::OwnedLimit)?;
    let work = credit
        .max_work()
        .checked_sub(VIEW_WORK)
        .filter(|n| *n > 0)
        .ok_or(E::WorkLimit)?;
    let remaining = CreditLimits::new(credit.max_leases(), credit.max_reservations(), owned, work)
        .map_err(|_| E::Malformed)?;
    let reply = decode_credit_reply(encoded, remaining, maximum_wire_bytes, owned)?;
    let public = project_credit_reply(&reply).map_err(|_| E::Malformed)?;
    Ok(visit(public))
}
