//! Generic original caller replies, separate from retained-envelope authority.

use super::admission::charge_schema;
use super::flow_input::value::PolicyInputKind as Kind;
use super::original_entry::TermPosition;
use super::original_entry::indexed::OriginalPolicyCurrentIndex;
use super::{
    CompletePolicyEncodingError as EncodingError, OriginalPolicyReadValue, PolicySchemaCharge,
    PolicySchemaError, PolicySchemaLimits,
};

mod maps;
mod model;
mod resolve;
mod source;
mod states;
mod value;

use model::PolicyReplyRef as Ref;
pub use model::{
    OriginalPolicyReply, OriginalPolicyReplyContext, OriginalPolicyReplyError,
    OriginalPolicyReplyMapEntry, OriginalPolicyReplyMapIter, OriginalPolicyReplyRejection,
    OriginalPolicyReplyValue,
};
pub use source::{
    original_policy_get_reply_projection_bytes, original_policy_projection_reply_bytes,
};

/// Projects a successful original setter envelope without rebinding stored type.
///
/// This post-front-admission component borrows bytes, request and captured safety.
/// It performs one fresh structural decode and no policy business normalization.
/// Native/custom Enumerable capture and eventual wire emission remain separate.
///
/// # Errors
/// Preserves operational refusals and reached ordinary source exceptions. An
/// undecodable source envelope returns the inner allocation-failed diagnosis.
pub fn present_original_policy_set_reply<'input>(
    workflow_type: &'input [u8],
    encoded: &'input [u8],
    state: Option<&'input [u8]>,
    context: OriginalPolicyReplyContext<'input>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<
    Result<OriginalPolicyReply<'input>, OriginalPolicyReplyRejection>,
    OriginalPolicyReplyError,
> {
    entry(limits, charge)?;
    let live = external(
        context.additional_live,
        encoded.len(),
        workflow_type.len(),
        state,
    );
    let Some(index) = retained(encoded, context, live, limits, charge)? else {
        return Ok(Err(OriginalPolicyReplyRejection::AllocationFailed));
    };
    Ok(Ok(prepare(
        workflow_type,
        state,
        Some(index),
        context,
        limits,
        charge,
    )?))
}

/// Projects an original getter read, including absence and its distinct origins.
///
/// Binary type/query front validation belongs to the caller. Getter decoding
/// binds the atom-keyed stored type; absence resolves defaults at generation zero.
///
/// # Errors
/// Preserves operational/raised origins separately from the four static diagnoses.
pub fn present_original_policy_get_reply<'input>(
    workflow_type: &'input [u8],
    value: OriginalPolicyReadValue<'input>,
    state: Option<&'input [u8]>,
    context: OriginalPolicyReplyContext<'input>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<
    Result<OriginalPolicyReply<'input>, OriginalPolicyReplyRejection>,
    OriginalPolicyReplyError,
> {
    get_reply(workflow_type, value, state, context, limits, charge, None)
}

/// Presents an admitted transient rendering using the fixed portable profile.
/// The donor remains external and must survive every borrowed observation.
/// # Errors
/// Preserves full grammar, profile, work and explicit transient-size refusals.
pub fn present_original_policy_projection_get_reply<'input>(
    workflow_type: &'input [u8],
    encoded: &'input [u8],
    state: Option<&'input [u8]>,
    mut context: OriginalPolicyReplyContext<'input>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    maximum_projection_bytes: usize,
) -> Result<
    Result<OriginalPolicyReply<'input>, OriginalPolicyReplyRejection>,
    OriginalPolicyReplyError,
> {
    context.portable = true;
    get_reply(
        workflow_type,
        OriginalPolicyReadValue::Binary(encoded),
        state,
        context,
        limits,
        charge,
        Some(maximum_projection_bytes),
    )
}

fn get_reply<'input>(
    workflow_type: &'input [u8],
    value: OriginalPolicyReadValue<'input>,
    state: Option<&'input [u8]>,
    context: OriginalPolicyReplyContext<'input>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
    transient: Option<usize>,
) -> Result<
    Result<OriginalPolicyReply<'input>, OriginalPolicyReplyRejection>,
    OriginalPolicyReplyError,
> {
    entry(limits, charge)?;
    let raw = match value {
        OriginalPolicyReadValue::Absent => None,
        OriginalPolicyReadValue::Binary(raw) => Some(raw),
        OriginalPolicyReadValue::Other => return Ok(Err(OriginalPolicyReplyRejection::Corrupt)),
        OriginalPolicyReadValue::Unavailable => {
            return Ok(Err(OriginalPolicyReplyRejection::ShardUnavailable));
        }
        OriginalPolicyReadValue::ReadFailed => {
            return Ok(Err(OriginalPolicyReplyRejection::ReadFailed));
        }
    };
    let index = if let Some(raw) = raw {
        let live = external(
            context.additional_live,
            raw.len(),
            workflow_type.len(),
            state,
        );
        let decoded = match transient {
            Some(maximum) => super::original_entry::indexed::decode_transient_portable_retained(
                raw, maximum, maximum, live, limits, charge,
            ),
            None => retained(raw, context, live, limits, charge),
        };
        let Some(index) = decoded? else {
            return Ok(Err(OriginalPolicyReplyRejection::Corrupt));
        };
        let stored = index.entry().workflow_type();
        let width = workflow_type
            .len()
            .checked_add(stored.map_or(0, <[u8]>::len))
            .ok_or_else(overflow)?;
        pay(charge, 128 + u64::try_from(width).map_err(|_| overflow())?)?;
        if stored != Some(workflow_type) {
            return Ok(Err(OriginalPolicyReplyRejection::Corrupt));
        }
        Some(index)
    } else {
        None
    };
    Ok(Ok(prepare(
        workflow_type,
        state,
        index,
        context,
        limits,
        charge,
    )?))
}

fn prepare<'input>(
    workflow_type: &'input [u8],
    state: Option<&'input [u8]>,
    index: Option<OriginalPolicyCurrentIndex<'input, 'input>>,
    context: OriginalPolicyReplyContext<'input>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalPolicyReply<'input>, OriginalPolicyReplyError> {
    let mut owner = initialize(workflow_type, state, index, context, limits, charge)?;
    let states = owner.states;
    let base = resolve::root(&mut owner, context.defaults, charge)?;
    pay(charge, 128)?;
    owner.records.push(base);
    if let Some(query) = state {
        if owner.index.is_some() {
            let config = match (&owner.index, states) {
                (Some(index), Some(states)) if index.at(states).kind() == Kind::Map => {
                    source::selected(index, states, query, charge)?
                }
                _ => None,
            };
            pay(charge, 128)?;
            let mut selected = resolve::state(&owner, config, Ref::Bytes(query), 0, charge)?;
            if resolve::false_or_nil(&owner, selected.governance) {
                selected.governance = base.governance;
            }
            pay(charge, 128)?;
            owner.records.push(selected);
            owner.selected = 1;
        }
    } else {
        states::all(&mut owner, states, &context, charge)?;
    }
    pay(charge, 128)?;
    Ok(owner)
}

fn retained<'input>(
    raw: &'input [u8],
    context: OriginalPolicyReplyContext<'input>,
    live: usize,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<Option<OriginalPolicyCurrentIndex<'input, 'input>>, EncodingError> {
    if context.portable {
        super::original_entry::indexed::decode_portable_retained(raw, live, limits, charge)
    } else {
        super::original_entry::decode_retained(raw, &context.safety, 263_168, live, limits, charge)
    }
}

fn initialize<'input>(
    workflow_type: &'input [u8],
    state: Option<&'input [u8]>,
    mut index: Option<OriginalPolicyCurrentIndex<'input, 'input>>,
    context: OriginalPolicyReplyContext<'input>,
    limits: PolicySchemaLimits,
    charge: &mut PolicySchemaCharge,
) -> Result<OriginalPolicyReply<'input>, OriginalPolicyReplyError> {
    if workflow_type.len() > 65_522 || state.is_some_and(|value| value.len() > 64 * 1_024 * 1_024) {
        return Err(EncodingError::ByteLimit.into());
    }
    let bytes = index
        .as_ref()
        .map_or(0, |value| value.entry().encoded().len());
    let live = context
        .additional_live
        .checked_add(bytes)
        .and_then(|n| n.checked_add(workflow_type.len()))
        .and_then(|n| n.checked_add(state.map_or(0, <[u8]>::len)))
        .ok_or_else(overflow)?;
    let mut resident = live;
    if let Some(value) = index.as_mut() {
        resident = resident
            .checked_add(value.owner_bound()?)
            .ok_or_else(overflow)?;
        value.build_catalog(resident, charge)?;
        resident = resident
            .checked_add(value.catalog_bound()?)
            .ok_or_else(overflow)?;
    }
    pay(charge, 128)?;
    let states = index
        .as_ref()
        .map(|value| source::field(value, value.cursor().position(), b"states", charge))
        .transpose()?
        .flatten();
    let capacity = capacity(index.as_ref(), state, states)?;
    let depth = index
        .as_ref()
        .map_or(0, OriginalPolicyCurrentIndex::depth_bound);
    let scratch = capacity
        .checked_mul(520)
        .and_then(|n| n.checked_add(1_024))
        .and_then(|n| depth.checked_mul(64).and_then(|d| n.checked_add(d)))
        .ok_or_else(overflow)?;
    resident = resident.checked_add(scratch).ok_or_else(overflow)?;
    peak(charge, resident)?;
    let mut records = Vec::new();
    reserve(&mut records, capacity, 512, charge)?;
    let mut order = Vec::new();
    reserve(&mut order, capacity, 8, charge)?;
    pay(charge, 128)?;
    pay(charge, 256)?;
    let owner = OriginalPolicyReply {
        index,
        requested_type: workflow_type,
        requested_state: state,
        profile: limits,
        resident_bound: resident,
        records,
        order,
        capacity,
        version: Ref::StaticAtom("nil"),
        max_active: Ref::StaticAtom("nil"),
        selected: 0,
        inline: [0; 256],
        coordinates: [(0, 0); 4],
        attributes: 0,
        states,
    };
    Ok(owner)
}

fn capacity(
    index: Option<&OriginalPolicyCurrentIndex<'_, '_>>,
    query: Option<&[u8]>,
    states: Option<TermPosition>,
) -> Result<usize, EncodingError> {
    let Some(index) = index else {
        return Ok(1);
    };
    if query.is_some() {
        return Ok(2);
    }
    let Some(states) = states else {
        return Ok(1);
    };
    let cursor = index.at(states);
    match cursor.kind() {
        Kind::Map => cursor.len().checked_add(1).ok_or_else(overflow),
        Kind::List | Kind::ImproperList => index.count().checked_add(1).ok_or_else(overflow),
        _ => Ok(1),
    }
}

fn reserve<T>(
    target: &mut Vec<T>,
    count: usize,
    width: u64,
    charge: &mut PolicySchemaCharge,
) -> Result<(), EncodingError> {
    let fee = u64::try_from(count)
        .map_err(|_| overflow())?
        .checked_mul(width)
        .and_then(|n| n.checked_add(128))
        .ok_or_else(overflow)?;
    pay(charge, fee)?;
    target
        .try_reserve_exact(count)
        .map_err(|_| EncodingError::AllocationFailure)
}

fn external(extra: usize, bytes: usize, kind: usize, query: Option<&[u8]>) -> usize {
    // Saturation defers an impossible live sum until the decoder's real checked
    // positive R3 admission; malformed first-pass None still wins before reserve.
    extra
        .saturating_add(bytes)
        .saturating_add(kind)
        .saturating_add(query.map_or(0, <[u8]>::len))
}

fn entry(limits: PolicySchemaLimits, charge: &mut PolicySchemaCharge) -> Result<(), EncodingError> {
    if charge.profile() != limits {
        return Err(EncodingError::Schema(PolicySchemaError::ProfileMismatch));
    }
    pay(charge, 128)
}

pub(super) fn observe(
    owner: &OriginalPolicyReply<'_>,
    charge: &mut PolicySchemaCharge,
) -> Result<(), EncodingError> {
    if charge.profile() != owner.profile {
        return Err(EncodingError::Schema(PolicySchemaError::ProfileMismatch));
    }
    charge_schema(
        charge,
        128,
        u64::try_from(owner.resident_bound).map_err(|_| overflow())?,
    )
    .map_err(EncodingError::Schema)
}

pub(super) fn pay(charge: &mut PolicySchemaCharge, amount: u64) -> Result<(), EncodingError> {
    charge_schema(charge, amount, 0).map_err(EncodingError::Schema)
}

pub(super) fn peak(charge: &mut PolicySchemaCharge, bytes: usize) -> Result<(), EncodingError> {
    charge_schema(charge, 0, u64::try_from(bytes).map_err(|_| overflow())?)
        .map_err(EncodingError::Schema)
}

pub(super) fn overflow() -> EncodingError {
    EncodingError::Schema(PolicySchemaError::LengthOverflow)
}
