//! One connection: a reader loop and a writer task.
//!
//! Every request gets the next order number on its lane. Replies arrive at
//! the writer in any order; it holds each one until the replies before it on
//! the same lane have gone, then writes everything ready in one call. A bad
//! frame header closes the connection, since the stream cannot be resynced.

use std::collections::{BTreeMap, HashMap};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::frame::{FLAG_NO_REPLY, HEADER_LEN, Header};

use super::dispatch::{self, Context, Negotiated, Next};

/// A reply frame, or an empty frame for a request that wants no reply.
pub(super) struct Outgoing {
    lane: u32,
    order: u64,
    frame: Vec<u8>,
}

/// Where one request's reply goes.
pub(super) struct ReplyTo {
    outgoing: UnboundedSender<Outgoing>,
    header: Header,
    order: u64,
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
        });
    }
}

#[derive(Default)]
struct Lane {
    next: u64,
    held: BTreeMap<u64, Vec<u8>>,
}

async fn write_loop(mut writer: OwnedWriteHalf, mut replies: UnboundedReceiver<Outgoing>) {
    let mut lanes: HashMap<u32, Lane> = HashMap::new();
    let mut buffer = Vec::new();
    while let Some(first) = replies.recv().await {
        let mut ready = vec![first];
        while let Ok(more) = replies.try_recv() {
            ready.push(more);
        }
        for outgoing in ready {
            let lane = lanes.entry(outgoing.lane).or_default();
            lane.held.insert(outgoing.order, outgoing.frame);
            while let Some(frame) = lane.held.remove(&lane.next) {
                buffer.extend_from_slice(&frame);
                lane.next = lane.next.saturating_add(1);
            }
        }
        if !buffer.is_empty() {
            if writer.write_all(&buffer).await.is_err() {
                return;
            }
            buffer.clear();
        }
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
    loop {
        let mut header = [0_u8; HEADER_LEN];
        if reader.read_exact(&mut header).await.is_err() {
            break;
        }
        let Ok(header) = Header::parse_request(header) else {
            break;
        };
        let Ok(length) = usize::try_from(header.body_len) else {
            break;
        };
        let mut body = vec![0_u8; length];
        if reader.read_exact(&mut body).await.is_err() {
            break;
        }
        let next = orders.entry(header.lane).or_insert(0);
        let order = *next;
        *next = next.saturating_add(1);
        let reply = ReplyTo {
            outgoing: outgoing.clone(),
            header,
            order,
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
