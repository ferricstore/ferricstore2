//! Flow commands beyond the queue fast path (ADR-014): the typed-map body
//! becomes the command's options, the application reads them, and the reply
//! is the term the node answers with.

use wf_application::commands::flow;

use crate::request;
use crate::response;

use super::connection::ReplyTo;
use super::dispatch::{Context, refuse};

/// Runs command `name` with a typed-map `body`.
pub(super) fn flow(context: &Context, name: &str, body: &[u8], reply: ReplyTo) {
    let args = match request::options(name, body) {
        Ok(args) => args,
        Err(error) => return refuse(reply, error.message()),
    };
    let command = match flow::parse(name, &args) {
        Ok(command) => command,
        Err(error) => return refuse(reply, &error.to_string()),
    };
    context.service.execute(
        command,
        Box::new(move |result| match result {
            Ok(term) => match response::term(&term) {
                Ok(body) => reply.send(0, &body),
                Err(_) => refuse(reply, "ERR reply exceeds the frame limit"),
            },
            Err(error) => refuse(reply, &error.to_string()),
        }),
    );
}
