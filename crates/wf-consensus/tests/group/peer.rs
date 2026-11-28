//! Three members over loopback TCP (`group::serve` and `group::Peers`): they
//! elect a leader, replicate its writes, and after the leader stops the other
//! two elect a new one and keep committing. A member that lost its log
//! (`group::serve_member`) gives no vote until it caught up (ADR-019).

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use openraft::error::{ClientWriteError, RaftError};
use openraft::impls::Vote;
use openraft::impls::leader_id_adv::LeaderId;
use openraft::network::{RPCOption, RaftNetworkFactory, RaftNetworkV2};
use openraft::raft::VoteRequest;
use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::vote::RaftLeaderId;
use openraft::{LogId, Raft};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use wf_consensus::group::{Config, LogStore, Peers, Proposal, serve, serve_member};
use wf_ports::consensus::log::ConsensusLog;
use wf_storage::log::{EntryLog, GroupLogLimits};
use wf_test_support::resources::TestResources;

use super::machine::Machine;
use super::{TestResult, runtime};

struct Member {
    raft: Raft<Config, Machine>,
    machine: Machine,
    server: JoinHandle<()>,
}

/// How a member serves: as a voter, or settling first as `serve_member`
/// does, with its log kept under `log`.
enum Serving<'a> {
    Voter,
    Joining { log: &'a str, machine: Machine },
}

async fn start(
    root: &TestResources,
    id: u64,
    listener: TcpListener,
    addresses: &BTreeMap<u64, SocketAddr>,
) -> TestResult<Member> {
    start_as(root, id, listener, addresses, Serving::Voter).await
}

fn names(addresses: &BTreeMap<u64, SocketAddr>) -> BTreeMap<u64, String> {
    addresses
        .iter()
        .map(|(id, address)| (*id, address.to_string()))
        .collect()
}

async fn start_as(
    root: &TestResources,
    id: u64,
    listener: TcpListener,
    addresses: &BTreeMap<u64, SocketAddr>,
    serving: Serving<'_>,
) -> TestResult<Member> {
    let config = openraft::Config {
        heartbeat_interval: 50,
        election_timeout_min: 300,
        election_timeout_max: 600,
        // As a group member's: a member that lost its log is caught up.
        allow_log_reversion: Some(true),
        ..openraft::Config::default()
    };
    let (directory, machine) = match &serving {
        Serving::Voter => (format!("m{id}"), Machine::default()),
        Serving::Joining { log, machine } => ((*log).to_owned(), machine.clone()),
    };
    let log: Arc<dyn ConsensusLog> = Arc::new(EntryLog::open(
        &root.path().join(directory),
        GroupLogLimits::default(),
    )?);
    let raft = Raft::new(
        id,
        Arc::new(config.validate()?),
        Peers::new(names(addresses)),
        LogStore::new(Arc::clone(&log)),
        machine.clone(),
    )
    .await?;
    let server = match serving {
        Serving::Voter => tokio::spawn(serve(
            listener,
            raft.clone(),
            (names(addresses), id),
            machine.store(),
        )),
        Serving::Joining { .. } => tokio::spawn(serve_member(
            listener,
            raft.clone(),
            (names(addresses), id, true),
            (log, machine.store(), wf_consensus::group::MAX_FRAME_BYTES),
        )),
    };
    Ok(Member {
        raft,
        machine,
        server,
    })
}

async fn leader(members: &BTreeMap<u64, Member>) -> TestResult<u64> {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        for (id, member) in members {
            if member.raft.current_leader().await == Some(*id) {
                return Ok(*id);
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err("no leader".into())
}

/// Writes through the leader, retrying a definite refusal.
async fn write(members: &BTreeMap<u64, Member>, value: u32) -> TestResult<u64> {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        let id = leader(members).await?;
        let member = members.get(&id).ok_or("leader missing")?;
        match member
            .raft
            .client_write(Proposal(value.to_be_bytes().to_vec()))
            .await
        {
            Ok(written) => return Ok(written.log_id.index),
            Err(RaftError::APIError(ClientWriteError::ForwardToLeader(_))) => {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err("write not committed".into())
}

async fn converge(members: &BTreeMap<u64, Member>, index: u64) -> TestResult {
    let until = Instant::now() + Duration::from_secs(20);
    while Instant::now() < until {
        let applied = |member: &Member| {
            member
                .machine
                .image()
                .applied
                .is_some_and(|applied| applied.index >= index)
        };
        if members.values().all(applied) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("members did not apply {index}").into())
}

#[test]
fn members_over_tcp_replicate_and_survive_the_leader_s_loss() -> TestResult {
    runtime()?.block_on(async {
        let root = TestResources::new()?;
        let mut listeners = BTreeMap::new();
        for id in [1_u64, 2, 3] {
            listeners.insert(id, TcpListener::bind("127.0.0.1:0").await?);
        }
        let addresses: BTreeMap<u64, SocketAddr> = listeners
            .iter()
            .map(|(id, listener)| Ok((*id, listener.local_addr()?)))
            .collect::<TestResult<_>>()?;
        let mut members = BTreeMap::new();
        for (id, listener) in listeners {
            members.insert(id, start(&root, id, listener, &addresses).await?);
        }
        let first = members.get(&1).ok_or("member 1 missing")?;
        first.raft.initialize(BTreeSet::from([1, 2, 3])).await?;
        let mut last = 0;
        for value in 0..30 {
            last = write(&members, value).await?;
        }
        converge(&members, last).await?;
        let old = leader(&members).await?;
        let stopped = members.remove(&old).ok_or("leader missing")?;
        stopped.raft.shutdown().await?;
        stopped.server.abort();
        for value in 30..40 {
            last = write(&members, value).await?;
        }
        converge(&members, last).await?;
        let applied: Vec<Vec<(u64, Vec<u8>)>> = members
            .values()
            .map(|member| member.machine.image().proposals)
            .collect();
        assert_eq!(applied.first().map(Vec::len), Some(40));
        assert!(applied.windows(2).all(|pair| pair.first() == pair.get(1)));
        Ok(())
    })
}

/// Whether member `id` answers a pre-vote; a member that gives no vote
/// answers none.
async fn answers_votes(addresses: &BTreeMap<u64, SocketAddr>, id: u64) -> TestResult<bool> {
    let mut link = Peers::new(names(addresses)).new_client(id, &()).await;
    let candidate = VoteRequest::new(
        Vote::new(1_000, 1),
        Some(LogId::new(LeaderId::new(1_000, 1), 1_000)),
    );
    Ok(link
        .pre_vote(candidate, RPCOption::new(Duration::from_secs(1)))
        .await
        .is_ok())
}

/// A new group of three members that settle as `serve_member` does, with
/// twenty writes applied everywhere; returns the last write's index.
async fn joined_group(
    root: &TestResources,
) -> TestResult<(BTreeMap<u64, Member>, BTreeMap<u64, SocketAddr>, u64)> {
    let mut listeners = BTreeMap::new();
    for id in [1_u64, 2, 3] {
        listeners.insert(id, TcpListener::bind("127.0.0.1:0").await?);
    }
    let addresses: BTreeMap<u64, SocketAddr> = listeners
        .iter()
        .map(|(id, listener)| Ok((*id, listener.local_addr()?)))
        .collect::<TestResult<_>>()?;
    // Every member settles, and member 1 forms the group.
    let mut members = BTreeMap::new();
    for (id, listener) in listeners {
        let log = format!("m{id}");
        let joining = Serving::Joining {
            log: &log,
            machine: Machine::default(),
        };
        members.insert(id, start_as(root, id, listener, &addresses, joining).await?);
    }
    let mut last = 0;
    for value in 0..20 {
        last = write(&members, value).await?;
    }
    converge(&members, last).await?;
    Ok((members, addresses, last))
}

async fn stop(members: &mut BTreeMap<u64, Member>, id: u64) -> TestResult {
    let stopped = members.remove(&id).ok_or("member missing")?;
    stopped.raft.shutdown().await?;
    stopped.server.abort();
    // Its listener closes once the aborted server is dropped.
    let _aborted = stopped.server.await;
    Ok(())
}

/// Starts member `id` again over the log under `log`.
async fn restart(
    root: &TestResources,
    members: &mut BTreeMap<u64, Member>,
    addresses: &BTreeMap<u64, SocketAddr>,
    (id, log): (u64, &str),
    machine: Machine,
) -> TestResult {
    let address = addresses.get(&id).ok_or("no address")?;
    let listener = TcpListener::bind(address).await?;
    let joining = Serving::Joining { log, machine };
    members.insert(id, start_as(root, id, listener, addresses, joining).await?);
    Ok(())
}

async fn answers_votes_within(
    addresses: &BTreeMap<u64, SocketAddr>,
    id: u64,
    wait: Duration,
) -> TestResult<bool> {
    let until = Instant::now() + wait;
    while Instant::now() < until {
        if answers_votes(addresses, id).await? {
            return Ok(true);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(false)
}

#[test]
fn a_member_that_lost_its_log_answers_votes_only_once_it_caught_up() -> TestResult {
    runtime()?.block_on(async {
        let root = TestResources::new()?;
        let (mut members, addresses, last) = joined_group(&root).await?;
        let wiped = if leader(&members).await? == 3 { 2 } else { 3 };
        stop(&mut members, wiped).await?;
        // It comes back without its log, and cannot apply yet.
        let machine = Machine::default();
        machine.pause();
        restart(
            &root,
            &mut members,
            &addresses,
            (wiped, "lost"),
            machine.clone(),
        )
        .await?;
        // It takes the leader's entries but gives no vote while it has not
        // applied them.
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            assert!(
                !answers_votes(&addresses, wiped).await?,
                "voted before catching up"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let member = members.get(&wiped).ok_or("member missing")?;
        assert!(member.raft.metrics().borrow_watched().last_log_index >= Some(last));
        machine.resume();
        converge(&members, last).await?;
        assert!(
            answers_votes_within(&addresses, wiped, Duration::from_secs(5)).await?,
            "no vote after catching up"
        );
        Ok(())
    })
}

/// The members a member that lost its log asks need not answer together:
/// here the other follower is down while the leader catches it up, and the
/// leader is down once that follower is back.
#[test]
fn a_member_that_lost_its_log_settles_from_answers_given_at_different_times() -> TestResult {
    runtime()?.block_on(async {
        let root = TestResources::new()?;
        let (mut members, addresses, last) = joined_group(&root).await?;
        let first = leader(&members).await?;
        let mut followers = [1_u64, 2, 3].into_iter().filter(|id| *id != first);
        let wiped = followers.next().ok_or("no follower")?;
        let other = followers.next().ok_or("no second follower")?;
        stop(&mut members, other).await?;
        stop(&mut members, wiped).await?;
        restart(
            &root,
            &mut members,
            &addresses,
            (wiped, "lost"),
            Machine::default(),
        )
        .await?;
        converge(&members, last).await?;
        stop(&mut members, first).await?;
        restart(
            &root,
            &mut members,
            &addresses,
            (other, &format!("m{other}")),
            Machine::default(),
        )
        .await?;
        assert!(
            answers_votes_within(&addresses, wiped, Duration::from_secs(5)).await?,
            "no vote once every member had answered"
        );
        Ok(())
    })
}

/// Binds a loopback listener for each of `ids`.
async fn listeners(ids: &[u64]) -> TestResult<BTreeMap<u64, TcpListener>> {
    let mut listeners = BTreeMap::new();
    for id in ids {
        listeners.insert(*id, TcpListener::bind("127.0.0.1:0").await?);
    }
    Ok(listeners)
}

fn addresses_of(listeners: &BTreeMap<u64, TcpListener>) -> TestResult<BTreeMap<u64, SocketAddr>> {
    listeners
        .iter()
        .map(|(id, listener)| Ok((*id, listener.local_addr()?)))
        .collect()
}

/// A group whose member list names another group's member (an address
/// reused, or a list mistyped): that member takes none of its traffic, so
/// it neither follows the other group's leader nor applies its entries.
#[test]
fn a_member_refuses_the_traffic_of_another_group() -> TestResult {
    runtime()?.block_on(async {
        let (x_root, y_root) = (TestResources::new()?, TestResources::new()?);
        let mut x = listeners(&[1, 2, 3]).await?;
        let x_addresses = addresses_of(&x)?;
        let y = listeners(&[1, 2]).await?;
        let mut y_addresses = addresses_of(&y)?;
        let shared = *x_addresses.get(&3).ok_or("no address")?;
        y_addresses.insert(3, shared);
        // Of group X only member 3 runs: it has no log and no leader.
        let listener = x.remove(&3).ok_or("no listener")?;
        let stranger = start(&x_root, 3, listener, &x_addresses).await?;
        let mut members = BTreeMap::new();
        for (id, listener) in y {
            members.insert(id, start(&y_root, id, listener, &y_addresses).await?);
        }
        let first = members.get(&1).ok_or("member 1 missing")?;
        first.raft.initialize(BTreeSet::from([1, 2, 3])).await?;
        let mut last = 0;
        for value in 0..10 {
            last = write(&members, value).await?;
        }
        converge(&members, last).await?;
        // Many heartbeats later, group X's member still knows nothing of Y.
        tokio::time::sleep(Duration::from_millis(500)).await;
        let metrics = stranger.raft.metrics().borrow_watched().clone();
        assert_eq!(metrics.current_leader, None, "followed another group");
        assert_eq!(metrics.last_log_index, None, "took another group's log");
        assert_eq!(stranger.machine.image().proposals, Vec::new());
        Ok(())
    })
}

/// A process serving at member 2's address as member 3 (a node started
/// with another's id) answers none of the requests meant for member 2.
#[test]
fn a_member_refuses_requests_meant_for_another_member() -> TestResult {
    runtime()?.block_on(async {
        let root = TestResources::new()?;
        let mut bound = listeners(&[1, 2, 3]).await?;
        let addresses = addresses_of(&bound)?;
        let listener = bound.remove(&2).ok_or("no listener")?;
        let _impostor = start(&root, 3, listener, &addresses).await?;
        assert!(
            !answers_votes(&addresses, 2).await?,
            "member 3 answered for 2"
        );
        // The refusal names both members, for whoever reads the error.
        let mut link = Peers::new(names(&addresses)).new_client(2, &()).await;
        let candidate = VoteRequest::new(Vote::new(1, 1), None);
        let refusal = link
            .vote(candidate, RPCOption::new(Duration::from_secs(1)))
            .await
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(
            refusal.contains("a request for member 2 of group ")
                && refusal.contains(" reached member 3 of group "),
            "{refusal}"
        );
        Ok(())
    })
}

/// A member that settled into a new group and stopped before any entry
/// reached it kept its storage: it holds the mark that it votes and no
/// log. With the leader down, it votes for the other member, which leads
/// and catches it up.
#[test]
fn a_member_stopped_before_its_first_entry_votes_when_it_starts_again() -> TestResult {
    runtime()?.block_on(async {
        let root = TestResources::new()?;
        let (mut members, addresses, _) = joined_group(&root).await?;
        let first = leader(&members).await?;
        let early = if first == 3 { 2 } else { 3 };
        stop(&mut members, early).await?;
        stop(&mut members, first).await?;
        EntryLog::open(&root.path().join("early"), GroupLogLimits::default())?.save_voter()?;
        restart(
            &root,
            &mut members,
            &addresses,
            (early, "early"),
            Machine::default(),
        )
        .await?;
        let next = write(&members, 99).await?;
        converge(&members, next).await?;
        Ok(())
    })
}

/// Waits until member `id` holds the log through `index`.
async fn holds(members: &BTreeMap<u64, Member>, id: u64, index: u64) -> TestResult {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        let member = members.get(&id).ok_or("member missing")?;
        if member.raft.metrics().borrow_watched().last_log_index >= Some(index) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(format!("member {id} did not take the log through {index}").into())
}

/// Restarts member `id` over the log under `log` with a machine that
/// cannot apply: it takes entries, but no commit point it waits for.
async fn restart_paused(
    root: &TestResources,
    members: &mut BTreeMap<u64, Member>,
    addresses: &BTreeMap<u64, SocketAddr>,
    (id, log): (u64, &str),
) -> TestResult<Machine> {
    stop(members, id).await?;
    let machine = Machine::default();
    machine.pause();
    restart(root, members, addresses, (id, log), machine.clone()).await?;
    Ok(machine)
}

/// A member that lost its log and stopped while catching up holds part of
/// the group's log, but not the mark that it votes: it asks again when it
/// starts and gives no vote until it caught up. Then the mark is saved, and
/// it votes as soon as it starts again, before it applies anything.
#[test]
fn a_member_that_stopped_while_catching_up_votes_only_once_it_caught_up() -> TestResult {
    runtime()?.block_on(async {
        let root = TestResources::new()?;
        let (mut members, addresses, last) = joined_group(&root).await?;
        let wiped = if leader(&members).await? == 3 { 2 } else { 3 };
        restart_paused(&root, &mut members, &addresses, (wiped, "lost")).await?;
        holds(&members, wiped, last).await?;
        let machine = restart_paused(&root, &mut members, &addresses, (wiped, "lost")).await?;
        let until = Instant::now() + Duration::from_secs(1);
        while Instant::now() < until {
            assert!(
                !answers_votes(&addresses, wiped).await?,
                "voted with part of the log"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        machine.resume();
        converge(&members, last).await?;
        assert!(answers_votes_within(&addresses, wiped, Duration::from_secs(5)).await?);
        restart_paused(&root, &mut members, &addresses, (wiped, "lost")).await?;
        assert!(
            answers_votes_within(&addresses, wiped, Duration::from_secs(2)).await?,
            "the mark was not kept"
        );
        Ok(())
    })
}
