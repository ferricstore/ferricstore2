//! Requests a member sends outside `OpenRaft`'s replication: what another
//! member holds (ADR-019), once, and a follower's newest snapshot, which a
//! leader that trims its log asks over a link it keeps (ADR-018 phase 3).

use std::io;
use std::time::Duration;

use openraft::type_config::alias::SnapshotOf;

use super::super::Config;
use super::super::wire::{Held, Recipient, Request, Response};
use super::{CONNECT_TIMEOUT, Link, SNAPSHOT_TIMEOUT};

impl Link {
    /// A link to `recipient` at `address`; it connects on its first
    /// request and keeps the connection while requests succeed.
    pub(crate) const fn to(address: String, recipient: Recipient) -> Self {
        Self {
            address: Some(address),
            recipient,
            stream: None,
        }
    }
}

/// Asks over `link` within `deadline`; a failed or late exchange drops the
/// link's connection.
async fn ask(link: &mut Link, request: &Request, deadline: Duration) -> io::Result<Response> {
    tokio::time::timeout(deadline, link.exchange(request))
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
    let mut link = Link::to(address.to_owned(), recipient);
    match ask(&mut link, &Request::Status, CONNECT_TIMEOUT).await? {
        Response::Status(held) => Ok(held),
        _ => Err(mismatched_answer()),
    }
}

/// The newest snapshot of the member `link` reaches, if its last index is
/// past `after` and at most `upto`.
///
/// # Errors
/// The member cannot be reached, refuses the request or answers something
/// else.
pub(crate) async fn newest(
    link: &mut Link,
    after: u64,
    upto: u64,
) -> io::Result<Option<SnapshotOf<Config, Vec<u8>>>> {
    let request = Request::Newest { after, upto };
    match ask(link, &request, SNAPSHOT_TIMEOUT).await? {
        Response::Newest(newest) => Ok(newest),
        _ => Err(mismatched_answer()),
    }
}
