//! Routes one request frame to the handshake or the queue service.

use std::sync::Arc;

use wf_application::commands::flow::NodeService;
use wf_application::commands::queue::Cancellation;

use crate::frame::{FLAG_COMPRESSED, FLAG_CUSTOM_PAYLOAD, FLAG_MORE_CHUNKS, Header, Opcode};
use crate::request::{self, Startup};
use crate::response::{self, COMPACT_CODECS, Handshake, STATUS_ERROR};

use super::ServerInfo;
use super::connection::ReplyTo;
use super::{exec, generic, pipeline, translate};

/// What every request of a connection shares.
pub(super) struct Context {
    pub(super) service: Arc<dyn NodeService>,
    pub(super) info: Arc<ServerInfo>,
    pub(super) client_id: u64,
    /// Set when the connection stops reading; cancels its waiting claims.
    pub(super) closed: Cancellation,
}

/// Compact reply codecs the client accepted in `STARTUP`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Negotiated {
    pub(super) claim_jobs: bool,
    pub(super) ok_list: bool,
}

impl Negotiated {
    fn from_startup(startup: &Startup) -> Self {
        let wants = |name: &str| {
            startup.compact_flow_responses
                && startup.codecs.iter().any(|codec| codec == name.as_bytes())
        };
        Self {
            claim_jobs: wants("flow_claim_jobs_v1"),
            ok_list: wants("ok_list_v1"),
        }
    }

    fn selected(self) -> Vec<&'static str> {
        COMPACT_CODECS
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| match *name {
                "flow_claim_jobs_v1" => self.claim_jobs,
                "ok_list_v1" => self.ok_list,
                _ => false,
            })
            .collect()
    }
}

/// Whether the connection continues after a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Next {
    Continue,
    Quit,
}

/// Sends an `ERR ...` error map.
pub(super) fn refuse(reply: ReplyTo, message: &str) {
    reply.send(
        0,
        &response::error(STATUS_ERROR, message).unwrap_or_default(),
    );
}

fn handshake_reply(context: &Context, negotiated: Negotiated, reply: ReplyTo) {
    let selected = negotiated.selected();
    let body = response::handshake(&Handshake {
        node: &context.info.node,
        host: &context.info.host,
        port: context.info.port,
        shard_count: context.info.shard_count,
        client_id: context.client_id,
        compact: negotiated.claim_jobs || negotiated.ok_list,
        selected: &selected,
    });
    match body {
        Ok(body) => reply.send(0, &body),
        Err(_) => refuse(reply, "ERR handshake reply exceeds the frame limit"),
    }
}

/// Handles one request; the reply may be sent later from another thread.
pub(super) fn handle(
    context: &Context,
    negotiated: &mut Negotiated,
    header: &Header,
    body: &[u8],
    reply: ReplyTo,
) -> Next {
    if header.flags & (FLAG_COMPRESSED | FLAG_MORE_CHUNKS) != 0 {
        refuse(
            reply,
            "ERR compressed or chunked frames are not supported by this server",
        );
        return Next::Continue;
    }
    match Opcode::from_code(header.opcode) {
        Some(Opcode::Hello | Opcode::Options) => handshake_reply(context, *negotiated, reply),
        Some(Opcode::Startup) => match request::startup(body) {
            Ok(startup) => {
                *negotiated = Negotiated::from_startup(&startup);
                handshake_reply(context, *negotiated, reply);
            }
            Err(error) => refuse(reply, error.message()),
        },
        Some(Opcode::Ping) => reply.send(0, &response::ok_text(b"PONG").unwrap_or_default()),
        Some(Opcode::Quit) => {
            reply.send(0, &response::ok_text(b"OK").unwrap_or_default());
            return Next::Quit;
        }
        Some(Opcode::FlowCreateMany) => {
            translate::create_many(context, *negotiated, header, body, reply);
        }
        Some(Opcode::Flow(name)) => generic::flow(context, name, header.flags, body, reply),
        Some(Opcode::FlowClaimDue) if header.flags & FLAG_CUSTOM_PAYLOAD == 0 => {
            generic::claim(context, "FLOW.CLAIM_DUE", body, reply);
        }
        Some(Opcode::FlowClaimDue) => {
            translate::claim_due(context, *negotiated, header, body, reply);
        }
        Some(Opcode::FlowReclaim) => generic::claim(context, "FLOW.RECLAIM", body, reply),
        Some(Opcode::Pipeline) => pipeline::pipeline(context, header.flags, body, reply),
        Some(Opcode::CommandExec) => exec::exec(context, body, reply),
        Some(Opcode::FlowCompleteMany) if header.flags & FLAG_CUSTOM_PAYLOAD == 0 => {
            generic::flow(context, "FLOW.COMPLETE_MANY", header.flags, body, reply);
        }
        Some(Opcode::FlowCompleteMany) => {
            translate::complete_many(context, *negotiated, header, body, reply);
        }
        None => refuse(
            reply,
            &format!(
                "ERR FSNP opcode {:#06x} is not supported by this server",
                header.opcode
            ),
        ),
    }
    Next::Continue
}
