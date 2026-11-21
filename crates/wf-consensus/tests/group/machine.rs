//! A state machine for the gate: it records applied proposals in memory and
//! snapshots by copying them.

use std::future::{Future, ready};
use std::io;
use std::sync::{Arc, Mutex, PoisonError};

use futures_util::TryStreamExt;
use futures_util::stream::Stream;
use openraft::storage::{EntryResponder, RaftSnapshotBuilder, RaftStateMachine};
use openraft::type_config::alias::{
    EntryOf, LogIdOf, SnapshotMetaOf, SnapshotOf, StoredMembershipOf,
};
use openraft::{EntryPayload, OptionalSend, Snapshot, SnapshotMeta, StoredMembership};

use wf_consensus::group::Config;

/// What a member applied.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Image {
    pub(super) applied: Option<LogIdOf<Config>>,
    pub(super) membership: StoredMembershipOf<Config>,
    /// Each applied proposal's index and bytes.
    pub(super) proposals: Vec<(u64, Vec<u8>)>,
}

/// The last snapshot, with its metadata.
type Current = Option<(SnapshotMetaOf<Config>, Image)>;

#[derive(Clone, Default)]
pub(super) struct Machine {
    state: Arc<Mutex<Image>>,
    snapshot: Arc<Mutex<Current>>,
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

    fn keep(&self, current: Current) {
        *self.snapshot.lock().unwrap_or_else(PoisonError::into_inner) = current;
    }
}

// The machine answers synchronously, so most methods return ready futures.
impl RaftSnapshotBuilder<Config> for Machine {
    type SnapshotData = Image;

    fn build_snapshot(
        &mut self,
    ) -> impl Future<Output = Result<SnapshotOf<Config, Image>, io::Error>> + Send {
        let image = self.image();
        let meta = SnapshotMeta {
            last_log_id: image.applied,
            last_membership: image.membership.clone(),
        };
        self.keep(Some((meta.clone(), image.clone())));
        ready(Ok(Snapshot {
            meta,
            snapshot: image,
        }))
    }
}

impl RaftStateMachine<Config> for Machine {
    type SnapshotData = Image;
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
        snapshot: Image,
    ) -> impl Future<Output = Result<(), io::Error>> + Send {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = snapshot.clone();
        self.keep(Some((meta.clone(), snapshot)));
        ready(Ok(()))
    }

    fn get_current_snapshot(
        &mut self,
    ) -> impl Future<Output = Result<Option<SnapshotOf<Config, Image>>, io::Error>> + Send {
        let current = self
            .snapshot
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        ready(Ok(current.map(|(meta, image)| Snapshot {
            meta,
            snapshot: image,
        })))
    }
}
