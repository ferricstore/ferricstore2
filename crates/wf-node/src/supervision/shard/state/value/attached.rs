//! References a Flow attaches by name but does not own (audit gap G9).

use std::collections::BTreeMap;

use wf_application::commands::flow::record::{FlowMeta, RETAINED_NAMED, Values};
use wf_application::commands::flow::term::Term;
use wf_application::commands::flow::value_codec;
use wf_application::commands::queue::{Partition, ServiceError};

use super::super::{Entry, ShardState};

/// The attached references a read asks for by name (audit gap G9).
pub(in crate::supervision::shard::state) fn requested_refs(
    entry: &Entry,
    values: &Values,
) -> Vec<(Vec<u8>, Vec<u8>)> {
    let Some(meta) = entry.meta.as_deref() else {
        return Vec::new();
    };
    meta.value_refs
        .iter()
        .filter(|(name, _)| match values {
            Values::None => false,
            Values::All => true,
            Values::Names(names) => names.contains(name),
        })
        .map(|(name, reference)| (name.clone(), reference.clone()))
        .collect()
}

/// Attaches references by name (audit gap G9); one replaces the Flow's own
/// value of that name, which stays readable by version.
///
/// # Errors
/// `CROSSSLOT` for a reference outside the Flow's slot (audit gap G11).
pub(in crate::supervision::shard::state) fn attach(
    meta: &mut FlowMeta,
    refs: &BTreeMap<Vec<u8>, Vec<u8>>,
    partition: &Partition,
) -> Result<(), ServiceError> {
    for reference in refs.values() {
        super::held::check_slot(partition, reference)?;
    }
    for (name, reference) in refs {
        if let Some(value) = meta.values.remove(name) {
            meta.retain((RETAINED_NAMED, name, value.version), &value.bytes);
        }
        meta.value_refs.insert(name.clone(), reference.clone());
    }
    Ok(())
}

impl ShardState {
    /// A read's `values` with the attached references it asked for that
    /// resolve on this shard (audit gap G9); others are left out, as
    /// Elixir leaves out references it cannot read.
    pub(in crate::supervision::shard::state) fn with_attached(
        &mut self,
        term: Term,
        now: u64,
        attached: Vec<(Vec<u8>, Vec<u8>)>,
    ) -> Term {
        let Term::Map(mut fields) = term else {
            return term;
        };
        let resolved: Vec<(Vec<u8>, Term)> = attached
            .into_iter()
            .filter_map(|(name, reference)| {
                let bytes = self.value_bytes(now, &reference)?;
                Some((name, value_codec::load(&bytes)))
            })
            .collect();
        if resolved.is_empty() {
            return Term::Map(fields);
        }
        match fields
            .iter_mut()
            .find(|(key, _)| key.as_slice() == b"values")
        {
            Some((_, Term::Map(values))) => values.extend(resolved),
            _ => fields.push((b"values".to_vec(), Term::Map(resolved))),
        }
        Term::Map(fields)
    }
}
