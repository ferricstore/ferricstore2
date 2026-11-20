//! Taking and loading node snapshots (ADR-013).
//!
//! A snapshot body is a version byte, the shard count, the base segment,
//! then each shard's image as a `u64` length and bytes. Taking one rotates
//! the log, asks every shard to capture (each logs its marker), checks that
//! every image decodes, writes the file, then removes the log segments and
//! snapshots before it. Snapshot ids only grow: the next id is taken before
//! any marker is logged, and on open it starts above every marker in the log,
//! so markers of a failed attempt are never mistaken for a written snapshot.

use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError, Weak};

use wf_storage::log::GroupLog;
use wf_storage::snapshot::files::SnapshotFiles;

use crate::supervision::shard::Request;
use crate::supervision::shard::image;
use crate::supervision::shard::state::ShardState;

use super::NodeError;
use super::replay::Cut;
use super::router::{Router, shard_index};

/// Version 6 images carry each Flow's fields beyond its record, its history
/// (with timeout extras), the shard's shared values and its Flow type
/// policies (ADR-014).
const VERSION: u8 = 6;

struct Progress {
    next: u64,
    base: u64,
}

/// The node's snapshot files and when the next snapshot is due.
pub(crate) struct Snapshots {
    files: SnapshotFiles,
    log: Arc<GroupLog>,
    every: u64,
    progress: Mutex<Progress>,
}

/// The shard count, base segment and shard images of a snapshot body.
fn parse(body: &[u8]) -> Option<(u16, u64, Vec<&[u8]>)> {
    let (&version, rest) = body.split_first()?;
    let (count, rest) = rest.split_at_checked(2)?;
    let (base, mut rest) = rest.split_at_checked(8)?;
    let count = u16::from_be_bytes(count.try_into().ok()?);
    let base = u64::from_be_bytes(base.try_into().ok()?);
    let mut images = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        let (length, after) = rest.split_at_checked(8)?;
        let length = usize::try_from(u64::from_be_bytes(length.try_into().ok()?)).ok()?;
        let (image, after) = after.split_at_checked(length)?;
        images.push(image);
        rest = after;
    }
    (version == VERSION && rest.is_empty() && count > 0).then_some((count, base, images))
}

/// Loads snapshot `id` into `states`, routing each record by partition.
pub(crate) fn load(
    states: &mut [ShardState],
    id: u64,
    body: &[u8],
    shards: u16,
) -> Result<Cut, NodeError> {
    let (count, base, images) = parse(body).ok_or(NodeError::SnapshotImage)?;
    for bytes in images {
        let decoded = image::decode(bytes).ok_or(NodeError::SnapshotImage)?;
        for value in decoded.shared {
            let state = shard_index(&value.partition, shards)
                .and_then(|index| states.get_mut(index))
                .ok_or(NodeError::SnapshotImage)?;
            state.store_shared(
                value.partition,
                value.reference,
                value.bytes,
                value.expires_at,
            );
        }
        // Policies belong to shard 0 whichever image held them.
        for (flow_type, generation, policy) in decoded.policies {
            states
                .first_mut()
                .ok_or(NodeError::SnapshotImage)?
                .restore_policy(&flow_type, generation, &policy)
                .map_err(|_| NodeError::SnapshotImage)?;
        }
        for restored in decoded.records {
            let state = shard_index(&restored.partition, shards)
                .and_then(|index| states.get_mut(index))
                .ok_or(NodeError::SnapshotImage)?;
            state.restore(restored);
        }
    }
    Ok(Cut::new(id, base, count))
}

impl Snapshots {
    /// `next` is the next snapshot id; `base` the segment the log starts at.
    pub(crate) fn new(
        files: SnapshotFiles,
        log: Arc<GroupLog>,
        every: u64,
        next: u64,
        base: u64,
    ) -> Self {
        Self {
            files,
            log,
            every,
            progress: Mutex::new(Progress { next, base }),
        }
    }

    /// Whether the log moved `every` segments past the last snapshot.
    fn due(&self) -> bool {
        let progress = self.progress.lock().unwrap_or_else(PoisonError::into_inner);
        self.every > 0 && self.log.current_segment() >= progress.base.saturating_add(self.every)
    }

    /// Takes a snapshot when one is due and the node still runs.
    pub(crate) fn check(&self, router: &Weak<Router>) {
        if !self.due() {
            return;
        }
        if let Some(router) = router.upgrade() {
            // A failed snapshot leaves the log as it was; the next check
            // tries again.
            let _attempt = self.take(&router);
        }
    }

    /// Takes a snapshot; returns its id.
    pub(crate) fn take(&self, router: &Router) -> Result<u64, NodeError> {
        let mut progress = self.progress.lock().unwrap_or_else(PoisonError::into_inner);
        let id = progress.next;
        progress.next = id.saturating_add(1);
        let base = self.log.rotate()?;
        let images = capture(router, id)?;
        let count = u16::try_from(images.len()).map_err(|_| NodeError::Capture)?;
        let mut header = vec![VERSION];
        header.extend_from_slice(&count.to_be_bytes());
        header.extend_from_slice(&base.to_be_bytes());
        let lengths: Vec<[u8; 8]> = images
            .iter()
            .map(|image| u64::try_from(image.len()).map(u64::to_be_bytes))
            .collect::<Result<_, _>>()
            .map_err(|_| NodeError::Capture)?;
        let mut parts: Vec<&[u8]> = vec![&header];
        for (length, image) in lengths.iter().zip(&images) {
            parts.push(length);
            parts.push(image);
        }
        self.files.write(id, &parts)?;
        progress.base = base;
        self.log.remove_before(base)?;
        self.files.remove_before(id)?;
        Ok(id)
    }

    /// After open: removes what a loaded snapshot made redundant, in case a
    /// crash came between writing it and removing them.
    pub(crate) fn tidy(&self, cut: &Cut) -> Result<(), NodeError> {
        self.log.remove_before(cut.base)?;
        self.files.remove_before(cut.id)?;
        Ok(())
    }
}

/// Every shard's image for snapshot `id`, in shard order, each checked to
/// decode.
fn capture(router: &Router, id: u64) -> Result<Vec<Vec<u8>>, NodeError> {
    let shards = router.shards.len();
    let (sender, receiver) = mpsc::channel();
    for index in 0..shards {
        let sender = sender.clone();
        router.send(
            index,
            Request::Capture {
                snapshot: id,
                done: Box::new(move |image| {
                    // The snapshot may have failed and stopped listening.
                    let _gone = sender.send((index, image));
                }),
            },
        );
    }
    drop(sender);
    let mut images = vec![Vec::new(); shards];
    for _ in 0..shards {
        let (index, image) = receiver.recv().map_err(|_| NodeError::Capture)?;
        let image = image.map_err(|_| NodeError::Capture)?;
        if image::decode(&image).is_none() {
            return Err(NodeError::SnapshotImage);
        }
        *images.get_mut(index).ok_or(NodeError::Capture)? = image;
    }
    Ok(images)
}
