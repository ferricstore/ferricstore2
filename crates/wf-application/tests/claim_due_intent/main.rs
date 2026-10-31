//! Public mathematical claim identity controls; no service or apply authority.
#![forbid(unsafe_code)]

use std::error::Error;

use wf_application::commands::{IntentLimits, prepare_claim_due_intent};
use wf_domain::claim::caller::{
    ClaimCallerLimits, ClaimDueInput, ClaimPartitionsInput, ClaimReplyMode, ClaimStatesInput,
    ClaimValuesInput, NormalizedClaimDue, normalize_claim_due,
};
use wf_domain::workflow::request::ClientIntent;
use wf_types::identity::TenantId;

mod fields;
mod ports;
mod vectors;

type TestError = Box<dyn Error>;
type TestResult<T = ()> = Result<T, TestError>;

fn fixture_input(name: &str) -> TestResult<ClaimDueInput<'static>> {
    let mut input = ClaimDueInput::new(b"t\xff", b"w\xff");
    match name {
        "defaults" | "values_omitted" => {}
        "explicit_fixed_defaults" => {
            input.lease_ms = Some(30_000);
            input.limit = Some(1);
            input.reclaim_expired = Some(true);
            input.reclaim_ratio = Some(25);
            input.reply_mode = Some(ClaimReplyMode::Records);
            input.full = Some(true);
            input.payload = Some(true);
        }
        "auto" => input.partitions = ClaimPartitionsInput::One(b"aUtO"),
        "any" => input.partitions = ClaimPartitionsInput::Any,
        "global" => input.partitions = ClaimPartitionsInput::Global,
        "state_keep_last" => {
            input.states = ClaimStatesInput::Many(&[b"queued" as &[u8], b"ready", b"queued"]);
        }
        "partition_keep_first" => {
            input.partitions = ClaimPartitionsInput::Many(&[b"p2", b"p1", b"p2"]);
        }
        "values_empty" => input.values = ClaimValuesInput::Names(&[]),
        "opaque_governance_zero" => {
            input.governance_limit_scope = Some(b"\0\xff");
            input.governance_shard_id = Some(0);
        }
        "requested_now_zero" => input.requested_now_ms = Some(0),
        "invalid_scalars" => {
            input.lease_ms = Some(0);
            input.limit = Some(0);
            input.reclaim_ratio = Some(u64::MAX);
            input.priority = Some(u64::MAX);
            input.payload_max_bytes = Some(u64::MAX);
        }
        _ => return Err("unknown independent public claim fixture".into()),
    }
    Ok(input)
}

fn normalized(input: ClaimDueInput<'_>) -> TestResult<NormalizedClaimDue<'_>> {
    Ok(normalize_claim_due(input, ClaimCallerLimits::default())?)
}

fn intent(tenant: &TenantId, input: ClaimDueInput<'_>) -> TestResult<ClientIntent> {
    let request = normalized(input)?;
    Ok(prepare_claim_due_intent(
        tenant,
        &request,
        IntentLimits::default(),
    )?)
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                char::from(DIGITS[usize::from(byte >> 4)]),
                char::from(DIGITS[usize::from(byte & 15)]),
            ]
        })
        .collect()
}
