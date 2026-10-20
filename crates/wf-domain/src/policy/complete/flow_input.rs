//! Original incoming policy normalization into owned deterministic ETF snapshots.

use super::admission::charge_schema;
use super::{CircuitTrackingFailure, GovernanceField, GovernanceRuleKind, PolicySchemaLimits};
use super::{CompletePolicyEncodingError as Error, PolicySchemaCharge, PolicySchemaError};
use super::{
    PolicyIndexedInputRejection, PolicyRetentionInputRejection, PolicyRetryInputRejection,
};

mod core;
mod etf;
mod governance;
mod interop;
pub(in crate::policy::complete) use interop::{admit_raw, encode_raw, measure_raw, read_raw};
pub use interop::{
    encode_original_policy_from_complete, original_core_policy_patch_bytes,
    original_policy_from_complete_bytes, prepare_stored_core_policy_patch,
};
mod layout;
pub(in crate::policy::complete) mod order;
mod preflight;
pub(in crate::policy::complete) mod rejection;
mod states;
pub(in crate::policy::complete) mod value;

pub(in crate::policy::complete) type NormalizedFlowPolicyLayout<'input> =
    (layout::FlowPolicyLayout<'input>, usize);

/// Borrowed visible original input; this is no decoder or current-policy proof.
#[derive(Clone, Copy, Debug)]
pub enum RawFlowPolicyValue<'input> {
    /// The Elixir nil atom, distinct from the empty proper list.
    Nil,
    /// A mathematical integer represented directly.
    Integer(i128),
    /// Arbitrary integer magnitude; high zero padding and negative zero are legal.
    BigInteger {
        /// Mathematical sign; a zero magnitude is zero under either sign.
        negative: bool,
        /// Borrowed magnitude, with permitted high zero padding.
        little_endian: &'input [u8],
    },
    /// Borrowed atom spelling, without interning.
    Atom(&'input str),
    /// Original opaque binary bytes.
    Binary(&'input [u8]),
    /// Original map rows in captured source reduce order.
    Map(&'input [(Self, Self)]),
    /// Proper list; tuples represent keyword or binary state-policy pairs.
    List(&'input [Self]),
    /// Original tuple.
    Tuple(&'input [Self]),
    /// Nonempty prefix and actual tail; only list tails are mathematically proper.
    ImproperList {
        /// Actual nonempty prefix descriptors.
        prefix: &'input [Self],
        /// Actual tail; nil atom is not a proper-list terminator.
        tail: &'input Self,
    },
    /// Unsupported visible class, without hidden-payload correspondence proof.
    Other,
}

/// Reached caller diagnoses, without retained outcome or payment authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlowPolicyInputRejection {
    /// Type/options do not have the source root classes.
    RootShape,
    /// List options are not a proper keyword list.
    RootKeyword,
    /// State-only mode is present at type level.
    TopStateMode,
    /// Invalid customer version.
    Version,
    /// Invalid type-only active duration.
    MaxActive,
    /// Invalid states root class.
    StatesShape,
    /// Invalid states list or pair class.
    StatesList,
    /// Invalid nonempty binary state name.
    StateName,
    /// Reserved running state.
    RunningState,
    /// Invalid state policy container.
    StatePolicyShape,
    /// Type-only active duration appears in a state policy.
    StateMaxActive,
    /// Invalid present state mode.
    StateMode,
    /// Original retry diagnosis.
    Retry(PolicyRetryInputRejection),
    /// Original retention diagnosis with immutable captured limit.
    Retention(PolicyRetentionInputRejection),
    /// Original indexed-field diagnosis.
    Indexed(PolicyIndexedInputRejection),
    /// Original governance diagnosis.
    Governance(GovernancePolicyInputRejection),
    /// Complete normalized snapshot exceeds the source cap.
    SnapshotBytes,
}

/// Finite original governance diagnoses and their source field parameters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GovernancePolicyInputRejection {
    /// Invalid governance root.
    Shape,
    /// Invalid governance mode.
    Mode,
    /// Invalid effects container.
    EffectsShape,
    /// Invalid audit container.
    AuditShape,
    /// Invalid approvals container.
    ApprovalsShape,
    /// Invalid named rule-map container.
    RuleMapShape(GovernanceRuleKind),
    /// Invalid named rule container.
    RuleShape(GovernanceRuleKind),
    /// Invalid denial audit mode.
    Denials,
    /// Invalid enforcement mode.
    Enforcement,
    /// Invalid idempotency boolean.
    RequireIdempotencyBoolean,
    /// Invalid event boolean.
    EffectEventsBoolean,
    /// Invalid string-list class for a supported list field.
    StringList(GovernanceField),
    /// Too many raw list occurrences, including duplicates.
    ListCount,
    /// Invalid nonempty binary list leaf or improper tail.
    ListValue,
    /// One list leaf exceeds its field cap.
    ListBytes(GovernanceField),
    /// Too many surviving rules after Map.new.
    RuleCount(GovernanceRuleKind),
    /// Invalid rule name class/emptiness.
    RuleName(GovernanceRuleKind),
    /// Rule name exceeds the source cap.
    RuleNameBytes(GovernanceRuleKind),
    /// Invalid required nonnegative limit.
    Limit,
    /// Invalid positive integer for a supported positive field.
    Positive(GovernanceField),
    /// Invalid opaque budget unit.
    Unit,
    /// Invalid percentage.
    Percent,
    /// Ordered circuit tracking consistency failure.
    Tracking(CircuitTrackingFailure),
    /// Normalized governance exceeds its separate source cap.
    PolicyBytes,
}

/// Normalizes one original incoming policy in source order, then emits one Vec.
///
/// The visible bounded profile is distinct from raw decoding and from a future
/// caller/frame/Fresh/current/envelope parent. No accepted-data token escapes.
///
/// # Errors
/// Outer errors preserve operational profile/context/work/cap/allocation errors.
/// Inner errors are only the reached original caller diagnosis.
pub fn normalize_original_complete_policy_input(
    workflow_type: &RawFlowPolicyValue<'_>,
    attrs: &RawFlowPolicyValue<'_>,
    history_max_events_limit: u32,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<Vec<u8>, FlowPolicyInputRejection>, Error> {
    let (layout, length) = match prepare_original_flow_policy_layout(
        value::PolicyInputCursor::Flow(*workflow_type),
        value::PolicyInputCursor::Flow(*attrs),
        history_max_events_limit,
        limits,
        charge,
    )? {
        Ok(prepared) => prepared,
        Err(reason) => return Ok(Err(reason)),
    };
    let root = layout.root.ok_or(Error::Malformed)?;
    let bytes = etf::emit_snapshot(&layout, root, length, charge)?;
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    Ok(Ok(bytes))
}

pub(in crate::policy::complete) fn prepare_original_flow_policy_layout<'input>(
    workflow_type: value::PolicyInputCursor<'input>,
    attrs: value::PolicyInputCursor<'input>,
    history_max_events_limit: u32,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<NormalizedFlowPolicyLayout<'input>, FlowPolicyInputRejection>, Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    super::retention_input::valid_limit(history_max_events_limit)?;
    let (count, depth) = preflight::measure(workflow_type, attrs, charge)?;
    let layout = layout::FlowPolicyLayout::new(count, depth, charge)?;
    finish_layout(
        workflow_type,
        attrs,
        history_max_events_limit,
        layout,
        charge,
    )
}

pub(in crate::policy::complete) fn prepare_joined_flow_policy_layout<'input>(
    input: (
        value::PolicyInputCursor<'input>,
        value::PolicyInputCursor<'input>,
        u32,
    ),
    bounds: (usize, usize),
    live: usize,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<NormalizedFlowPolicyLayout<'input>, FlowPolicyInputRejection>, Error> {
    if charge.profile() != limits {
        return Err(Error::Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(charge, 128, 0).map_err(Error::Schema)?;
    super::retention_input::valid_limit(input.2)?;
    preflight::measure_joined(input.0, input.1, charge)?;
    let layout = layout::FlowPolicyLayout::new_with_base(bounds.0, bounds.1, live, charge)?;
    finish_layout(input.0, input.1, input.2, layout, charge)
}

fn finish_layout<'input>(
    workflow_type: value::PolicyInputCursor<'input>,
    attrs: value::PolicyInputCursor<'input>,
    history_max_events_limit: u32,
    mut layout: layout::FlowPolicyLayout<'input>,
    charge: &mut PolicySchemaCharge,
) -> Result<Result<NormalizedFlowPolicyLayout<'input>, FlowPolicyInputRejection>, Error> {
    let root = match self::core::normalize(
        workflow_type,
        attrs,
        history_max_events_limit,
        &mut layout,
        charge,
    )? {
        Ok(root) => root,
        Err(reason) => return Ok(Err(reason)),
    };
    layout.root = Some(root);
    let length = etf::measure_snapshot(&layout, root, charge)?;
    if length > 262_144 {
        return Ok(Err(FlowPolicyInputRejection::SnapshotBytes));
    }
    Ok(Ok((layout, length)))
}

pub(in crate::policy::complete) fn emit_original_flow_policy_envelope(
    normalized: &NormalizedFlowPolicyLayout<'_>,
    generation: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, Error> {
    let (layout, length) = normalized;
    let root = layout.root.ok_or(Error::Malformed)?;
    etf::emit_envelope(layout, root, *length, generation, charge)
}

/// Encodes one exact caller diagnosis into a freshly paid final owner.
///
/// # Errors
/// Preserves profile, invalid descriptor/context, work and allocation refusals.
pub fn encode_original_complete_policy_input_rejection_body(
    reason: FlowPolicyInputRejection,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Vec<u8>, Error> {
    rejection::encode(reason, limits, charge)
}
