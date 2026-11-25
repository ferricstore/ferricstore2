//! `COMMAND_EXEC` frames (ADR-014 step 9c): a FLOW command sent as text, with
//! its arguments, runs exactly as its typed form would; the reply is the
//! same term. `PING` answers as its own opcode does; other commands are not
//! part of this workflow server.

use wf_application::commands::flow::text::{self, Parsed};
use wf_application::commands::flow::{Term, claim};
use wf_application::commands::queue::ServiceError;

use crate::request;

use super::connection::ReplyTo;
use super::deadline;
use super::dispatch::{Context, refuse_request};
use super::generic;

/// Where a command's outcome goes: its reply, or its refusal, which keeps
/// its status and, for a query failure, its diagnostic (audit gap G65).
pub(super) type Done = Box<dyn FnOnce(Result<Term, ServiceError>) + Send>;

fn rejected(text: impl Into<String>) -> ServiceError {
    ServiceError::rejected(text.into())
}

/// `FERRICSTORE.CAPABILITIES`: the management map the SDKs'
/// `capabilities()` reads, naming what this node serves (audit gap G73).
fn capabilities(context: &Context) -> Term {
    wf_application::commands::flow::term::map(vec![
        ("acl_management", Term::Bool(false)),
        ("flow_observability", Term::Bool(true)),
        ("health", Term::Bool(context.info.health)),
        ("namespace_management", Term::Bool(false)),
        ("quota_management", Term::Bool(false)),
        ("sdk", Term::Bool(true)),
        ("telemetry", Term::Bool(false)),
    ])
}

/// Runs one text command from its `{command, args}` map.
pub(super) fn run(context: &Context, payload: &Term, done: Done) {
    if let Err(late) = deadline::check(payload) {
        return done(Err(late.error()));
    }
    let Some(Term::Bin(name)) = payload.get(b"command") else {
        return done(Err(rejected("ERR native field command must be binary")));
    };
    let args = match payload.get(b"args") {
        None | Some(Term::Nil) => &[][..],
        Some(Term::List(args)) => args.as_slice(),
        Some(_) => return done(Err(rejected("ERR native COMMAND_EXEC args must be a list"))),
    };
    let upper = name.to_ascii_uppercase();
    // PING answers here as its own opcode does; SDKs send it as text to
    // check a connection (audit gap G61).
    if upper == b"PING" {
        return done(match args {
            [] => Ok(Term::text("PONG")),
            [message] => Ok(message.clone()),
            _ => Err(rejected("ERR wrong number of arguments for 'ping' command")),
        });
    }
    if upper == b"FERRICSTORE.CAPABILITIES" {
        return done(if args.is_empty() {
            Ok(capabilities(context))
        } else {
            Err(rejected(
                "ERR wrong number of arguments for 'ferricstore.capabilities' command",
            ))
        });
    }
    if !upper.starts_with(b"FLOW.") {
        return done(Err(rejected(format!(
            "ERR command {} is not supported by this server",
            String::from_utf8_lossy(name)
        ))));
    }
    match text::parse_with_clock(name, args, context.info.client_clock) {
        Ok(Parsed::Command(command)) => context.service.execute(command, done),
        Ok(Parsed::Claim(typed, map)) => {
            let mut parsed = match claim::parse(typed, &map) {
                Ok(parsed) => parsed,
                Err(error) => return done(Err(error)),
            };
            if parsed.request.block_ms.is_some() {
                parsed.request.cancellation = Some(context.closed.clone());
            }
            let shape = parsed.shape;
            context.service.claim(
                parsed.request,
                Box::new(move |result| done(result.map(|claimed| claim::reply(shape, claimed)))),
            );
        }
        Err(error) => done(Err(error)),
    }
}

/// Runs a `COMMAND_EXEC` frame.
pub(super) fn exec(context: &Context, body: &[u8], reply: ReplyTo) {
    let payload = match request::options("COMMAND_EXEC", body) {
        Ok(payload) => payload,
        Err(error) => return refuse_request(reply, &error),
    };
    if let Err(late) = deadline::check(&payload) {
        return late.refuse(reply);
    }
    run(
        context,
        &payload,
        Box::new(move |outcome| match outcome {
            Ok(term) => generic::send(reply, &term),
            Err(error) => generic::failed(reply, &error),
        }),
    );
}
