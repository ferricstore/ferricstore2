//! A stored value as a Flow's payload (audit gap G16): a read that returns
//! the payload resolves the reference on this shard, or marks it missing.

use wf_application::commands::flow::record::{Include, payload_fields};
use wf_application::commands::flow::term::Term;

use super::super::ShardState;
use super::super::key::Parts;

const PAYLOAD_FIELDS: [&[u8]; 4] = [
    b"payload",
    b"payload_size",
    b"payload_omitted",
    b"payload_missing",
];

impl ShardState {
    /// `term` with the payload its Flow names by reference, when the read
    /// returns payloads.
    pub(in crate::supervision::shard::state) fn with_payload_ref(
        &mut self,
        term: Term,
        now: u64,
        key: &Parts,
        include: &Include,
    ) -> Term {
        if !include.payload {
            return term;
        }
        let Some(reference) = self
            .hot(key)
            .and_then(|entry| entry.meta.as_deref())
            .and_then(|meta| meta.payload_ref.clone())
        else {
            return term;
        };
        let Term::Map(mut fields) = term else {
            return term;
        };
        fields.retain(|(name, _)| !PAYLOAD_FIELDS.contains(&name.as_slice()));
        let resolved = match self.value_bytes(now, &reference) {
            Some(bytes) => payload_fields(&bytes, include.max_bytes),
            None => vec![
                ("payload", Term::Nil),
                ("payload_missing", Term::Bool(true)),
            ],
        };
        fields.extend(
            resolved
                .into_iter()
                .map(|(name, value)| (name.as_bytes().to_vec(), value)),
        );
        Term::Map(fields)
    }
}
