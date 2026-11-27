//! A state machine for the gate: it records applied proposals in memory and
//! snapshots them as bytes (the applied id, the membership and each
//! proposal), as the peer network carries snapshots.

use std::collections::BTreeSet;
use std::future::{Future, ready};
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures_util::TryStreamExt;
use futures_util::stream::Stream;
use openraft::impls::leader_id_adv::LeaderId;
use openraft::storage::{EntryResponder, RaftSnapshotBuilder, RaftStateMachine};
use openraft::type_config::alias::{
    EntryOf, LogIdOf, SnapshotMetaOf, SnapshotOf, StoredMembershipOf,
};
use openraft::{
    EntryPayload, LogId, Membership, OptionalSend, Snapshot, SnapshotMeta, StoredMembership,
};

use wf_consensus::group::{Config, Stored};
use wf_ports::consensus::log::{ConsensusLogError, SnapshotStore};

use super::memory::MemoryStore;

/// What a member applied.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Image {
    pub(super) applied: Option<LogIdOf<Config>>,
    pub(super) membership: StoredMembershipOf<Config>,
    /// Each applied proposal's index and bytes.
    pub(super) proposals: Vec<(u64, Vec<u8>)>,
}

/// The last snapshot, with its metadata.
type Current = Option<(SnapshotMetaOf<Config>, Stored)>;

fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn put_log_id(out: &mut Vec<u8>, log_id: Option<&LogIdOf<Config>>) {
    match log_id {
        Some(log_id) => {
            out.push(1);
            put_u64(out, log_id.leader_id.term);
            put_u64(out, log_id.leader_id.node_id);
            put_u64(out, log_id.index);
        }
        None => out.push(0),
    }
}

fn put_ids(out: &mut Vec<u8>, ids: &BTreeSet<u64>) {
    put_u64(out, u64::try_from(ids.len()).unwrap_or(u64::MAX));
    for id in ids {
        put_u64(out, *id);
    }
}

/// An image as snapshot bytes.
fn encode(image: &Image) -> Vec<u8> {
    let mut out = Vec::new();
    put_log_id(&mut out, image.applied.as_ref());
    put_log_id(&mut out, image.membership.log_id().as_ref());
    let configs = image.membership.membership().get_joint_config();
    put_u64(&mut out, u64::try_from(configs.len()).unwrap_or(u64::MAX));
    for config in configs {
        put_ids(&mut out, config);
    }
    let members: BTreeSet<u64> = image
        .membership
        .membership()
        .nodes()
        .map(|(id, ())| *id)
        .collect();
    put_ids(&mut out, &members);
    put_u64(
        &mut out,
        u64::try_from(image.proposals.len()).unwrap_or(u64::MAX),
    );
    for (index, bytes) in &image.proposals {
        put_u64(&mut out, *index);
        put_u64(&mut out, u64::try_from(bytes.len()).unwrap_or(u64::MAX));
        out.extend_from_slice(bytes);
    }
    out
}

struct Reader<'a>(&'a [u8]);

/// Bytes the test machine did not write.
struct Malformed;

impl Reader<'_> {
    fn u64(&mut self) -> Option<u64> {
        let (value, rest) = self.0.split_first_chunk::<8>()?;
        self.0 = rest;
        Some(u64::from_be_bytes(*value))
    }

    /// A presence byte, then the id when present.
    fn log_id(&mut self) -> Result<Option<LogIdOf<Config>>, Malformed> {
        let (&present, rest) = self.0.split_first().ok_or(Malformed)?;
        self.0 = rest;
        if present == 0 {
            return Ok(None);
        }
        let mut part = || self.u64().ok_or(Malformed);
        let (term, node_id, index) = (part()?, part()?, part()?);
        Ok(Some(LogId::new(LeaderId { term, node_id }, index)))
    }

    fn ids(&mut self) -> Option<BTreeSet<u64>> {
        (0..self.u64()?).map(|_| self.u64()).collect()
    }
}

/// An image from its snapshot bytes.
fn decode(bytes: &[u8]) -> Option<Image> {
    let mut reader = Reader(bytes);
    let applied = reader.log_id().ok()?;
    let membership_log_id = reader.log_id().ok()?;
    let configs = (0..reader.u64()?)
        .map(|_| reader.ids())
        .collect::<Option<Vec<_>>>()?;
    let members = reader.ids()?;
    let membership = Membership::new(configs, members).ok()?;
    let mut proposals = Vec::new();
    for _ in 0..reader.u64()? {
        let index = reader.u64()?;
        let length = usize::try_from(reader.u64()?).ok()?;
        let (proposal, rest) = reader.0.split_at_checked(length)?;
        reader.0 = rest;
        proposals.push((index, proposal.to_vec()));
    }
    Some(Image {
        applied,
        membership: StoredMembership::new(membership_log_id, membership),
        proposals,
    })
}

#[derive(Clone, Default)]
pub(super) struct Machine {
    state: Arc<Mutex<Image>>,
    snapshot: Arc<Mutex<Current>>,
    /// While set, applying waits.
    paused: Arc<AtomicBool>,
    /// Where its snapshots are kept.
    store: MemoryStore,
}

/// Applies one entry to `image`.
fn record(image: &mut Image, entry: EntryOf<Config>) {
    image.applied = Some(entry.log_id);
    match entry.payload {
        EntryPayload::Blank => {}
        EntryPayload::Normal(proposal) => image.proposals.push((entry.log_id.index, proposal.0)),
        EntryPayload::Membership(membership) => {
            image.membership = StoredMembership::new(Some(entry.log_id), membership);
        }
    }
}

impl Machine {
    pub(super) fn image(&self) -> Image {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Holds back applying until [`Machine::resume`].
    pub(super) fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub(super) fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    fn keep(&self, current: Current) {
        *self.snapshot.lock().unwrap_or_else(PoisonError::into_inner) = current;
    }

    /// Where its snapshots are kept; a member over TCP receives snapshots
    /// into it.
    pub(super) fn store(&self) -> Arc<dyn SnapshotStore> {
        Arc::new(self.store.clone())
    }

    /// Writes `bytes` as snapshot `id` and adopts it.
    fn save(&self, id: u64, bytes: &[u8]) -> io::Result<Stored> {
        let failed = |error: ConsensusLogError| io::Error::other(error.to_string());
        let mut sink = self.store.create(id).map_err(failed)?;
        sink.write_all(bytes)?;
        let length = sink.finish().map_err(failed)?;
        self.store.adopt(id).map_err(failed)?;
        Ok(Stored::new(self.store(), id, length))
    }
}

// The machine answers synchronously, so most methods return ready futures.
impl RaftSnapshotBuilder<Config> for Machine {
    type SnapshotData = Stored;

    fn build_snapshot(
        &mut self,
    ) -> impl Future<Output = Result<SnapshotOf<Config, Stored>, io::Error>> + Send {
        let image = self.image();
        let meta = SnapshotMeta {
            last_log_id: image.applied,
            last_membership: image.membership.clone(),
        };
        let id = image.applied.map_or(0, |log_id| log_id.index);
        let stored = match self.save(id, &encode(&image)) {
            Ok(stored) => stored,
            Err(error) => return ready(Err(error)),
        };
        self.keep(Some((meta.clone(), stored.clone())));
        ready(Ok(Snapshot {
            meta,
            snapshot: stored,
        }))
    }
}

impl RaftStateMachine<Config> for Machine {
    type SnapshotData = Stored;
    type SnapshotBuilder = Self;

    fn applied_state(
        &mut self,
    ) -> impl Future<
        Output = Result<(Option<LogIdOf<Config>>, StoredMembershipOf<Config>), io::Error>,
    > + Send {
        let image = self.image();
        ready(Ok((image.applied, image.membership)))
    }

    async fn apply<Strm>(&mut self, mut entries: Strm) -> Result<(), io::Error>
    where
        Strm: Stream<Item = Result<EntryResponder<Config>, io::Error>> + Unpin + OptionalSend,
    {
        while let Some((entry, responder)) = entries.try_next().await? {
            while self.paused.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            record(
                &mut self.state.lock().unwrap_or_else(PoisonError::into_inner),
                entry,
            );
            if let Some(responder) = responder {
                responder.send(());
            }
        }
        Ok(())
    }

    fn get_snapshot_builder(&mut self) -> impl Future<Output = Self::SnapshotBuilder> + Send {
        ready(self.clone())
    }

    fn install_snapshot(
        &mut self,
        meta: &SnapshotMetaOf<Config>,
        snapshot: Stored,
    ) -> impl Future<Output = Result<(), io::Error>> + Send {
        let mut bytes = Vec::new();
        let read = snapshot
            .open()
            .and_then(|mut source| source.read_to_end(&mut bytes));
        let Some(image) = read.ok().and_then(|_| decode(&bytes)) else {
            return ready(Err(io::Error::other("malformed test snapshot")));
        };
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = image;
        self.keep(Some((meta.clone(), snapshot)));
        ready(Ok(()))
    }

    fn get_current_snapshot(
        &mut self,
    ) -> impl Future<Output = Result<Option<SnapshotOf<Config, Stored>>, io::Error>> + Send {
        let current = self
            .snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        ready(Ok(
            current.map(|(meta, snapshot)| Snapshot { meta, snapshot })
        ))
    }
}
