//! Payload, result and error versions, current or kept (ADR-016 phase C),
//! as history with `values: true` shows them.

use std::borrow::Cow;
use wf_application::commands::flow::record::{
    FlowMeta, RETAINED_ERROR, RETAINED_PAYLOAD, RETAINED_RESULT, hydrated,
};

use wf_application::commands::flow::term::Term;
use wf_domain::workflow::WorkflowRecord;
use wf_storage::cold::ColdStore;

use super::super::super::image;

/// The stored bytes of a payload, result or error version, current or kept
/// (ADR-016 phase C), read from `store` when on disk (phase D).
pub(in crate::supervision::shard::state) fn version_bytes<'a>(
    (record, meta): (&'a WorkflowRecord, &'a FlowMeta),
    store: &ColdStore,
    kind: u8,
    version: u64,
) -> Option<Cow<'a, [u8]>> {
    let (current, stored) = match kind {
        RETAINED_PAYLOAD => (record.payload(), meta.payload_version),
        RETAINED_RESULT => (record.result(), meta.result_version),
        _ => (record.error(), meta.error_version),
    };
    match current {
        Some(bytes) if stored == version => Some(Cow::Borrowed(bytes)),
        _ => meta
            .retained(kind, b"", version)
            .and_then(|kept| image::kept_bytes(store, kept)),
    }
}

/// A history event with the payload, result and error it named, as
/// `FLOW.HISTORY values: true` shows them.
pub(in crate::supervision::shard::state) fn with_values(
    event: Term,
    (record, meta): (&WorkflowRecord, &FlowMeta),
    store: &ColdStore,
    (refs, max_bytes): ([Option<u64>; 3], u64),
) -> Term {
    let Term::List(mut parts) = event else {
        return event;
    };
    if let Some(Term::Map(fields)) = parts.get_mut(1) {
        let kinds = [
            ("payload", RETAINED_PAYLOAD),
            ("result", RETAINED_RESULT),
            ("error", RETAINED_ERROR),
        ];
        for ((name, kind), version) in kinds.into_iter().zip(refs) {
            // Replaced versions are kept while the Flow lives (audit gap
            // G37); each is shown as Elixir's history hydration shows it:
            // the value and its size, omitted above `max_bytes` (G36).
            let Some(bytes) = version.and_then(|v| version_bytes((record, meta), store, kind, v))
            else {
                continue;
            };
            fields.extend(
                hydrated(name, &bytes, max_bytes)
                    .into_iter()
                    .map(|(field, value)| (field.as_bytes().to_vec(), value)),
            );
        }
    }
    Term::List(parts)
}
