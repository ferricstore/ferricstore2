//! One connection: a reader loop and a writer task.
//!
//! Every request gets the next order number on its lane. Replies arrive at
//! the writer in any order; it holds each one until the replies before it on
//! the same lane have gone, then writes everything ready in one call. A bad
//! frame header closes the connection, since the stream cannot be resynced.
//! Each request holds permits for itself and its body's bytes until its
//! reply is written; with none left, the reader waits (ADR-017). A frame
//! must finish within the frame deadline once it started (audit gap G66).

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;

use crate::frame::{FLAG_NO_REPLY, HEADER_LEN, Header};

use super::InFlight;

/// The most of a body read ahead of its bytes arriving.
const BODY_CHUNK: usize = 64 * 1024;
use super::dispatch::{self, Context, Negotiated, Next};

/// A request's share of its connection's in-flight limits, given back when
/// its reply is written or dropped.
pub(super) struct Permits {
    _request: OwnedSemaphorePermit,
    _bytes: OwnedSemaphorePermit,
}

/// A reply frame, or an empty frame for a request that wants no reply.
pub(super) struct Outgoing {
    lane: u32,
    order: u64,
    frame: Vec<u8>,
    permits: Permits,
}

/// Where one request's reply goes.
pub(super) struct ReplyTo {
    outgoing: UnboundedSender<Outgoing>,
    header: Header,
    order: u64,
    permits: Permits,
}

impl ReplyTo {
    /// Sends the reply frame; a reply too large for a frame becomes an error.
    pub(super) fn send(self, flags: u8, body: &[u8]) {
        let mut frame = Vec::new();
        if self.header.flags & FLAG_NO_REPLY == 0
            && self
                .header
                .encode_response(flags, body, &mut frame)
                .is_err()
        {
            frame.clear();
            let error = crate::response::error(
                crate::response::STATUS_ERROR,
                "ERR reply exceeds the frame limit",
            )
            .unwrap_or_default();
            if self.header.encode_response(0, &error, &mut frame).is_err() {
                frame.clear();
            }
        }
        // The writer is gone only when the connection closed.
        let _closed = self.outgoing.send(Outgoing {
            lane: self.header.lane,
            order: self.order,
            frame,
            permits: self.permits,
        });
    }
}

#[derive(Default)]
struct Lane {
    next: u64,
    held: BTreeMap<u64, (Vec<u8>, Permits)>,
}

async fn write_loop(mut writer: OwnedWriteHalf, mut replies: UnboundedReceiver<Outgoing>) {
    let mut lanes: HashMap<u32, Lane> = HashMap::new();
    let mut buffer = Vec::new();
    let mut written: Vec<Permits> = Vec::new();
    while let Some(first) = replies.recv().await {
        let mut ready = vec![first];
        while let Ok(more) = replies.try_recv() {
            ready.push(more);
        }
        for outgoing in ready {
            let lane = lanes.entry(outgoing.lane).or_default();
            lane.held
                .insert(outgoing.order, (outgoing.frame, outgoing.permits));
            while let Some((frame, permits)) = lane.held.remove(&lane.next) {
                buffer.extend_from_slice(&frame);
                written.push(permits);
                lane.next = lane.next.saturating_add(1);
            }
        }
        if !buffer.is_empty() {
            if writer.write_all(&buffer).await.is_err() {
                return;
            }
            buffer.clear();
        }
        // The replies left: their requests' permits go back to the reader.
        written.clear();
    }
}

/// The connection's in-flight limits, as permits.
struct Limits {
    requests: Arc<Semaphore>,
    bytes: Arc<Semaphore>,
    max_bytes: u32,
}

impl Limits {
    fn new(limit: InFlight) -> Self {
        let permits =
            |count: u32| Arc::new(Semaphore::new(usize::try_from(count.max(1)).unwrap_or(1)));
        Self {
            requests: permits(limit.requests),
            bytes: permits(limit.bytes),
            max_bytes: limit.bytes.max(1),
        }
    }

    /// Waits for room for one request with a body of `length` bytes; a body
    /// over the byte limit waits for all of it.
    async fn admit(&self, length: usize) -> Option<Permits> {
        let request = Arc::clone(&self.requests).acquire_owned().await.ok()?;
        let cost = u32::try_from(length)
            .unwrap_or(u32::MAX)
            .clamp(1, self.max_bytes);
        let bytes = Arc::clone(&self.bytes)
            .acquire_many_owned(cost)
            .await
            .ok()?;
        Some(Permits {
            _request: request,
            _bytes: bytes,
        })
    }
}

/// Runs one connection until the client closes it, sends `QUIT` or sends a
/// frame that is not FSNP.
pub(super) async fn run(stream: TcpStream, context: Context) {
    let (mut reader, writer) = stream.into_split();
    let (outgoing, replies) = unbounded_channel();
    let writer = tokio::spawn(write_loop(writer, replies));
    let mut orders: HashMap<u32, u64> = HashMap::new();
    let mut negotiated = Negotiated::default();
    let limits = Limits::new(context.info.in_flight);
    let frame_timeout = context.info.connections.frame_timeout;
    loop {
        let mut header = [0_u8; HEADER_LEN];
        // The first byte may take as long as the client likes; the rest of
        // the header must follow within the frame deadline.
        let Some((first, rest)) = header.split_first_mut() else {
            break;
        };
        if reader
            .read_exact(std::slice::from_mut(first))
            .await
            .is_err()
        {
            break;
        }
        if !matches!(
            timeout(frame_timeout, reader.read_exact(rest)).await,
            Ok(Ok(_))
        ) {
            break;
        }
        let Ok(header) = Header::parse_request(header) else {
            break;
        };
        let Ok(length) = usize::try_from(header.body_len) else {
            break;
        };
        let Some(permits) = limits.admit(length).await else {
            break;
        };
        // Once admitted, the body must arrive within the deadline; it is
        // read as it comes, not into a buffer of its declared length.
        let mut body = Vec::with_capacity(length.min(BODY_CHUNK));
        let mut taken = (&mut reader).take(u64::from(header.body_len));
        let read = timeout(frame_timeout, taken.read_to_end(&mut body)).await;
        if !matches!(read, Ok(Ok(read)) if read == length) {
            break;
        }
        let next = orders.entry(header.lane).or_insert(0);
        let order = *next;
        *next = next.saturating_add(1);
        let reply = ReplyTo {
            outgoing: outgoing.clone(),
            header,
            order,
            permits,
        };
        if dispatch::handle(&context, &mut negotiated, &header, &body, reply) == Next::Quit {
            break;
        }
    }
    context.closed.cancel();
    drop(outgoing);
    // The writer finishes once every pending reply has been sent or dropped;
    // waiting claims give theirs up once they see the cancellation.
    let _finished = writer.await;
}
