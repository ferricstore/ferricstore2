//! The Flows a schedule creates: their ids, their `FLOW.CREATE` options,
//! and the ownership attributes that let a retried fire recognise a target
//! it already created (`TargetOwnership`).
//!
//! The ownership digest is an HMAC-SHA256 keyed by the schedule's secret
//! over the schedule id, the target id and the target definition, as
//! Elixir's; the bytes it covers are this node's own term encoding.

use super::super::query::cursor;
use super::super::term::{Term, map};
use super::FLOW_ID_PREFIX;
use super::definition::{Definition, Kind};
use crate::routing::base64url_nopad;

/// The attribute holding a target's ownership digest.
pub const OWNER_ATTRIBUTE: &str = "ferricstore.schedule.owner.v1";
/// The attribute holding the owning schedule's id.
pub const SCHEDULE_ATTRIBUTE: &str = "ferricstore.schedule.id.v1";

/// The target a fire due at `due_at_ms` creates as fire number
/// `fire_count`: a one-shot's fixed id, or `<prefix>:<due>:<count>`.
#[must_use]
pub fn target_id(definition: &Definition, due_at_ms: u64, fire_count: u64) -> Vec<u8> {
    if let (Kind::OneShot | Kind::Delay, Some(id)) = (definition.kind, definition.target_text("id"))
    {
        return id.to_vec();
    }
    let prefix = definition
        .target_text("id_prefix")
        .unwrap_or(&definition.id);
    let mut id = prefix.to_vec();
    id.extend_from_slice(format!(":{due_at_ms}:{fire_count}").as_bytes());
    id
}

/// The correlation id targets carry: the target's own, or the schedule's
/// Flow id.
#[must_use]
pub fn correlation(definition: &Definition) -> Vec<u8> {
    definition.target_text("correlation_id").map_or_else(
        || {
            let mut id = FLOW_ID_PREFIX.as_bytes().to_vec();
            id.extend_from_slice(&definition.id);
            id
        },
        <[u8]>::to_vec,
    )
}

/// The ownership digest of `target_id`.
#[must_use]
pub fn digest(definition: &Definition, target_id: &[u8]) -> String {
    let mut key = [0_u8; 32];
    for (slot, byte) in key.iter_mut().zip(&definition.secret) {
        *slot = *byte;
    }
    let mut message = Vec::new();
    let covered = Term::List(vec![
        Term::Bin(definition.id.clone()),
        Term::Bin(target_id.to_vec()),
        definition.target.clone(),
    ]);
    // A definition that was stored encodes again.
    let _encoded = covered.encode(&mut message);
    base64url_nopad(&cursor::hmac(&key, &message))
}

/// The `FLOW.CREATE` options of a target.
#[must_use]
pub fn create_options(definition: &Definition, target_id: &[u8], now_ms: u64) -> Term {
    let target = &definition.target;
    let mut entries: Vec<(&str, Term)> = vec![
        ("id", Term::Bin(target_id.to_vec())),
        ("type", target.get(b"type").cloned().unwrap_or(Term::Nil)),
        (
            "state",
            target
                .get(b"state")
                .cloned()
                .unwrap_or_else(|| Term::text("queued")),
        ),
        ("correlation_id", Term::Bin(correlation(definition))),
        (
            "attributes",
            map(vec![
                (
                    OWNER_ATTRIBUTE,
                    Term::Bin(digest(definition, target_id).into_bytes()),
                ),
                (SCHEDULE_ATTRIBUTE, Term::Bin(definition.id.clone())),
            ]),
        ),
        ("now_ms", Term::int(now_ms)),
        (
            "run_at_ms",
            target
                .get(b"run_at_ms")
                .cloned()
                .unwrap_or_else(|| Term::int(now_ms)),
        ),
    ];
    for key in [
        "partition_key",
        "priority",
        "parent_flow_id",
        "root_flow_id",
        "payload",
        "payload_ref",
        "values",
        "value_refs",
    ] {
        if let Some(value) = target.get(key.as_bytes()) {
            entries.push((key, value.clone()));
        }
    }
    map(entries)
}

/// Whether an existing Flow (its GET map) is the target this schedule
/// created: same type and correlation, and its ownership digest.
#[must_use]
pub fn owned(record: &Term, definition: &Definition, target_id: &[u8]) -> bool {
    let expected = digest(definition, target_id);
    record.get(b"type") == definition.target.get(b"type")
        && record.get(b"correlation_id") == Some(&Term::Bin(correlation(definition)))
        && record
            .get(b"attributes")
            .and_then(|attributes| attributes.get(OWNER_ATTRIBUTE.as_bytes()))
            == Some(&Term::Bin(expected.into_bytes()))
}
