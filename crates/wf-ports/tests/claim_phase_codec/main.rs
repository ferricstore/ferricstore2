//! Independent private claim bytes and bounded decoded-envelope controls.
#![forbid(unsafe_code)]

use std::error::Error;

use wf_domain::workflow::request::{ClientIntent, RequestScope};
use wf_ports::consensus::{
    PreparedClaimPhase, PreparedClaimQuery, PreparedOperation, PreparedRequest,
};
use wf_types::identity::{GroupId, RecoveryId, RequestId, TenantId};
use wf_types::version::{CommittedTime, HlcTimestamp};

mod admission;
mod apply;
mod dispatch;
mod envelope;
mod vectors;

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn default_phase() -> PreparedClaimPhase {
    PreparedClaimPhase {
        query: PreparedClaimQuery {
            workflow_type: vec![b't', 255],
            states: None,
            exclude_states: Vec::new(),
            partitions: None,
            priority: None,
            worker: vec![b'w', 255],
            lease_ms: 50,
            limit: 1,
        },
        scan_budget: 3,
        head_budget: 4,
        token_version: 1,
        first_ordinal: u32::MAX,
    }
}

fn phase(name: &str) -> TestResult<PreparedClaimPhase> {
    let mut value = default_phase();
    let query = &mut value.query;
    match name {
        "claim_any" => {}
        "claim_states_empty" => query.states = Some(Vec::new()),
        "claim_states" | "claim_states_reverse" => {
            let mut values = vec![b"waiting".to_vec(), b"running".to_vec()];
            if name.ends_with("reverse") {
                values.reverse();
            }
            query.states = Some(values);
        }
        "claim_excluded" | "claim_excluded_reverse" => {
            query.exclude_states = vec![b"waiting".to_vec(), b"signal_wait".to_vec()];
            if name.ends_with("reverse") {
                query.exclude_states.reverse();
            }
        }
        "claim_partitions" | "claim_partitions_reverse" => {
            let mut values = vec![vec![0, 255], b"p".to_vec()];
            if name.ends_with("reverse") {
                values.reverse();
            }
            query.partitions = Some(values);
        }
        "claim_partitions_empty" => query.partitions = Some(Vec::new()),
        "claim_priority_zero" => query.priority = Some(0),
        "claim_priority_max" => query.priority = Some(255),
        "claim_empty_type" => query.workflow_type.clear(),
        "claim_empty_worker" => query.worker.clear(),
        "claim_lease_zero" => query.lease_ms = 0,
        "claim_lease_max" => query.lease_ms = u64::MAX,
        "claim_budgets_zero" => {
            query.limit = 0;
            value.scan_budget = 0;
            value.head_budget = 0;
        }
        "claim_budgets_max" => {
            query.limit = usize::try_from(u32::MAX)?;
            value.scan_budget = usize::try_from(u32::MAX)?;
            value.head_budget = usize::try_from(u32::MAX)?;
        }
        "claim_token_zero" => value.token_version = 0,
        "claim_token_max" => value.token_version = u16::MAX,
        "claim_ordinal_zero" => value.first_ordinal = 0,
        "claim_rich" => {
            query.states = Some(vec![b"waiting".to_vec(), b"running".to_vec()]);
            query.exclude_states = vec![b"waiting".to_vec()];
            query.partitions = Some(vec![vec![0, 255], b"p".to_vec()]);
            query.priority = Some(2);
            query.lease_ms = 7;
            query.limit = 5;
            value.scan_budget = 9;
            value.head_budget = 11;
            value.first_ordinal = 7;
        }
        _ => return Err("unknown claim fixture".into()),
    }
    Ok(value)
}

fn request(name: &str) -> TestResult<PreparedRequest> {
    Ok(PreparedRequest {
        scope: RequestScope::new(
            TenantId::new(vec![0, 255])?,
            GroupId::new(b"G".to_vec())?,
            RecoveryId::new(b"R".to_vec())?,
        ),
        request: RequestId::new(b"Q".to_vec())?,
        intent: ClientIntent::new(
            1,
            std::array::from_fn(|index| u8::try_from(index).unwrap_or_default()),
        )?,
        stamp: CommittedTime::new(HlcTimestamp::new(11, 2)?, 7)?,
        operation: PreparedOperation::ClaimPhase(phase(name)?),
    })
}

fn bytes(hex: &str) -> TestResult<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return Err("odd fixture hex".into());
    }
    hex.as_bytes()
        .chunks(2)
        .map(|pair| -> TestResult<u8> { Ok(u8::from_str_radix(std::str::from_utf8(pair)?, 16)?) })
        .collect()
}
