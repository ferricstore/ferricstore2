//! Exact private owner point; key construction grants no state authority.

use super::{StateValueError, encode_components};
use wf_domain::workflow::request::RequestScope;

pub(crate) fn encode_credit_owner_key(
    scope: &RequestScope,
    credit_scope: &[u8],
    maximum: usize,
) -> Result<Vec<u8>, StateValueError> {
    if credit_scope.is_empty() || credit_scope.len() > 65_535 {
        return Err(StateValueError::InvalidValue);
    }
    encode_components(
        12,
        &[
            scope.group().as_bytes(),
            scope.recovery().as_bytes(),
            scope.tenant().as_bytes(),
            credit_scope,
        ],
        maximum,
    )
}
