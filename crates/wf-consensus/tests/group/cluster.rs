//! Three in-process members, each with its own entry log: a leader is
//! elected and its writes commit on every member; committed writes survive
//! the leader's loss and a restarted member catches up; under message loss,
//! cut links and restarts, every acknowledged write is applied exactly once,
//! at the same index, on every member.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use openraft::Raft;
use openraft::error::{ClientWriteError, RaftError};
use wf_storage::log::{EntryLog, GroupLogLimits};
use wf_test_support::resources::TestResources;

use super::machine::Machine;
use super::network::Router;
use super::{TestResult, runtime};
use wf_consensus::group::{LogStore, Proposal};

const MEMBERS: [u64; 3] = [1, 2, 3];

struct Group {
    router: Router,
    root: TestResources,
    config: Arc<openraft::Config>,
    machines: BTreeMap<u64, Machine>,
}

impl Group {
    async fn new(seed: u64) -> TestResult<Self> {
        let config = openraft::Config {
            heartbeat_interval: 50,
            election_timeout_min: 200,
            election_timeout_max: 400,
            ..openraft::Config::default()
        };
        let mut group = Self {
            router: Router::new(seed),
            root: TestResources::new()?,
            config: Arc::new(config.validate()?),
            machines: BTreeMap::new(),
        };
        for id in MEMBERS {
            group.start(id).await?;
        }
        let first = group.router.member(1).ok_or("member 1 missing")?;
        first.initialize(BTreeSet::from(MEMBERS)).await?;
        Ok(group)
    }

    /// Starts member `id` on its log, with a state machine that applies
    /// again from the log.
    async fn start(&mut self, id: u64) -> TestResult {
        let log = EntryLog::open(
            &self.root.path().join(format!("m{id}")),
            GroupLogLimits::default(),
        )?;
        let machine = Machine::default();
        let member = Raft::new(
            id,
            Arc::clone(&self.config),
            self.router.from(id),
            LogStore::new(Arc::new(log)),
            machine.clone(),
        )
        .await?;
        self.router.add(id, member);
        self.machines.insert(id, machine);
        Ok(())
    }

    async fn stop(&mut self, id: u64) -> TestResult {
        if let Some(member) = self.router.remove(id) {
            member.shutdown().await?;
        }
        Ok(())
    }

    /// Whether member `id` runs and leads.
    async fn leads(&self, id: u64) -> bool {
        match self.router.member(id) {
            Some(member) => member.current_leader().await == Some(id),
            None => false,
        }
    }

    /// The member that leads now, if any.
    async fn current(&self) -> Option<u64> {
        for id in MEMBERS {
            if self.leads(id).await {
                return Some(id);
            }
        }
        None
    }

    /// The member that leads, once one does.
    async fn leader(&self, deadline: Duration) -> TestResult<u64> {
        let until = Instant::now() + deadline;
        while Instant::now() < until {
            if let Some(id) = self.current().await {
                return Ok(id);
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err("no leader".into())
    }

    /// Writes `value` through the leader; the index it committed at. Only a
    /// definite refusal (not the leader) is retried: after a timeout the
    /// outcome is unknown, so the write is not sent again. Without faults a
    /// write commits; only a loaded machine delays it, so it may take ten
    /// seconds.
    async fn write(&self, value: &[u8]) -> TestResult<u64> {
        self.write_within(value, Duration::from_secs(10), false)
            .await
    }

    /// Writes `value` as [`Group::write`] does, under loss: a write lost
    /// for a second stays unacknowledged.
    async fn write_under_loss(&self, value: &[u8]) -> TestResult<u64> {
        self.write_within(value, Duration::from_secs(1), false)
            .await
    }

    /// Writes the end marker once the faults are gone: it commits, but a
    /// replication stream may still be backing off from a lost message, the
    /// last restart may have taken the leader, and a loaded machine
    /// stretches both, so it waits longer than a write under loss. A marker
    /// whose entry a deposed leader discarded is sent again: a second marker
    /// changes nothing the run checks.
    async fn end(&self) -> TestResult<u64> {
        self.write_within(b"end", Duration::from_secs(10), true)
            .await
    }

    /// Writes `value` as [`Group::write`] does, waiting up to `wait` for
    /// the commit, and sending it again after a discarded entry when
    /// `again` says so.
    async fn write_within(&self, value: &[u8], wait: Duration, again: bool) -> TestResult<u64> {
        // Time to find a leader beside the commit's own wait.
        let until = Instant::now() + wait + Duration::from_secs(if again { 20 } else { 4 });
        while Instant::now() < until {
            let Ok(leader) = self.leader(Duration::from_millis(500)).await else {
                continue;
            };
            let Some(member) = self.router.member(leader) else {
                continue;
            };
            let proposal = Proposal(value.to_vec());
            match tokio::time::timeout(wait, member.client_write(proposal)).await {
                Ok(Ok(written)) => return Ok(written.log_id.index),
                Ok(Err(RaftError::APIError(ClientWriteError::ForwardToLeader(_)))) => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Ok(Err(RaftError::APIError(ClientWriteError::LogEntryDiscarded(_)))) if again => {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Ok(Err(error)) => return Err(error.into()),
                Err(_) => return Err("write outcome unknown".into()),
            }
        }
        Err("no leader took the write".into())
    }

    /// Whether member `id` is stopped or applied `index`.
    fn caught_up(&self, id: u64, index: u64) -> bool {
        let applied = self
            .machines
            .get(&id)
            .and_then(|machine| machine.image().applied);
        self.router.member(id).is_none() || applied.is_some_and(|applied| applied.index >= index)
    }

    /// Waits until every running member applied `index`.
    async fn converge(&self, index: u64) -> TestResult {
        let until = Instant::now() + Duration::from_secs(20);
        while Instant::now() < until {
            if MEMBERS.iter().all(|id| self.caught_up(*id, index)) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        Err(format!("members did not apply {index}").into())
    }

    /// Each member's applied proposals.
    fn applied(&self) -> Vec<Vec<(u64, Vec<u8>)>> {
        self.machines
            .values()
            .map(|machine| machine.image().proposals)
            .collect()
    }
}

#[test]
fn a_leader_s_writes_commit_on_every_member() -> TestResult {
    runtime()?.block_on(async {
        let group = Group::new(1).await?;
        let mut last = 0;
        for value in 0..50_u32 {
            last = group.write(&value.to_be_bytes()).await?;
        }
        group.converge(last).await?;
        let applied = group.applied();
        assert_eq!(applied.first().map(Vec::len), Some(50));
        assert!(applied.windows(2).all(|pair| pair.first() == pair.get(1)));
        Ok(())
    })
}

#[test]
fn committed_writes_survive_the_leader_s_loss_and_a_restart() -> TestResult {
    runtime()?.block_on(async {
        let mut group = Group::new(2).await?;
        for value in 0..20_u32 {
            group.write(&value.to_be_bytes()).await?;
        }
        let old = group.leader(Duration::from_secs(10)).await?;
        group.stop(old).await?;
        let mut last = 0;
        for value in 20..40_u32 {
            last = group.write(&value.to_be_bytes()).await?;
        }
        assert_ne!(group.leader(Duration::from_secs(10)).await?, old);
        group.start(old).await?;
        group.converge(last).await?;
        let applied = group.applied();
        let values: Vec<u32> = applied
            .first()
            .ok_or("no member")?
            .iter()
            .filter_map(|(_, bytes)| Some(u32::from_be_bytes(bytes.as_slice().try_into().ok()?)))
            .collect();
        assert_eq!(values, (0..40).collect::<Vec<_>>());
        assert!(applied.windows(2).all(|pair| pair.first() == pair.get(1)));
        Ok(())
    })
}

/// One seeded run: loss, cuts and restarts while writing.
async fn lossy_run(seed: u64) -> TestResult {
    let mut group = Group::new(seed).await?;
    group.router.faults().loss = 100;
    let mut acknowledged = Vec::new();
    for value in 0..60_u32 {
        if value % 15 == 14 {
            let id = group.router.faults().below(3).saturating_add(1);
            group.stop(id).await?;
            group.start(id).await?;
        }
        if value % 10 == 5 {
            let a = group.router.faults().below(3).saturating_add(1);
            let b = group.router.faults().below(3).saturating_add(1);
            let mut faults = group.router.faults();
            faults.cut.clear();
            if a != b {
                faults.cut.insert((a.min(b), a.max(b)));
            }
        }
        if let Ok(index) = group.write_under_loss(&value.to_be_bytes()).await {
            acknowledged.push((index, value));
        }
    }
    {
        let mut faults = group.router.faults();
        faults.loss = 0;
        faults.cut.clear();
    }
    let last = group.end().await?;
    group.converge(last).await?;
    let applied = group.applied();
    assert!(
        applied.windows(2).all(|pair| pair.first() == pair.get(1)),
        "seed {seed}: members differ"
    );
    let first = applied.first().ok_or("no member")?;
    for (index, value) in &acknowledged {
        let found: Vec<&(u64, Vec<u8>)> = first
            .iter()
            .filter(|(_, bytes)| bytes.as_slice() == value.to_be_bytes())
            .collect();
        assert_eq!(found.len(), 1, "seed {seed}: value {value} applied once");
        assert_eq!(found.first().map(|(at, _)| at), Some(index), "seed {seed}");
    }
    assert!(
        acknowledged.len() >= 30,
        "seed {seed}: {} acknowledged",
        acknowledged.len()
    );
    Ok(())
}

#[test]
fn loss_cuts_and_restarts_keep_every_acknowledged_write_once_everywhere() -> TestResult {
    let runtime = runtime()?;
    for seed in [7, 11, 13] {
        runtime.block_on(lossy_run(seed))?;
    }
    Ok(())
}
