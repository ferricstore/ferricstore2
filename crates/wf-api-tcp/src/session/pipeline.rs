//! Typed `PIPELINE` frames (ADR-014): every Flow command in the frame runs
//! as it would alone, and one reply lists each command's outcome in order as
//! Elixir's typed pipeline reply does: `{lane_id, opcode, request_id,
//! status, value}`, with `ok` and the reply, or `error` and its message.

use std::sync::{Arc, Mutex, PoisonError};

use wf_application::commands::flow::{self, Term, claim};

use crate::frame::{FLAG_CUSTOM_PAYLOAD, Opcode};
use crate::request::{self, PipelineCommand};
use crate::response;

use super::connection::ReplyTo;
use super::dispatch::{Context, refuse};

type Outcome = Result<Term, String>;

struct Pending {
    outcomes: Vec<Option<Outcome>>,
    remaining: usize,
    reply: Option<ReplyTo>,
    commands: Vec<PipelineCommand>,
    /// Answer `[status, value]` pairs instead of maps (audit gap G59).
    pairs: bool,
}

fn item(command: &PipelineCommand, outcome: Outcome, pairs: bool) -> Term {
    let (status, value) = match outcome {
        Ok(value) => ("ok", value),
        Err(message) => ("error", Term::Bin(message.into_bytes())),
    };
    if pairs {
        return Term::List(vec![Term::text(status), value]);
    }
    flow::term::map(vec![
        ("lane_id", Term::Int(command.lane_id)),
        ("opcode", Term::Int(i64::from(command.opcode))),
        ("request_id", Term::Int(command.request_id)),
        ("status", Term::text(status)),
        ("value", value),
    ])
}

fn settle(pending: &Mutex<Pending>, position: usize, outcome: Outcome) {
    let mut state = pending.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(slot) = state.outcomes.get_mut(position) {
        *slot = Some(outcome);
    }
    state.remaining = state.remaining.saturating_sub(1);
    if state.remaining > 0 {
        return;
    }
    let Some(reply) = state.reply.take() else {
        return;
    };
    let outcomes = std::mem::take(&mut state.outcomes);
    let pairs = state.pairs;
    let items: Vec<Term> = state
        .commands
        .iter()
        .zip(outcomes)
        .map(|(command, outcome)| {
            item(
                command,
                outcome.unwrap_or_else(|| Err("ERR flow store unavailable".to_owned())),
                pairs,
            )
        })
        .collect();
    drop(state);
    match response::term(&Term::List(items)) {
        Ok(body) => reply.send(0, &body),
        Err(_) => refuse(reply, "ERR reply exceeds the frame limit"),
    }
}

/// `PING`'s reply: its `message`, or `PONG`, as Elixir's
/// `do_execute(@op_ping)` (audit gap G57).
pub(super) fn pong(body: &Term) -> Term {
    body.get(b"message")
        .cloned()
        .unwrap_or_else(|| Term::text("PONG"))
}

fn run(context: &Context, command: &PipelineCommand, done: Box<dyn FnOnce(Outcome) + Send>) {
    let failed = |error: flow::FlowError| error.to_string();
    // A batch's typed-map body runs as its Flow command, as it does alone
    // (audit gap G58).
    let opcode = match Opcode::from_code(command.opcode) {
        Some(Opcode::FlowCompleteMany) => Some(Opcode::Flow("FLOW.COMPLETE_MANY")),
        Some(Opcode::FlowCreateMany) => Some(Opcode::Flow("FLOW.CREATE_MANY")),
        other => other,
    };
    match opcode {
        Some(Opcode::Ping) => done(Ok(pong(&command.body))),
        Some(Opcode::Flow(name)) => match context.admit(command.body.clone()) {
            Ok(args) => match flow::parse(name, &args) {
                Ok(parsed) => context.service.execute(
                    parsed,
                    Box::new(move |result| done(result.map_err(|error| error.to_string()))),
                ),
                Err(error) => done(Err(failed(error))),
            },
            Err(late) => done(Err(late.message().to_owned())),
        },
        Some(opcode @ (Opcode::FlowClaimDue | Opcode::FlowReclaim)) => {
            let name = if opcode == Opcode::FlowReclaim {
                "FLOW.RECLAIM"
            } else {
                "FLOW.CLAIM_DUE"
            };
            let args = match context.admit(command.body.clone()) {
                Ok(args) => args,
                Err(late) => return done(Err(late.message().to_owned())),
            };
            match claim::parse(name, &args) {
                Ok(parsed) => {
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
                Err(error) => done(Err(failed(error))),
            }
        }
        Some(Opcode::CommandExec) => super::exec::run(context, &command.body, done),
        _ => done(Err(format!(
            "ERR FSNP opcode {:#06x} is not supported in a pipeline by this server",
            command.opcode
        ))),
    }
}

/// Runs a typed `PIPELINE` frame.
pub(super) fn pipeline(context: &Context, flags: u8, body: &[u8], reply: ReplyTo) {
    if flags & FLAG_CUSTOM_PAYLOAD != 0 {
        return refuse(
            reply,
            "ERR compact PIPELINE requests are not supported by this server",
        );
    }
    let commands = match request::pipeline(body) {
        Ok(commands) => commands,
        Err(error) => return refuse(reply, error.message()),
    };
    if commands.is_empty() {
        return match response::term(&Term::List(Vec::new())) {
            Ok(body) => reply.send(0, &body),
            Err(_) => refuse(reply, "ERR reply exceeds the frame limit"),
        };
    }
    let pending = Arc::new(Mutex::new(Pending {
        outcomes: commands.iter().map(|_| None).collect(),
        remaining: commands.len(),
        reply: Some(reply),
        commands: commands.clone(),
        pairs: request::pipeline_pairs(body),
    }));
    for (position, command) in commands.iter().enumerate() {
        let part = Arc::clone(&pending);
        run(
            context,
            command,
            Box::new(move |outcome| settle(&part, position, outcome)),
        );
    }
}
