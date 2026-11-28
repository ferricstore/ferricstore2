//! The member role commands (ADR-020): `CLUSTER.ROLE` answers the role this
//! member holds, and `CLUSTER.PROMOTE <node>` and `CLUSTER.DEMOTE <node>`
//! make a replica a voter and a voter a replica. `<node>` is a member's
//! name as the slot table names it; any other name is refused before the
//! node is asked. They come as their own opcodes, with an `args` list, or
//! as text through `COMMAND_EXEC`.

use wf_application::commands::flow::Term;
use wf_application::commands::queue::{MemberRole, ServiceError};

use crate::request;

use super::connection::ReplyTo;
use super::deadline;
use super::dispatch::{Context, refuse_request};
use super::exec::Done;
use super::generic;

fn rejected(text: String) -> ServiceError {
    ServiceError::rejected(text)
}

/// Whether `upper`, a command name in upper case, is a member role
/// command.
pub(super) fn is_cluster(upper: &[u8]) -> bool {
    matches!(
        upper,
        b"CLUSTER.ROLE" | b"CLUSTER.PROMOTE" | b"CLUSTER.DEMOTE"
    )
}

/// The member that `name` names, as the slot table names members.
fn member(context: &Context, name: &[u8]) -> Result<u64, ServiceError> {
    let members = &context.info.members;
    if members.is_empty() {
        return Err(rejected("ERR this node is not in a group".to_owned()));
    }
    members
        .iter()
        .find(|(_, endpoint)| endpoint.node.as_bytes() == name)
        .map(|(id, _)| *id)
        .ok_or_else(|| {
            let names: Vec<&str> = members
                .values()
                .map(|endpoint| endpoint.node.as_str())
                .collect();
            rejected(format!(
                "ERR unknown node {}; expected one of {}",
                String::from_utf8_lossy(name),
                names.join(", ")
            ))
        })
}

/// Runs member role command `upper` (in upper case) with `args`.
pub(super) fn run(context: &Context, upper: &[u8], args: &[Term], done: Done) {
    let role = match upper {
        b"CLUSTER.PROMOTE" => Some(MemberRole::Voter),
        b"CLUSTER.DEMOTE" => Some(MemberRole::Replica),
        _ => None,
    };
    match (role, args) {
        (None, []) => done(Ok(Term::text(context.service.role().name()))),
        (Some(role), [Term::Bin(name)]) => match member(context, name) {
            Ok(member) => context.service.change_role(
                member,
                role,
                Box::new(move |outcome| done(outcome.map(|()| Term::text("OK")))),
            ),
            Err(error) => done(Err(error)),
        },
        _ => done(Err(rejected(format!(
            "ERR wrong number of arguments for '{}' command",
            String::from_utf8_lossy(upper).to_ascii_lowercase()
        )))),
    }
}

/// Runs a member role command's own frame: its `args` list body.
pub(super) fn frame(context: &Context, name: &str, body: &[u8], reply: ReplyTo) {
    let payload = match request::options(name, body) {
        Ok(payload) => payload,
        Err(error) => return refuse_request(reply, &error),
    };
    if let Err(late) = deadline::check(&payload) {
        return late.refuse(reply);
    }
    let args = match payload.get(b"args") {
        None | Some(Term::Nil) => &[][..],
        Some(Term::List(args)) => args.as_slice(),
        Some(_) => {
            return generic::failed(
                reply,
                &rejected(format!("ERR native {name} args must be a list")),
            );
        }
    };
    run(
        context,
        name.as_bytes(),
        args,
        Box::new(move |outcome| match outcome {
            Ok(term) => generic::send(reply, &term),
            Err(error) => generic::failed(reply, &error),
        }),
    );
}
