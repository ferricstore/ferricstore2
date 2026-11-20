//! Translates queue requests to the service and its results to replies.
//!
//! Batch replies follow the Elixir server: when every item succeeded, a
//! single `OK` if the client asked for it (or the completion batch was not
//! independent), otherwise a list of `OK`, compact when negotiated; when any
//! item failed, a list of `ok` or `[error, message]` per item. Options the
//! service does not implement are refused by name.

use wf_application::commands::queue::{
    ClaimFlows, ClaimedFlow, CompleteFlow, CreateFlows, ItemOutcome, NewFlow, ServiceError,
};

use crate::frame::{FLAG_CUSTOM_PAYLOAD, Header};
use crate::request::{self, ClaimShape, Partitions};
use crate::response::{self, ClaimedJob};

use super::connection::ReplyTo;
use super::dispatch::{Context, Negotiated, refuse};

/// Most Flows one claim may return, as the handshake declares.
const MAX_CLAIM: usize = 10_000;

fn priority(value: i64) -> Result<u8, String> {
    u8::try_from(value)
        .ok()
        .filter(|priority| *priority <= 2)
        .ok_or_else(|| "ERR flow priority must be 0, 1 or 2".to_owned())
}

fn millis(value: i64, field: &str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("ERR flow {field} must not be negative"))
}

fn batch_reply(
    reply: ReplyTo,
    result: Result<Vec<ItemOutcome>, ServiceError>,
    single_ok: bool,
    compact: bool,
) {
    let outcomes = match result {
        Ok(outcomes) => outcomes,
        Err(error) => return refuse(reply, &error.to_string()),
    };
    if outcomes.iter().all(Result::is_ok) {
        let (flags, body) = if single_ok {
            (0, response::ok_text(b"OK"))
        } else if compact {
            (
                FLAG_CUSTOM_PAYLOAD,
                response::compact_ok_list(outcomes.len()),
            )
        } else {
            (0, response::ok_text_list(outcomes.len()))
        };
        return reply.send(flags, &body.unwrap_or_default());
    }
    let messages: Vec<Option<String>> = outcomes
        .iter()
        .map(|outcome| outcome.err().map(|error| error.to_string()))
        .collect();
    let items: Vec<response::ItemOutcome<'_>> = messages
        .iter()
        .map(|message| {
            message
                .as_deref()
                .map_or(response::ItemOutcome::Ok, response::ItemOutcome::Error)
        })
        .collect();
    reply.send(0, &response::item_outcomes(&items).unwrap_or_default());
}

fn single_reply(reply: ReplyTo, result: Result<Vec<ItemOutcome>, ServiceError>) {
    match result.map(|outcomes| outcomes.into_iter().next()) {
        Ok(Some(Ok(()))) => reply.send(0, &response::ok_text(b"OK").unwrap_or_default()),
        Ok(Some(Err(error))) | Err(error) => refuse(reply, &error.to_string()),
        Ok(None) => refuse(reply, &ServiceError::Unavailable.to_string()),
    }
}

pub(super) fn create_many(
    context: &Context,
    negotiated: Negotiated,
    _header: &Header,
    body: &[u8],
    reply: ReplyTo,
) {
    let parsed = match request::create_many(body) {
        Ok(parsed) => parsed,
        Err(error) => return refuse(reply, error.message()),
    };
    if !parsed.independent {
        return refuse(
            reply,
            "ERR FLOW.CREATE_MANY without independent items is not supported by this server",
        );
    }
    let shape = priority(parsed.priority).and_then(|priority| {
        Ok((
            priority,
            parsed
                .run_at_ms
                .map(|at| millis(at, "run_at_ms"))
                .transpose()?,
        ))
    });
    let (priority, run_at_ms) = match shape {
        Ok(shape) => shape,
        Err(message) => return refuse(reply, &message),
    };
    let shared_partition = parsed.partition_key;
    let flows = parsed
        .items
        .into_iter()
        .map(|item| NewFlow {
            id: item.id,
            partition: item.partition_key.or_else(|| shared_partition.clone()),
            payload: item.payload,
        })
        .collect();
    let request = CreateFlows {
        flow_type: parsed.flow_type,
        state: parsed.state,
        run_at_ms,
        priority,
        flows,
    };
    let (single_ok, compact) = (parsed.ok_on_success, negotiated.ok_list);
    context.service.create(
        request,
        Box::new(move |result| batch_reply(reply, result, single_ok, compact)),
    );
}

pub(super) fn create(context: &Context, body: &[u8], reply: ReplyTo) {
    let parsed = match request::create(body) {
        Ok(parsed) => parsed,
        Err(error) => return refuse(reply, error.message()),
    };
    let shape = priority(parsed.priority).and_then(|priority| {
        Ok((
            priority,
            parsed
                .run_at_ms
                .map(|at| millis(at, "run_at_ms"))
                .transpose()?,
        ))
    });
    let (priority, run_at_ms) = match shape {
        Ok(shape) => shape,
        Err(message) => return refuse(reply, &message),
    };
    let flows = vec![NewFlow {
        id: parsed.id,
        partition: parsed.partition_key,
        payload: parsed.payload,
    }];
    let request = CreateFlows {
        flow_type: parsed.flow_type,
        state: parsed.state,
        run_at_ms,
        priority,
        flows,
    };
    context
        .service
        .create(request, Box::new(move |result| single_reply(reply, result)));
}

fn claim_reply(
    reply: ReplyTo,
    result: Result<Vec<ClaimedFlow>, ServiceError>,
    shape: ClaimShape,
    compact: bool,
) {
    let flows = match result {
        Ok(flows) => flows,
        Err(error) => return refuse(reply, &error.to_string()),
    };
    let jobs: Vec<ClaimedJob<'_>> = flows
        .iter()
        .map(|flow| ClaimedJob {
            id: &flow.id,
            partition: &flow.partition,
            lease_token: &flow.lease_token,
            fencing_token: i64::try_from(flow.fence).unwrap_or(i64::MAX),
            run_state: &flow.run_state,
        })
        .collect();
    if compact {
        reply.send(
            FLAG_CUSTOM_PAYLOAD,
            &response::compact_claim_jobs(&jobs, shape).unwrap_or_default(),
        );
    } else {
        reply.send(
            0,
            &response::typed_claim_jobs(&jobs, shape).unwrap_or_default(),
        );
    }
}

pub(super) fn claim_due(
    context: &Context,
    negotiated: Negotiated,
    header: &Header,
    body: &[u8],
    reply: ReplyTo,
) {
    let parsed = match request::claim_due(header.flags, body) {
        Ok(parsed) => parsed,
        Err(error) => return refuse(reply, error.message()),
    };
    let Some(ratio) = u8::try_from(parsed.reclaim_ratio)
        .ok()
        .filter(|ratio| *ratio <= 100)
    else {
        return refuse(
            reply,
            "ERR flow reclaim_ratio must be an integer between 0 and 100",
        );
    };
    if parsed.block_ms.is_some() {
        return refuse(
            reply,
            "ERR blocking FLOW.CLAIM_DUE is not supported by this server",
        );
    }
    let limit = usize::try_from(parsed.limit)
        .ok()
        .filter(|limit| (1..=MAX_CLAIM).contains(limit));
    let lease_ms = u64::try_from(parsed.lease_ms)
        .ok()
        .filter(|lease| *lease > 0);
    let priority = parsed.priority.map(priority).transpose();
    let (Some(limit), Some(lease_ms), Ok(priority)) = (limit, lease_ms, priority) else {
        return refuse(
            reply,
            "ERR FLOW.CLAIM_DUE needs a limit of 1 to 10000, a positive lease_ms and a priority of 0 to 2",
        );
    };
    let partitions = match parsed.partitions {
        Partitions::Any => None,
        Partitions::One(partition) => Some(vec![partition]),
        Partitions::Many(partitions) => Some(partitions),
    };
    let request = ClaimFlows {
        flow_type: parsed.flow_type,
        state: parsed.state,
        worker: parsed.worker,
        lease_ms,
        limit,
        priority,
        partitions,
        reclaim: parsed.reclaim_expired.then_some(ratio),
    };
    let (shape, compact) = (parsed.shape, negotiated.claim_jobs);
    context.service.claim(
        request,
        Box::new(move |result| claim_reply(reply, result, shape, compact)),
    );
}

fn completion(
    id: Vec<u8>,
    partition: Option<Vec<u8>>,
    lease_token: Vec<u8>,
    fencing_token: i64,
) -> Option<CompleteFlow> {
    Some(CompleteFlow {
        id,
        partition,
        lease_token,
        fence: u64::try_from(fencing_token).ok()?,
        result: None,
    })
}

pub(super) fn complete_many(
    context: &Context,
    negotiated: Negotiated,
    header: &Header,
    body: &[u8],
    reply: ReplyTo,
) {
    let parsed = match request::complete_many(header.flags, body) {
        Ok(parsed) => parsed,
        Err(error) => return refuse(reply, error.message()),
    };
    let shared_partition = parsed.partition_key;
    let completions: Option<Vec<CompleteFlow>> = parsed
        .items
        .into_iter()
        .map(|item| {
            completion(
                item.id,
                item.partition_key.or_else(|| shared_partition.clone()),
                item.lease_token,
                item.fencing_token,
            )
        })
        .collect();
    let Some(completions) = completions else {
        return refuse(reply, "ERR flow fencing_token must not be negative");
    };
    let single_ok = parsed.ok_on_success || !parsed.independent;
    let compact = negotiated.ok_list;
    context.service.complete(
        completions,
        Box::new(move |result| batch_reply(reply, result, single_ok, compact)),
    );
}

pub(super) fn complete(context: &Context, body: &[u8], reply: ReplyTo) {
    let parsed = match request::complete(body) {
        Ok(parsed) => parsed,
        Err(error) => return refuse(reply, error.message()),
    };
    let Some(completion) = completion(
        parsed.id,
        parsed.partition_key,
        parsed.lease_token,
        parsed.fencing_token,
    ) else {
        return refuse(reply, "ERR flow fencing_token must not be negative");
    };
    context.service.complete(
        vec![completion],
        Box::new(move |result| single_reply(reply, result)),
    );
}
