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
    let log = EntryLog::open(&root.path().join(directory), GroupLogLimits::default())?;
    let fresh = log.bounds().1.is_none();
    let raft = Raft::new(
        id,
        Arc::new(config.validate()?),
        Peers::new(names(addresses)),
        LogStore::new(Arc::new(log)),
        machine.clone(),
    )
    .await?;
    let server = match serving {
        Serving::Voter => tokio::spawn(serve(listener, raft.clone())),
        Serving::Joining { .. } => tokio::spawn(serve_member(
            listener,
            raft.clone(),
            names(addresses),
            id,
            fresh,
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
