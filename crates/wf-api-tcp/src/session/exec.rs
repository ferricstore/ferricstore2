//! `COMMAND_EXEC` frames (ADR-014 step 9c): a FLOW command sent as text, with
//! its arguments, runs exactly as its typed form would; the reply is the
//! same term. Other commands are not part of this workflow server.

use wf_application::commands::flow::text::{self, Parsed};
use wf_application::commands::flow::{Term, claim};

use crate::request;
use crate::response;

use super::connection::ReplyTo;
use super::dispatch::{Context, refuse};

/// Where a command's outcome goes: its reply, or its refusal text.
pub(super) type Done = Box<dyn FnOnce(Result<Term, String>) + Send>;

/// Runs one text command from its `{command, args}` map.
pub(super) fn run(context: &Context, payload: &Term, done: Done) {
    let Some(Term::Bin(name)) = payload.get(b"command") else {
        return done(Err("ERR native field command must be binary".to_owned()));
    };
    let args = match payload.get(b"args") {
        None | Some(Term::Nil) => &[][..],
        Some(Term::List(args)) => args.as_slice(),
        Some(_) => return done(Err("ERR native COMMAND_EXEC args must be a list".to_owned())),
    };
    if !name.to_ascii_uppercase().starts_with(b"FLOW.") {
        return done(Err(format!(
            "ERR command {} is not supported by this server",
            String::from_utf8_lossy(name)
        )));
    }
    match text::parse_with_clock(name, args, context.info.client_clock) {
        Ok(Parsed::Command(command)) => context.service.execute(
            command,
            Box::new(move |result| done(result.map_err(|error| error.to_string()))),
        ),
        Ok(Parsed::Claim(typed, map)) => {
            let mut parsed = match claim::parse(typed, &map) {
                Ok(parsed) => parsed,
                Err(error) => return done(Err(error.to_string())),
            };
            if parsed.request.block_ms.is_some() {
                parsed.request.cancellation = Some(context.closed.clone());
            }
            let shape = parsed.shape;
            context.service.claim(
                parsed.request,
                Box::new(move |result| {
                    done(
                        result
                            .map(|claimed| claim::reply(shape, claimed))
                            .map_err(|error| error.to_string()),
                    );
                }),
            );
        }
        Err(error) => done(Err(error.to_string())),
    }
}

/// Runs a `COMMAND_EXEC` frame.
pub(super) fn exec(context: &Context, body: &[u8], reply: ReplyTo) {
    let payload = match request::options("COMMAND_EXEC", body) {
        Ok(payload) => payload,
        Err(error) => return refuse(reply, error.message()),
    };
    run(
        context,
        &payload,
        Box::new(move |outcome| match outcome {
            Ok(term) => match response::term(&term) {
                Ok(body) => reply.send(0, &body),
                Err(_) => refuse(reply, "ERR reply exceeds the frame limit"),
            },
            Err(message) => refuse(reply, &message),
        }),
    );
}
