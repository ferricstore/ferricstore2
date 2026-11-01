//! Captured atom fields, original public partition projection and exact identity.
use super::{
    I,
    budget::{Budget, add, scope},
    model::OriginalQueryRecordError as E,
    primitive,
};
use crate::blob::{owns_original_state_key, parse_original_state_key};
use wf_domain::metadata::scope::{
    original_logical_partition, original_physical_partition, original_system_metadata_scope,
};

pub(super) const NAMES: [&str; 25] = [
    "id",
    "type",
    "state",
    "version",
    "priority",
    "partition_key",
    "created_at_ms",
    "updated_at_ms",
    "next_run_at_ms",
    "lease_deadline_ms",
    "attempts",
    "run_state",
    "max_active_ms",
    "parent_flow_id",
    "root_flow_id",
    "correlation_id",
    "state_enter_seq",
    "history_max_events",
    "history_hot_max_events",
    "lease_owner",
    "attributes",
    "state_meta",
    "indexed_attributes",
    "indexed_state_meta",
    "system_metadata",
];
pub(super) struct Fields<'r, 'a> {
    values: [Option<I<'r, 'a>>; 25],
}
impl<'r, 'a> Fields<'r, 'a> {
    pub(super) fn get(&self, index: usize) -> Option<I<'r, 'a>> {
        self.values.get(index).copied().flatten()
    }
    pub(super) fn read(record: I<'r, 'a>, budget: &mut Budget) -> Result<Self, E> {
        let I::Map(rows) = record else {
            return Err(E::InvalidRecord);
        };
        let mut fields = Self { values: [None; 25] };
        for &(key, value) in rows {
            budget.spend(25)?;
            let I::Atom(name) = key else {
                continue;
            };
            let Some(index) = NAMES.iter().position(|s| *s == name) else {
                continue;
            };
            let slot = fields.values.get_mut(index).ok_or(E::Limit)?;
            if slot.is_some() {
                return Err(E::InvalidRecord);
            }
            *slot = Some(value);
        }
        Ok(fields)
    }
}
pub(super) struct RecordPlan<'r, 'a> {
    pub(super) fields: Fields<'r, 'a>,
    pub(super) id: &'a [u8],
    pub(super) version: u64,
    pub(super) partition: Option<&'a [u8]>,
    pub(super) scope: Option<Vec<u8>>,
    pub(super) root_is_id: bool,
    pub(super) sections: [usize; 5],
    pub(super) metadata_bytes: usize,
}

pub(super) fn plan<'r, 'a>(
    state_key: I<'_, '_>,
    record: I<'r, 'a>,
    budget: &mut Budget,
) -> Result<RecordPlan<'r, 'a>, E> {
    let I::Binary(state_key) = state_key else {
        return Err(E::InvalidRecord);
    };
    if state_key.is_empty() || state_key.len() > 65_535 {
        return Err(E::InvalidRecord);
    }
    budget.spend(add(state_key.len(), 49)?)?;
    let key_id = parse_original_state_key(state_key)
        .filter(|id| id.len() <= 65483)
        .ok_or(E::InvalidRecord)?;
    let fields = Fields::read(record, budget)?;
    let metadata = fields.get(24).unwrap_or(I::Map(&[]));
    let derived = original_system_metadata_scope(metadata, budget.peer_limits()?).map_err(scope)?;
    budget.peer(derived.work_units, derived.temporary_bytes)?;
    budget.admit(derived.value.as_ref().map_or(0, Vec::len))?;
    let partition = logical(&fields, metadata, derived.value.as_deref(), budget)?;
    let Some(I::Binary(id)) = fields.get(0) else {
        return Err(E::InvalidRecord);
    };
    budget.spend(id.len())?;
    if id.is_empty() || id.len() > 65483 || id != key_id {
        return Err(E::InvalidRecord);
    }
    let version = primitive::uint(fields.get(3).ok_or(E::InvalidRecord)?, budget)?;
    if version > 9_007_199_254_740_991 {
        return Err(E::InvalidRecord);
    }
    bind(state_key, id, partition, derived.value.as_deref(), budget)?;
    let root_is_id = match fields.get(14) {
        Some(I::Binary(root)) => {
            budget.spend(root.len())?;
            root == id
        }
        _ => false,
    };
    let mut plan = RecordPlan {
        fields,
        id,
        version,
        partition,
        scope: derived.value,
        root_is_id,
        sections: [0; 5],
        metadata_bytes: 0,
    };
    plan.sections = super::metadata::measure(&plan, budget)?;
    plan.metadata_bytes = plan.sections.iter().try_fold(11, |n, size| add(n, *size))?;
    if add(plan.metadata_bytes, plan.scope.as_ref().map_or(0, Vec::len))? > 32629 {
        return Err(E::MetadataTooLarge);
    }
    Ok(plan)
}
fn logical<'a>(
    fields: &Fields<'_, 'a>,
    metadata: I<'_, '_>,
    isolation: Option<&[u8]>,
    budget: &mut Budget,
) -> Result<Option<&'a [u8]>, E> {
    match fields.get(5) {
        Some(I::Binary(bytes)) => {
            let logical = if let Some(scope_prefix) = isolation {
                let result = original_logical_partition(bytes, scope_prefix, budget.peer_limits()?)
                    .map_err(scope)?;
                budget.peer(result.work_units, result.temporary_bytes)?;
                result.value
            } else {
                bytes
            };
            if logical.is_empty() {
                return Err(E::InvalidRecord);
            }
            Ok(Some(logical))
        }
        Some(value)
            if primitive::nil(value) && matches!(metadata, I::Map(rows) if rows.is_empty()) =>
        {
            Ok(None)
        }
        _ => Err(E::InvalidRecord),
    }
}
fn bind(
    key: &[u8],
    id: &[u8],
    logical: Option<&[u8]>,
    isolation: Option<&[u8]>,
    budget: &mut Budget,
) -> Result<(), E> {
    let before = budget.live();
    let physical = if let Some(partition) = logical {
        let result = original_physical_partition(partition, isolation, budget.peer_limits()?)
            .map_err(scope)?;
        budget.peer(result.work_units, result.temporary_bytes)?;
        if let std::borrow::Cow::Owned(bytes) = &result.value {
            budget.admit(bytes.len())?;
        }
        Some(result.value)
    } else {
        None
    };
    let result = owns_original_state_key(
        Some(I::Binary(id)),
        Some(physical.as_deref().map_or(I::Nil, I::Binary)),
        None,
        I::Binary(key),
        budget.peer_limits()?,
    )
    .map_err(scope)?;
    budget.peer(result.work_units, result.temporary_bytes)?;
    drop(physical);
    budget.release_to(before);
    if result.value {
        Ok(())
    } else {
        Err(E::InvalidRecord)
    }
}
