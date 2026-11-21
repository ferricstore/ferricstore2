//! Flow commands beyond the queue fast path (ADR-014): the typed-map body
//! becomes the command's options, the application reads them, and the reply
//! is the term the node answers with.

use wf_application::commands::flow::{self, claim};
use wf_application::commands::queue::ServiceError;

use crate::request;
use crate::response;

use super::connection::ReplyTo;
use super::dispatch::{Context, refuse};

/// Runs a typed-map `FLOW.CLAIM_DUE` or `FLOW.RECLAIM`: claims of every
/// shape go through the multi-shard claim, and the reply is built from the
/// claimed records.
pub(super) fn claim(context: &Context, name: &str, body: &[u8], reply: ReplyTo) {
    let args = match request::options(name, body) {
        Ok(args) => context.clock(args),
        Err(error) => return refuse(reply, error.message()),
    };
    let mut parsed = match claim::parse(name, &args) {
        Ok(parsed) => parsed,
        Err(error) => return refuse(reply, &error.to_string()),
    };
    if parsed.request.block_ms.is_some() {
        parsed.request.cancellation = Some(context.closed.clone());
    }
    let shape = parsed.shape;
    context.service.claim(
        parsed.request,
        Box::new(move |result| match result {
            Ok(claimed) => send(reply, &claim::reply(shape, claimed)),
            // A governed claim's refusal is a map (ADR-014 step 15).
            Err(error @ ServiceError::Governance(_)) => failed(reply, &error),
            Err(error) => refuse(reply, &error.to_string()),
        }),
    );
}

/// Sends a service error with its status and map.
fn failed(reply: ReplyTo, error: &ServiceError) {
    match response::service_error(error) {
        Ok(body) => reply.send(0, &body),
        Err(_) => refuse(reply, "ERR reply exceeds the frame limit"),
    }
}

fn send(reply: ReplyTo, term: &flow::Term) {
    match response::term(term) {
        Ok(body) => reply.send(0, &body),
        Err(_) => refuse(reply, "ERR reply exceeds the frame limit"),
    }
}

/// Runs command `name` with a typed-map body, or a compact batch body when
/// `flags` say so.
pub(super) fn flow(context: &Context, name: &str, flags: u8, body: &[u8], reply: ReplyTo) {
    let decoded = if request::is_compact(flags) {
        request::many_options(name, body)
    } else {
        request::options(name, body)
    };
    let args = match decoded {
        Ok(args) => context.clock(args),
        Err(error) => return refuse(reply, error.message()),
    };
    let command = match flow::parse(name, &args) {
        Ok(command) => command,
        Err(error) => return failed(reply, &error),
    };
    context.service.execute(
        command,
        Box::new(move |result| match result {
            Ok(term) => send(reply, &term),
            Err(error) => failed(reply, &error),
        }),
    );
}
