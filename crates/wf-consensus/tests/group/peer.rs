//! Three members over loopback TCP (`group::serve` and `group::Peers`): they
//! elect a leader, replicate its writes, and after the leader stops the other
//! two elect a new one and keep committing.

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use openraft::Raft;
use openraft::error::{ClientWriteError, RaftError};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use wf_consensus::group::{Config, LogStore, Peers, Proposal, serve};
use wf_storage::log::{EntryLog, GroupLogLimits};
use wf_test_support::resources::TestResources;

use super::machine::Machine;
use super::{TestResult, runtime};

struct Member {
    raft: Raft<Config, Machine>,
    machine: Machine,
    server: JoinHandle<()>,
}

async fn start(
    root: &TestResources,
    id: u64,
    listener: TcpListener,
    addresses: &BTreeMap<u64, SocketAddr>,
) -> TestResult<Member> {
    let config = openraft::Config {
        heartbeat_interval: 50,
        election_timeout_min: 300,
        election_timeout_max: 600,
        ..openraft::Config::default()
    };
    let log = EntryLog::open(
        &root.path().join(format!("m{id}")),
        GroupLogLimits::default(),
    )?;
    let machine = Machine::default();
    let raft = Raft::new(
        id,
        Arc::new(config.validate()?),
        Peers::new(
            addresses
                .iter()
                .map(|(id, address)| (*id, address.to_string()))
                .collect(),
        ),
        LogStore::new(Arc::new(log)),
        machine.clone(),
    )
    .await?;
    let server = tokio::spawn(serve(listener, raft.clone()));
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
