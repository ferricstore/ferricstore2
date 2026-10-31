//! Exact lexicographic public Claim V1 argument keys and scalar encodings.
use super::{CodecError as E, IntentSink, NormalizedClaimDue, compound, header, scalar};
use crate::log::public_mutation::intent::{bytes, number};
use wf_domain::claim::caller::ClaimReplyMode;

pub(super) fn write(caller: &NormalizedClaimDue<'_>, out: &mut IntentSink<'_>) -> Result<(), E> {
    bytes(
        "governance_limit_scope",
        caller.governance_limit_scope(),
        out,
    )?;
    number("governance_shard_id", caller.governance_shard_id(), out)?;
    number("lease_ms", Some(caller.lease_ms()), out)?;
    number("limit", Some(caller.limit()), out)?;
    number("now_ms", caller.requested_now_ms(), out)?;
    scalar(5, b"partitions", out)?;
    compound::partitions(caller, out)?;
    boolean("payload", caller.payload_enabled(), out)?;
    number("payload_max_bytes", caller.payload_max_bytes(), out)?;
    number("priority", caller.priority(), out)?;
    boolean("reclaim_expired", caller.reclaim_expired(), out)?;
    number("reclaim_ratio", Some(caller.reclaim_ratio()), out)?;
    scalar(5, b"reply_mode", out)?;
    scalar(
        5,
        match caller.reply_mode() {
            ClaimReplyMode::Records => b"records",
            ClaimReplyMode::Jobs => b"jobs",
            ClaimReplyMode::Compact => b"jobs_compact",
            ClaimReplyMode::CompactAttrs => b"jobs_compact_attrs",
            ClaimReplyMode::CompactState => b"jobs_compact_state",
            ClaimReplyMode::CompactStateAttrs => b"jobs_compact_state_attrs",
        },
        out,
    )?;
    scalar(5, b"states", out)?;
    compound::states(caller, out)?;
    scalar(5, b"values", out)?;
    compound::values(caller, out)?;
    bytes("worker", Some(caller.worker()), out)?;
    bytes("workflow_type", Some(caller.workflow_type()), out)?;
    Ok(())
}

pub(super) fn boolean(key: &str, value: bool, out: &mut IntentSink<'_>) -> Result<(), E> {
    scalar(5, key.as_bytes(), out)?;
    header(if value { 2 } else { 1 }, 0, out)
}
