//! Member roles (ADR-020): a replica takes every committed entry and
//! answers reads, but never votes, never leads and counts toward no
//! quorum.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use wf_application::commands::flow::Term;
use wf_application::commands::queue::{MemberRole, QueueService, ServiceError};
use wf_node::composition::QueueNode;

use super::replica::{MEMBERS, open_as, peers, serving};
use super::replica_reads::{create, state};
use super::{TestResult, call, directory};

/// Waits until every node knows its leader, as a client waits for each
/// member's readiness: a replica knows it once the leader added it.
fn ready(nodes: &BTreeMap<u64, QueueNode>) -> TestResult {
    let until = Instant::now() + Duration::from_secs(20);
    while !nodes.values().all(QueueNode::ready) {
        if Instant::now() > until {
            let seen: Vec<_> = nodes
                .iter()
                .map(|(id, node)| (*id, node.route(), node.role(), node.poisoned()))
                .collect();
            return Err(format!("a member never knew its leader: {seen:?}").into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

/// One voter and two replicas: the voter alone forms and leads the group,
/// the replicas read every write it acknowledged and refuse writes, and
/// with the voter gone they elect no leader, though two of three would be
/// a majority if they voted.
#[test]
fn replicas_take_every_write_but_never_lead() -> TestResult {
    let path = directory("replica-roles")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open_as(&path, id, &peers, id == 1)?);
    }
    assert_eq!(serving(&nodes, "probe")?, 1);
    ready(&nodes)?;
    let voter = nodes.get(&1).ok_or("no voter")?;
    for round in 0..5 {
        let flow = format!("f{round}");
        create(voter, &flow)??;
        for id in [2, 3] {
            let replica = nodes.get(&id).ok_or("no replica")?;
            assert_eq!(
                state(replica, &flow)?,
                Ok(Term::text("ready")),
                "member {id}"
            );
        }
    }
    for id in [2, 3] {
        let replica = nodes.get(&id).ok_or("no replica")?;
        assert_eq!(create(replica, "refused")?, Err(ServiceError::Reroute));
    }
    drop(nodes.remove(&1));
    // Several election timeouts (at most a second each).
    std::thread::sleep(Duration::from_secs(4));
    let leaders: Vec<Option<u64>> = nodes.values().map(QueueNode::leader).collect();
    assert!(
        leaders
            .iter()
            .all(|leader| *leader != Some(2) && *leader != Some(3)),
        "a replica leads: {leaders:?}"
    );
    Ok(())
}

/// A follower keeps one connection to its leader for reads; once the
/// leader's process restarts and leads again, that connection is dead, and
/// the follower's next read reconnects instead of refusing as not led here.
#[test]
fn a_replica_reads_again_once_its_leader_restarted() -> TestResult {
    let path = directory("replica-roles-restart")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open_as(&path, id, &peers, id == 1)?);
    }
    assert_eq!(serving(&nodes, "probe")?, 1);
    ready(&nodes)?;
    create(nodes.get(&1).ok_or("no voter")?, "kept")??;
    let replica = nodes.get(&2).ok_or("no replica")?;
    assert_eq!(state(replica, "kept")?, Ok(Term::text("ready")));
    drop(nodes.remove(&1));
    nodes.insert(1, open_as(&path, 1, &peers, true)?);
    assert_eq!(serving(&nodes, "probe-again")?, 1);
    let replica = nodes.get(&2).ok_or("no replica")?;
    assert_eq!(state(replica, "kept")?, Ok(Term::text("ready")));
    Ok(())
}

fn role(nodes: &BTreeMap<u64, QueueNode>, id: u64) -> TestResult<MemberRole> {
    Ok(nodes.get(&id).ok_or("no such member")?.role())
}

/// Asks member `via` to give member `member` `role`.
fn change(
    nodes: &BTreeMap<u64, QueueNode>,
    via: u64,
    (member, role): (u64, MemberRole),
) -> TestResult<Result<(), ServiceError>> {
    let node = nodes.get(&via).ok_or("no such member")?;
    call(|reply| node.change_role(member, role, reply))
}

/// Waits until member `id` reports `role`.
fn holds(nodes: &BTreeMap<u64, QueueNode>, id: u64, role: MemberRole) -> TestResult {
    let until = Instant::now() + Duration::from_secs(20);
    while self::role(nodes, id)? != role {
        if Instant::now() > until {
            return Err(format!("member {id} never became {role:?}").into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

/// `CLUSTER.PROMOTE` (ADR-020): replicas promoted by any member, a replica
/// included, report themselves voters and then vote: with the first voter
/// gone, the two promoted ones elect a leader.
#[test]
fn promoted_replicas_vote() -> TestResult {
    let path = directory("replica-promote")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open_as(&path, id, &peers, id == 1)?);
    }
    assert_eq!(serving(&nodes, "probe")?, 1);
    ready(&nodes)?;
    let roles: Vec<MemberRole> = MEMBERS
        .iter()
        .map(|id| role(&nodes, *id))
        .collect::<TestResult<_>>()?;
    assert_eq!(
        roles,
        [MemberRole::Voter, MemberRole::Replica, MemberRole::Replica]
    );
    // Asked of a replica and of the other replica: each asks the leader.
    assert_eq!(change(&nodes, 3, (2, MemberRole::Voter))?, Ok(()));
    assert_eq!(change(&nodes, 2, (3, MemberRole::Voter))?, Ok(()));
    holds(&nodes, 2, MemberRole::Voter)?;
    holds(&nodes, 3, MemberRole::Voter)?;
    drop(nodes.remove(&1));
    let led = serving(&nodes, "probe-promoted")?;
    assert!(led == 2 || led == 3, "led by {led}");
    Ok(())
}

/// The member that serves once one other than `not` does: a demoted
/// leader hands leadership on once its demotion is committed (`OpenRaft`'s
/// step-down, 150 ms later).
fn serving_other(nodes: &BTreeMap<u64, QueueNode>, not: u64, probe: &str) -> TestResult<u64> {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let led = serving(nodes, probe)?;
        if led != not {
            return Ok(led);
        }
        if Instant::now() > until {
            return Err(format!("member {not} still leads").into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// `CLUSTER.DEMOTE` (ADR-020): a demoted leader hands leadership to
/// another voter and reports itself a replica, the last voter is never
/// demoted, changes that change nothing answer at once, and a demoted
/// member restarted with its setting still `voter` stays a replica.
#[test]
fn demoted_voters_stop_voting_and_the_last_voter_stays() -> TestResult {
    let path = directory("replica-demote")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open_as(&path, id, &peers, true)?);
    }
    let first = serving(&nodes, "probe")?;
    let asker = MEMBERS
        .into_iter()
        .find(|id| *id != first)
        .ok_or("no follower")?;
    assert_eq!(change(&nodes, asker, (first, MemberRole::Replica))?, Ok(()));
    holds(&nodes, first, MemberRole::Replica)?;
    let second = serving_other(&nodes, first, "probe-after")?;
    assert_eq!(
        change(&nodes, first, (second, MemberRole::Replica))?,
        Ok(())
    );
    holds(&nodes, second, MemberRole::Replica)?;
    let last = MEMBERS
        .into_iter()
        .find(|id| *id != first && *id != second)
        .ok_or("no voter left")?;
    assert_eq!(serving_other(&nodes, second, "probe-last")?, last);
    assert_eq!(
        change(&nodes, first, (last, MemberRole::Replica))?,
        Err(ServiceError::rejected(
            "ERR a group needs at least one voter".to_owned()
        ))
    );
    assert_eq!(change(&nodes, last, (last, MemberRole::Voter))?, Ok(()));
    assert_eq!(change(&nodes, last, (first, MemberRole::Replica))?, Ok(()));
    assert!(change(&nodes, last, (9, MemberRole::Voter))?.is_err());
    drop(nodes.remove(&first));
    nodes.insert(first, open_as(&path, first, &peers, true)?);
    ready(&nodes)?;
    assert_eq!(role(&nodes, first)?, MemberRole::Replica);
    Ok(())
}

/// Creates Flows on whichever member leads until `stop` is set, noting
/// each acknowledged one with the member that took it.
fn write_to_leaders(
    nodes: &BTreeMap<u64, QueueNode>,
    stop: &AtomicBool,
    acknowledged: &Mutex<Vec<(u64, String)>>,
) {
    let mut next = 0;
    while !stop.load(Ordering::Acquire) {
        let leaders = nodes
            .iter()
            .filter(|(id, node)| node.leader() == Some(**id));
        for (id, node) in leaders {
            let flow = format!("w{next}");
            next += 1;
            if matches!(create(node, &flow), Ok(Ok(_))) {
                acknowledged
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((*id, flow));
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Waits until at least `count` writes were acknowledged by members
/// `taken` accepts.
fn acknowledged_by(
    acknowledged: &Mutex<Vec<(u64, String)>>,
    taken: impl Fn(u64) -> bool,
    count: usize,
) -> TestResult {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let seen = acknowledged
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(id, _)| taken(*id))
            .count();
        if seen >= count {
            return Ok(());
        }
        if Instant::now() > until {
            return Err(format!("{seen} of {count} writes acknowledged").into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A leader demoted while it takes writes stops taking them at once and
/// hands leadership on only once what it proposed is committed: no member
/// loses a round (which would stop it until restarted), and every write
/// acknowledged on either side of the hand-off is kept.
#[test]
fn a_leader_demoted_under_writes_loses_no_round() -> TestResult {
    let path = directory("replica-demote-writes")?;
    let peers = peers()?;
    let mut nodes = BTreeMap::new();
    for id in MEMBERS {
        nodes.insert(id, open_as(&path, id, &peers, true)?);
    }
    let first = serving(&nodes, "probe")?;
    let asker = MEMBERS
        .into_iter()
        .find(|id| *id != first)
        .ok_or("no follower")?;
    let stop = AtomicBool::new(false);
    let acknowledged = Mutex::new(Vec::new());
    let led = std::thread::scope(|scope| -> TestResult<u64> {
        let writer = scope.spawn(|| write_to_leaders(&nodes, &stop, &acknowledged));
        let demoted = (|| -> TestResult<u64> {
            acknowledged_by(&acknowledged, |id| id == first, 3)?;
            assert_eq!(change(&nodes, asker, (first, MemberRole::Replica))?, Ok(()));
            let led = serving_other(&nodes, first, "probe-after")?;
            acknowledged_by(&acknowledged, |id| id == led, 3)?;
            Ok(led)
        })();
        stop.store(true, Ordering::Release);
        writer.join().map_err(|_| "the writer panicked")?;
        demoted
    })?;
    let poisoned: Vec<u64> = nodes
        .iter()
        .filter(|(_, node)| node.poisoned())
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(poisoned, Vec::<u64>::new());
    let leader = nodes.get(&led).ok_or("no leader")?;
    let acknowledged = acknowledged
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner);
    for (_, flow) in &acknowledged {
        assert_eq!(state(leader, flow)?, Ok(Term::text("ready")), "{flow}");
    }
    Ok(())
}
