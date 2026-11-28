//! `CLUSTER.ROLE`, `CLUSTER.PROMOTE` and `CLUSTER.DEMOTE` (ADR-020): the
//! SDKs send them as opcodes `0x030B`, `0x0309` and `0x030A` with
//! `{"args": [...]}`, and `COMMAND_EXEC` takes them as text. A node is
//! named as the slot table names it; any other name is refused before the
//! node is asked.

use std::sync::{Arc, Mutex, PoisonError};

use wf_application::commands::flow::{FlowCommand, FlowService, Term};
use wf_application::commands::queue::{
    ClaimFlows, ClaimedFlow, CompleteFlow, CreateFlows, ItemOutcome, MemberRole, QueueService,
    Reply, ServiceError,
};

use super::typed::{Typed, bin, encode, map, reply};
use super::{Client, TestResult, serving_service, start};

const COMMAND_EXEC: u16 = 0x0100;
const PROMOTE: u16 = 0x0309;
const DEMOTE: u16 = 0x030A;
const ROLE: u16 = 0x030B;

/// A group member's service that records the role changes it is asked
/// for, and refuses them with `refusal` when set.
struct Roles {
    role: MemberRole,
    asked: Mutex<Vec<(u64, MemberRole)>>,
    refusal: Option<&'static str>,
}

impl Roles {
    fn new(role: MemberRole, refusal: Option<&'static str>) -> Arc<Self> {
        Arc::new(Self {
            role,
            asked: Mutex::new(Vec::new()),
            refusal,
        })
    }

    fn asked(&self) -> Vec<(u64, MemberRole)> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl QueueService for Roles {
    fn create(&self, _request: CreateFlows, reply: Reply<Vec<ItemOutcome>>) {
        reply(Err(ServiceError::Reroute));
    }

    fn claim(&self, _request: ClaimFlows, reply: Reply<Vec<ClaimedFlow>>) {
        reply(Err(ServiceError::Reroute));
    }

    fn complete(&self, _completions: Vec<CompleteFlow>, reply: Reply<Vec<ItemOutcome>>) {
        reply(Err(ServiceError::Reroute));
    }

    fn role(&self) -> MemberRole {
        self.role
    }

    fn change_role(&self, member: u64, role: MemberRole, reply: Reply<()>) {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((member, role));
        reply(
            self.refusal
                .map_or(Ok(()), |text| Err(ServiceError::rejected(text.to_owned()))),
        );
    }
}

impl FlowService for Roles {
    fn execute(&self, _command: FlowCommand, reply: Reply<Term>) {
        reply(Err(ServiceError::Reroute));
    }
}

fn args(values: &[&str]) -> TestResult<Vec<u8>> {
    let values = values.iter().map(|value| bin(value)).collect();
    encode(&map(&[("args", Typed::List(values))]))
}

fn text(name: &str, values: &[&str]) -> TestResult<Vec<u8>> {
    let values = values.iter().map(|value| bin(value)).collect();
    encode(&map(&[
        ("command", bin(name)),
        ("args", Typed::List(values)),
    ]))
}

/// The refusal's status and message.
fn refusal(body: &[u8]) -> TestResult<(u16, Typed)> {
    let (status, refused) = reply(body)?;
    Ok((status, super::typed::at(&refused, &["message"])?.clone()))
}

/// Elixir: `cluster_commands_test.exs` "reports the configured raft role
/// even with no remote cluster nodes": a node without a group is a voter.
/// A member answers the role it holds.
#[test]
fn cluster_role_answers_this_member_s_role() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    assert_eq!(
        reply(&client.call(ROLE, 1, 0, &args(&[])?)?.body)?,
        (0, bin("voter"))
    );
    for name in ["CLUSTER.ROLE", "cluster.role"] {
        let answer = client.call(COMMAND_EXEC, 1, 0, &text(name, &[])?)?;
        assert_eq!(reply(&answer.body)?, (0, bin("voter")), "{name}");
    }
    let extra = client.call(ROLE, 1, 0, &args(&["x"])?)?;
    assert_eq!(
        refusal(&extra.body)?,
        (
            1,
            bin("ERR wrong number of arguments for 'cluster.role' command")
        )
    );
    let running = serving_service(Roles::new(MemberRole::Replica, None))?;
    let mut client = Client::connect(&running)?;
    assert_eq!(
        reply(&client.call(ROLE, 1, 0, &args(&[])?)?.body)?,
        (0, bin("replica"))
    );
    Ok(())
}

#[test]
fn promote_and_demote_name_a_member_of_the_group() -> TestResult {
    let roles = Roles::new(MemberRole::Voter, None);
    let running = serving_service(Arc::clone(&roles) as _)?;
    let mut client = Client::connect(&running)?;
    let ok = (0, bin("OK"));
    assert_eq!(
        reply(
            &client
                .call(PROMOTE, 1, 0, &args(&["ferricstore2-2"])?)?
                .body
        )?,
        ok
    );
    assert_eq!(
        reply(&client.call(DEMOTE, 1, 0, &args(&["ferricstore2-1"])?)?.body)?,
        ok
    );
    let exec = text("cluster.promote", &["ferricstore2-1"])?;
    assert_eq!(reply(&client.call(COMMAND_EXEC, 1, 0, &exec)?.body)?, ok);
    assert_eq!(
        roles.asked(),
        [
            (2, MemberRole::Voter),
            (1, MemberRole::Replica),
            (1, MemberRole::Voter)
        ]
    );
    Ok(())
}

/// Elixir: `cluster_membership_validation_test.exs` "CLUSTER.PROMOTE and
/// CLUSTER.DEMOTE refuse a name that is not a node": refused before the
/// node is asked, as are a missing name and a node without a group; the
/// node's own refusal is the reply.
#[test]
fn promote_and_demote_refuse_what_names_no_member() -> TestResult {
    let roles = Roles::new(MemberRole::Voter, None);
    let running = serving_service(Arc::clone(&roles) as _)?;
    let mut client = Client::connect(&running)?;
    for (opcode, name) in [(PROMOTE, "cluster.promote"), (DEMOTE, "cluster.demote")] {
        let unknown = client.call(opcode, 1, 0, &args(&["k"])?)?;
        assert_eq!(
            refusal(&unknown.body)?,
            (
                1,
                bin("ERR unknown node k; expected one of ferricstore2-1, ferricstore2-2")
            )
        );
        let missing = client.call(opcode, 1, 0, &args(&[])?)?;
        assert_eq!(
            refusal(&missing.body)?,
            (
                1,
                bin(&format!(
                    "ERR wrong number of arguments for '{name}' command"
                ))
            )
        );
    }
    assert_eq!(roles.asked(), []);
    let single = start()?;
    let mut alone = Client::connect(&single)?;
    let refused = alone.call(PROMOTE, 1, 0, &args(&["ferricstore2-1"])?)?;
    assert_eq!(
        refusal(&refused.body)?,
        (1, bin("ERR this node is not in a group"))
    );
    let last = "ERR a group needs at least one voter";
    let running = serving_service(Roles::new(MemberRole::Voter, Some(last)))?;
    let mut client = Client::connect(&running)?;
    let refused = client.call(DEMOTE, 1, 0, &args(&["ferricstore2-2"])?)?;
    assert_eq!(refusal(&refused.body)?, (1, bin(last)));
    Ok(())
}
