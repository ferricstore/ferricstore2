//! Pure preparation values; shared service authorization/submission is pending.

use wf_domain::claim::caller::NormalizedClaimDue;
use wf_domain::policy::installation::InstallCommand;
use wf_domain::workflow::request::ClientIntent;
use wf_types::identity::TenantId;

use crate::error::PreparationError;

/// Explicit typed comparison value chosen by a versioned operation schema.
///
/// This is neither a network DTO nor a generic domain/persistence serializer.
/// Opaque payloads are borrowed; avoid logging the tree's secret values.
#[derive(Clone, Copy)]
pub enum IntentValue<'a> {
    /// An explicitly present null, distinct from an omitted map entry.
    Null,
    /// Boolean false.
    False,
    /// Boolean true.
    True,
    /// Full unsigned integer, independent of JSON spelling.
    Unsigned(u64),
    /// Signed integer, retaining its category even when nonnegative.
    Signed(i64),
    /// Valid UTF-8 text without Unicode normalization.
    String(&'a str),
    /// Unchanged opaque bytes, distinct from text and absence.
    Bytes(&'a [u8]),
    /// Items in meaningful caller order.
    List(&'a [Self]),
    /// Unique string keys; preparation sorts them by UTF-8 bytes.
    Map(&'a [(&'a str, Self)]),
    /// Finite IEEE-754 binary64; preparation preserves negative zero.
    Float(f64),
}

/// Immutable preparation budget; all fixed root metadata counts against it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntentLimits {
    pub(crate) bytes: usize,
    pub(crate) values: usize,
    pub(crate) depth: usize,
}

impl IntentLimits {
    /// Builds positive limits, bounding recursive depth to 128.
    ///
    /// # Errors
    /// Returns [`PreparationError::InvalidLimits`] for zero or excessive depth.
    pub const fn new(bytes: usize, values: usize, depth: usize) -> Result<Self, PreparationError> {
        if bytes == 0 || values == 0 || depth == 0 || depth > 128 {
            Err(PreparationError::InvalidLimits)
        } else {
            Ok(Self {
                bytes,
                values,
                depth,
            })
        }
    }
}

impl Default for IntentLimits {
    fn default() -> Self {
        Self {
            bytes: 1_048_576,
            values: 16_384,
            depth: 32,
        }
    }
}

/// Explicit inputs after schema selection; this value confers no authority.
#[derive(Clone, Copy)]
pub struct IntentInput<'a> {
    /// Exact tenant selected by the authorizing application service.
    pub tenant: &'a TenantId,
    /// Nonempty operation name from the versioned operation registry.
    pub operation: &'a str,
    /// Complete typed arguments, including semantic guards and optionality.
    pub arguments: &'a [(&'a str, IntentValue<'a>)],
}

/// Computes version-1 comparison identity from explicit typed values.
///
/// The caller must validate its complete operation schema and current tenant
/// authorization. This pure helper grants neither of those capabilities and
/// never establishes durable submission or apply authority. Server-generated
/// preparation data and request/call IDs do not belong in `arguments`.
///
/// # Errors
/// Rejects nonfinite numbers, duplicate keys, invalid operation and exceeded
/// bounds. Allocation/overflow errors are operational preparation failures.
pub fn prepare_intent(
    input: IntentInput<'_>,
    limits: IntentLimits,
) -> Result<ClientIntent, PreparationError> {
    crate::intent::prepare(input, limits)
}

/// Compares supported policy-set caller fields under normalization version 1.
///
/// Retains patch presence, binary names and generation guards without reading
/// installed policy or resolving defaults. This mathematical helper also accepts
/// business-invalid typed input; the outer schema, authorization and source
/// policy admission must run separately before authoritative submission.
///
/// # Errors
/// Returns bounded byte/value/depth, checked length or allocation failures.
pub fn prepare_policy_set_intent(
    tenant: &TenantId,
    command: &InstallCommand,
    limits: IntentLimits,
) -> Result<ClientIntent, PreparationError> {
    crate::intent::prepare_policy_set(tenant, command, limits)
}

/// Compares normalized public claim callers without clock or scope authority.
///
/// Preserves meaningful caller options and source order, including mathematically
/// representable business-invalid values. Source/schema/scope authorization and
/// public parent admission must run separately before authoritative submission.
///
/// # Errors
/// Returns canonical byte/value/depth, checked length or allocation failures.
pub fn prepare_claim_due_intent(
    tenant: &TenantId,
    request: &NormalizedClaimDue<'_>,
    limits: IntentLimits,
) -> Result<ClientIntent, PreparationError> {
    crate::intent::prepare_claim_due(tenant, request, limits)
}
