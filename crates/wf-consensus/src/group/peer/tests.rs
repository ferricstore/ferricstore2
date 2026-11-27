use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::Arc;
use std::time::{Duration, Instant};

use openraft::type_config::async_runtime::watch::WatchReceiver;
use openraft::{Raft, SnapshotMeta, SnapshotPolicy, StoredMembership};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use wf_ports::consensus::log::{ConsensusLog, EntryId, SnapshotStore};
use wf_storage::log::{EntryLog, GroupLogLimits};
use wf_storage::snapshot::files::group::GroupSnapshots;

use super::super::ballot::Ballot;
use super::super::machine::Bridge;
use super::super::testing::{Machine, Root, TestResult, runtime};
use super::super::wire::{self, Recipient, Request, Response};
use super::super::{Config, LogStore, codec, snapshot};
use super::server::wanted;
use super::{Link, Peers, frame, newest, serving};

/// Frames of at most this many bytes, so a test snapshot spans several.
const LIMIT: u32 = 64 * 1024;

/// A leader asks for a snapshot newer than its own and no later than what
/// it applied: one at either bound's wrong side, or none, is declined.
#[test]
fn a_snapshot_answers_a_request_only_between_its_bounds() {
    assert!(wanted(Some(5), 4, 9));
    assert!(wanted(Some(9), 4, 9));
    assert!(!wanted(Some(4), 4, 9));
    assert!(!wanted(Some(10), 4, 9));
    assert!(!wanted(None, 0, 9));
}

/// A snapshot goes in the largest frames the limit allows, up to 1 MiB.
#[test]
fn a_snapshot_goes_in_frames_as_large_as_the_limit_allows() {
    assert_eq!(frame::chunk(LIMIT), 64 * 1024);
    assert_eq!(frame::chunk(u32::MAX), 1024 * 1024);
    assert_eq!(frame::chunk(0), 1);
}

/// Two ends of a loopback connection.
async fn pair() -> TestResult<(TcpStream, TcpStream)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let connecting = tokio::spawn(TcpStream::connect(address));
    let (accepted, _) = listener.accept().await?;
    Ok((connecting.await??, accepted))
}

/// Ten snapshot bytes sent as `frames`, then a frame `next`.
async fn sent(frames: &[&[u8]]) -> TestResult<TcpStream> {
    let (mut sending, receiving) = pair().await?;
    for body in frames {
        frame::write_frame(&mut sending, body).await?;
    }
    frame::write_frame(&mut sending, b"next").await?;
    // The receiving end reads what was written after this end closes.
    drop(sending);
    Ok(receiving)
}

/// The ways ten snapshot bytes can arrive wrongly: an empty frame before
/// them, or a frame that runs past them.
const MISFRAMED: [&[&[u8]]; 2] = [&[&[], &[1; 10]], &[&[1; 4], &[2; 7]]];

/// A snapshot refused before its bytes are read (one for another member)
/// has them dropped frame by frame, and the connection reads on in step;
/// an empty frame or one past the announced length is refused.
#[test]
fn skipped_snapshot_bytes_leave_the_connection_in_step() -> TestResult {
    runtime()?.block_on(async {
        let mut receiving = sent(&[&[1; 4], &[2; 6]]).await?;
        frame::skip(&mut receiving, 10, LIMIT).await?;
        assert_eq!(frame::read_frame(&mut receiving, LIMIT).await?, b"next");
        for frames in MISFRAMED {
            let mut receiving = sent(frames).await?;
            assert!(frame::skip(&mut receiving, 10, LIMIT).await.is_err());
        }
        Ok(())
    })
}

/// A store in a fresh directory under `root`.
fn store(root: &Root, name: &str) -> TestResult<Arc<dyn SnapshotStore>> {
    Ok(Arc::new(GroupSnapshots::open(&root.path().join(name))?))
}

/// Received snapshot bytes land in the store as sent, and the connection
/// reads on in step; an empty frame or one past the announced length is
/// refused and leaves nothing finished.
#[test]
fn received_snapshot_bytes_land_in_the_store_as_sent() -> TestResult {
    let root = Root::new();
    let store = store(&root, "received")?;
    runtime()?.block_on(async {
        let mut receiving = sent(&[&[1; 4], &[2; 6]]).await?;
        let length = frame::receive_into(&mut receiving, store.create(1)?, 10, LIMIT).await?;
        assert_eq!(length, 10);
        assert_eq!(frame::read_frame(&mut receiving, LIMIT).await?, b"next");
        let mut body = Vec::new();
        store.open(1)?.read_to_end(&mut body)?;
        assert_eq!(body, [[1; 4].as_slice(), &[2; 6]].concat());
        for (id, frames) in (2..).zip(MISFRAMED) {
            let mut receiving = sent(frames).await?;
            let sink = store.create(id)?;
            assert!(
                frame::receive_into(&mut receiving, sink, 10, LIMIT)
                    .await
                    .is_err()
            );
            assert!(store.open(id).is_err());
        }
        Ok(())
    })
}

/// A one-member group that leads, served over loopback.
struct Served {
    raft: Raft<Config, Bridge>,
    server: JoinHandle<()>,
    address: String,
    own: Recipient,
    store: Arc<dyn SnapshotStore>,
}

async fn served(root: &Root) -> TestResult<Served> {
    let log: Arc<dyn ConsensusLog> = Arc::new(EntryLog::open(
        &root.path().join("log"),
        GroupLogLimits::default(),
    )?);
    let store = store(root, "served")?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?.to_string();
    let peers = BTreeMap::from([(1, address.clone())]);
    let config = openraft::Config {
        snapshot_policy: SnapshotPolicy::Never,
        ..openraft::Config::default()
    }
    .validate()?;
    let bridge = Bridge::new(Arc::new(Machine), (1, 1), Arc::clone(&store));
    let raft = Raft::new(
        1,
        Arc::new(config),
        Peers::limited(peers.clone(), LIMIT),
        LogStore::new(log),
        bridge.clone(),
    )
    .await?;
    let _set = bridge.metrics.set(raft.metrics());
    raft.initialize(std::collections::BTreeSet::from([1]))
        .await?;
    let own = Recipient {
        group: wire::group_id(&peers),
        member: 1,
    };
    let ballot = Arc::new(Ballot::new(false));
    let serving = serving(
        listener,
        raft.clone(),
        (ballot, own),
        (LIMIT, Arc::clone(&store)),
    );
    Ok(Served {
        server: tokio::spawn(serving),
        raft,
        address,
        own,
        store,
    })
}

impl Served {
    /// Takes a snapshot once the member leads and applied an entry; its
    /// last index.
    async fn snapshot(&self) -> TestResult<u64> {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            let (leads, applied) = {
                let metrics = self.raft.metrics();
                let metrics = metrics.borrow_watched();
                (
                    metrics.current_leader == Some(1),
                    metrics.last_applied.map_or(0, |log_id| log_id.index),
                )
            };
            if leads && applied > 0 {
                break;
            }
            if Instant::now() > until {
                return Err("the member did not lead".into());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        self.raft.trigger().snapshot().await?;
        loop {
            let taken = self.raft.metrics().borrow_watched().snapshot;
            if let Some(log_id) = taken {
                return Ok(log_id.index);
            }
            if Instant::now() > until {
                return Err("the member took no snapshot".into());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn stop(self) -> TestResult {
        self.server.abort();
        self.raft.shutdown().await?;
        Ok(())
    }
}

/// A member answers a request for its newest snapshot with the snapshot,
/// its metadata and then its bytes, only when its last index lies past the
/// asker's own and at most what the asker applied; otherwise, and before
/// it has one, it answers that it has none.
#[test]
fn a_member_sends_its_newest_snapshot_only_between_the_bounds() -> TestResult {
    let root = Root::new();
    let fetched = store(&root, "fetched")?;
    runtime()?.block_on(async {
        let served = served(&root).await?;
        let mut link = Link::to(served.address.clone(), served.own, LIMIT);
        assert!(newest(&mut link, (0, 100), &fetched).await?.is_none());
        let index = served.snapshot().await?;
        let snapshot = newest(&mut link, (0, index), &fetched)
            .await?
            .ok_or("no snapshot")?;
        assert_eq!(
            snapshot.meta.last_log_id.map(|log_id| log_id.index),
            Some(index)
        );
        let mut source = snapshot.snapshot.open()?;
        assert_eq!(snapshot::read_head(&mut source)?, snapshot.meta);
        let mut rest = Vec::new();
        source.read_to_end(&mut rest)?;
        assert_eq!(rest, b"captured");
        assert!(newest(&mut link, (index, 100), &fetched).await?.is_none());
        assert!(
            newest(&mut link, (0, index.saturating_sub(1)), &fetched)
                .await?
                .is_none()
        );
        served.stop().await
    })
}

/// A snapshot sent to another member of the group is refused without being
/// stored, its bytes are read past, and the same connection answers the
/// next request.
#[test]
fn a_snapshot_for_another_member_is_refused_and_read_past() -> TestResult {
    let root = Root::new();
    let sending = store(&root, "sending")?;
    let mut sink = sending.create(7)?;
    sink.write_all(&vec![1; 200 * 1024])?;
    let length = sink.finish()?;
    runtime()?.block_on(async {
        let served = served(&root).await?;
        let other = Recipient {
            member: 2,
            ..served.own
        };
        let meta = SnapshotMeta {
            last_log_id: Some(codec::log_id(EntryId {
                term: 1,
                node: 1,
                index: 7,
            })),
            last_membership: StoredMembership::default(),
        };
        let vote = served.raft.metrics().borrow_watched().vote;
        let request = Request::Snapshot(vote, meta.clone(), length);
        let mut link = Link::to(served.address.clone(), other, LIMIT);
        let body = wire::encode_request(other, &request)?;
        let answer = link.exchange(&body, Some(sending.open(7)?)).await?;
        assert!(matches!(answer, Response::Failed(_)));
        assert!(served.store.open(snapshot::id_of(&meta)).is_err());
        let status = wire::encode_request(served.own, &Request::Status)?;
        let answer = link.exchange(&status, None).await?;
        assert!(matches!(answer, Response::Status(_)));
        served.stop().await
    })
}
