//! An append larger than a peer frame (ADR-018): its entries go as
//! several appends, each at most the frame limit and each following the
//! one before.

use std::io;

use openraft::raft::AppendEntriesRequest;
use openraft::type_config::alias::LogIdOf;

use super::super::{Config, codec};
use super::{Recipient, invalid, put_bytes, put_log_id, put_u64, put_vote};

/// An append for `recipient` as requests of at most `limit` bytes each:
/// its entries in order, each request's previous log id the last entry of
/// the one before; one without entries is one request.
///
/// # Errors
/// An entry too large for the format or, with the request around it, for
/// `limit`.
pub(crate) fn encode_append(
    recipient: Recipient,
    append: &AppendEntriesRequest<Config>,
    limit: usize,
) -> io::Result<Vec<Vec<u8>>> {
    let head = |prev: Option<&LogIdOf<Config>>, count: u32| {
        let mut out = Vec::new();
        put_u64(&mut out, recipient.group);
        put_u64(&mut out, recipient.member);
        out.push(1);
        put_vote(&mut out, &append.vote);
        put_log_id(&mut out, prev);
        put_log_id(&mut out, append.leader_commit.as_ref());
        out.extend_from_slice(&count.to_be_bytes());
        out
    };
    // A previous log id, when present, adds its term, node and index.
    let fixed = head(None, 0).len().saturating_add(24);
    let mut requests = Vec::new();
    let mut prev = append.prev_log_id;
    let mut part: (Option<LogIdOf<Config>>, u32, Vec<u8>) = (prev, 0, Vec::new());
    for entry in &append.entries {
        let mut one = Vec::new();
        put_log_id(&mut one, Some(&entry.log_id));
        put_bytes(&mut one, &codec::encode(entry)?)?;
        if fixed.saturating_add(one.len()) > limit {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a log entry does not fit the peer frame limit",
            ));
        }
        // An empty part takes the entry: it fits alone, as checked above.
        if fixed.saturating_add(part.2.len()).saturating_add(one.len()) > limit {
            let (start, count, entries) = std::mem::replace(&mut part, (prev, 0, Vec::new()));
            let mut request = head(start.as_ref(), count);
            request.extend_from_slice(&entries);
            requests.push(request);
        }
        part.1 = part.1.checked_add(1).ok_or_else(invalid)?;
        part.2.extend_from_slice(&one);
        prev = Some(entry.log_id);
    }
    let (start, count, entries) = part;
    let mut request = head(start.as_ref(), count);
    request.extend_from_slice(&entries);
    requests.push(request);
    Ok(requests)
}
