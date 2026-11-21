//! Payload, result and error versions, current or kept (ADR-016 phase C),
//! as history with `values: true` shows them.

use wf_application::commands::flow::record::{
    FlowMeta, RETAINED_ERROR, RETAINED_PAYLOAD, RETAINED_RESULT,
};
use wf_application::commands::flow::term::Term;
use wf_domain::workflow::WorkflowRecord;

/// The bytes of a payload, result or error version, current or kept, and
/// whether they are a term (ADR-016 phase C).
pub(in crate::supervision::shard::state) fn version_bytes<'a>(
    record: &'a WorkflowRecord,
    meta: &'a FlowMeta,
    kind: u8,
    version: u64,
) -> Option<(&'a [u8], bool)> {
    let (current, stored, term) = match kind {
        RETAINED_PAYLOAD => (record.payload(), meta.payload_version, meta.payload_term),
        RETAINED_RESULT => (record.result(), meta.result_version, false),
        _ => (record.error(), meta.error_version, meta.error_term),
    };
    match current {
        Some(bytes) if stored == version => Some((bytes, term)),
        _ => meta
            .retained(kind, b"", version)
            .map(|kept| (kept.bytes.as_slice(), kept.term)),
    }
}

/// A history event with the payload, result and error it named, as
/// `FLOW.HISTORY values: true` shows them.
pub(in crate::supervision::shard::state) fn with_values(
    event: Term,
    record: &WorkflowRecord,
    meta: &FlowMeta,
    refs: [Option<u64>; 3],
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
            let Some((bytes, term)) = version.and_then(|v| version_bytes(record, meta, kind, v))
            else {
                continue;
            };
            let value = match (term, Term::decode(bytes)) {
                (true, Some((decoded, []))) => decoded,
                _ => Term::Bin(bytes.to_vec()),
            };
            fields.push((name.as_bytes().to_vec(), value));
        }
    }
    Term::List(parts)
}
