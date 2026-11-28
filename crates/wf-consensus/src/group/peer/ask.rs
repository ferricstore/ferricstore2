//! Requests a member sends outside `OpenRaft`'s replication: what another
//! member holds (ADR-019), once; a follower's newest snapshot, which a
//! leader that trims its log asks over a link it keeps (ADR-018 phase 3);
//! and the leader's read index, which a follower asks before it answers
//! reads (ADR-018).

use std::io;
use std::sync::Arc;
use std::time::Duration;

use openraft::Snapshot;
use openraft::type_config::alias::SnapshotOf;
use wf_ports::consensus::log::SnapshotStore;

use super::super::Config;
use super::super::snapshot::{self, Stored, aside};
use super::super::wire::{self, Held, Recipient, Request, Response};
use super::{CONNECT_TIMEOUT, Link, MAX_FRAME_BYTES, SNAPSHOT_TIMEOUT};

impl Link {
    /// A link to `recipient` at `address` in frames of at most `limit`
    /// bytes; it connects on its first request and keeps the connection
    /// while requests succeed.
    pub(crate) const fn to(address: String, recipient: Recipient, limit: u32) -> Self {
        Self {
            address: Some(address),
            recipient,
            stream: None,
            limit,
        }
    }
}

/// Asks over `link` within `deadline`; a failed or late exchange drops the
/// link's connection.
async fn ask(link: &mut Link, request: &Request, deadline: Duration) -> io::Result<Response> {
    let body = wire::encode_request(link.recipient, request)?;
    tokio::time::timeout(deadline, link.exchange(&body, None))
        .await
        .map_err(io::Error::other)?
}

fn mismatched_answer() -> io::Error {
    io::Error::other("mismatched peer response")
}

/// What `recipient`, at `address`, holds (ADR-019).
///
/// # Errors
/// The member cannot be reached, refuses the request or answers something
/// else.
pub(crate) async fn status(address: &str, recipient: Recipient) -> io::Result<Held> {
    // The answer is a few bytes; the frame limit only bounds what is read.
    let mut link = Link::to(address.to_owned(), recipient, MAX_FRAME_BYTES);
    match ask(&mut link, &Request::Status, CONNECT_TIMEOUT).await? {
        Response::Status(held) => Ok(held),
        _ => Err(mismatched_answer()),
    }
}

/// The index the member `link` reaches, which leads, has a follower apply
/// through before it answers a read.
///
/// # Errors
/// The member cannot be reached within `deadline`, does not lead, or
/// answers something else.
pub(crate) async fn read_index(link: &mut Link, deadline: Duration) -> io::Result<u64> {
    match ask(link, &Request::ReadIndex, deadline).await? {
        Response::ReadIndex(index) => Ok(index),
        Response::Failed(why) => Err(io::Error::other(why)),
        _ => Err(mismatched_answer()),
    }
}

/// The newest snapshot of the member `link` reaches, if its last index is
/// past `after` and at most `upto`, received into `store` (finished, not
/// adopted).
///
/// # Errors
/// The member cannot be reached, refuses the request or answers something
/// else, or the snapshot cannot be stored.
pub(crate) async fn newest(
    link: &mut Link,
    (after, upto): (u64, u64),
    store: &Arc<dyn SnapshotStore>,
) -> io::Result<Option<SnapshotOf<Config, Stored>>> {
    let request = Request::Newest { after, upto };
    let fetched = async {
        let (meta, length) = match ask(link, &request, SNAPSHOT_TIMEOUT).await? {
            Response::Newest(Some(announced)) => announced,
            Response::Newest(None) => return Ok(None),
            _ => return Err(mismatched_answer()),
        };
        let id = snapshot::id_of(&meta);
        let opening = Arc::clone(store);
        let sink = aside(move || opening.create(id).map_err(io::Error::other)).await?;
        let length = link.receive_into(sink, length).await?;
        Ok(Some(Snapshot {
            meta,
            snapshot: Stored::new(Arc::clone(store), id, length),
        }))
    };
    tokio::time::timeout(SNAPSHOT_TIMEOUT, fetched)
        .await
        .map_err(io::Error::other)?
}
