//! Fresh bounded restoration followed by a borrowed public reply visit.

use crate::log::CodecError;
use wf_domain::governance::credit::CreditLimits;
use wf_domain::governance::view::CreditReplyView;

pub(super) fn visit<R>(
    _encoded: &[u8],
    _credit: CreditLimits,
    _maximum_wire_bytes: usize,
    _maximum_owned_bytes: usize,
    _visit: impl for<'reply> FnOnce(CreditReplyView<'reply>) -> R,
) -> Result<R, CodecError> {
    Err(CodecError::UnsupportedVersion)
}
