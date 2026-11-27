//! Frames on a peer connection: a `u32` length and a body of at most the
//! member's frame limit. A snapshot's bytes follow its request (or a
//! fetch's answer) as frames of at most [`SNAPSHOT_CHUNK`] bytes, streamed
//! between the snapshot's file and the connection: a blocking thread reads
//! or writes the file a few chunks ahead of the connection, so neither end
//! holds more of a snapshot than those chunks.

use std::io::{self, Read, Write as _};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use wf_ports::consensus::log::{SnapshotSink, SnapshotSource};

/// The most snapshot bytes one frame carries, below a smaller frame limit.
const SNAPSHOT_CHUNK: usize = 1024 * 1024;
/// Chunks between a snapshot's file and the connection.
const IN_FLIGHT: usize = 4;

fn invalid(problem: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, problem.to_owned())
}

/// Reads one frame of at most `limit` bytes.
pub(super) async fn read_frame(stream: &mut TcpStream, limit: u32) -> io::Result<Vec<u8>> {
    let length = stream.read_u32().await?;
    if length > limit {
        return Err(invalid("peer frame too large"));
    }
    let mut body = vec![0; usize::try_from(length).map_err(io::Error::other)?];
    stream.read_exact(&mut body).await?;
    Ok(body)
}

/// Writes `body` as one frame.
pub(super) async fn write_frame(stream: &mut TcpStream, body: &[u8]) -> io::Result<()> {
    let length = u32::try_from(body.len()).map_err(io::Error::other)?;
    let mut frame = Vec::with_capacity(body.len().saturating_add(4));
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(body);
    stream.write_all(&frame).await
}

/// The snapshot bytes one frame carries under frame limit `limit`.
pub(super) fn chunk(limit: u32) -> usize {
    usize::try_from(limit)
        .map_or(SNAPSHOT_CHUNK, |limit| limit.min(SNAPSHOT_CHUNK))
        .max(1)
}

/// Fills `buffer` from `source` unless it ends first; returns the bytes read.
fn fill(source: &mut dyn Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while let Some(rest) = buffer.get_mut(filled..)
        && !rest.is_empty()
    {
        match source.read(rest)? {
            0 => break,
            read => filled = filled.saturating_add(read),
        }
    }
    Ok(filled)
}

/// Reads `source` into `chunks` of `size` bytes until it ends or fails, or
/// nothing waits for them; a failure is passed on last.
fn read_chunks(
    mut source: Box<dyn SnapshotSource>,
    size: usize,
    chunks: &mpsc::Sender<io::Result<Vec<u8>>>,
) {
    loop {
        let mut buffer = vec![0; size];
        let chunk = match fill(&mut source, &mut buffer) {
            Ok(0) => return,
            Ok(read) => {
                buffer.truncate(read);
                Ok(buffer)
            }
            Err(error) => Err(error),
        };
        let failed = chunk.is_err();
        if chunks.blocking_send(chunk).is_err() || failed {
            return;
        }
    }
}

/// Writes `source`'s bytes as frames that fit frame limit `limit`. A read
/// that fails (the file's check included) ends the stream short, so the
/// receiver refuses it.
pub(super) async fn send_source(
    stream: &mut TcpStream,
    source: Box<dyn SnapshotSource>,
    limit: u32,
) -> io::Result<()> {
    let size = chunk(limit);
    let (chunks, mut arriving) = mpsc::channel::<io::Result<Vec<u8>>>(IN_FLIGHT);
    let reader = tokio::task::spawn_blocking(move || read_chunks(source, size, &chunks));
    while let Some(chunk) = arriving.recv().await {
        write_frame(stream, &chunk?).await?;
    }
    reader.await.map_err(io::Error::other)
}

/// Reads the `length` snapshot bytes that follow on `stream`, sent as
/// frames of at most `limit` bytes, into `sink`, and finishes it; a stream
/// cut short drops it. Returns its length.
pub(super) async fn receive_into(
    stream: &mut TcpStream,
    sink: Box<dyn SnapshotSink>,
    length: u64,
    limit: u32,
) -> io::Result<u64> {
    let (chunks, mut arriving) = mpsc::channel::<Vec<u8>>(IN_FLIGHT);
    let writer = tokio::task::spawn_blocking(move || -> io::Result<u64> {
        let mut sink = sink;
        let mut written: u64 = 0;
        while let Some(chunk) = arriving.blocking_recv() {
            sink.write_all(&chunk)?;
            let size = u64::try_from(chunk.len()).map_err(io::Error::other)?;
            written = written.saturating_add(size);
        }
        if written != length {
            return Err(invalid("snapshot cut short"));
        }
        sink.finish().map_err(io::Error::other)
    });
    let received = forward(stream, (length, limit), &chunks).await;
    drop(chunks);
    let finished = writer.await.map_err(io::Error::other)?;
    received?;
    finished
}

/// Reads the `length` snapshot bytes that follow on `stream` and passes
/// them on as they arrive.
async fn forward(
    stream: &mut TcpStream,
    (length, limit): (u64, u32),
    chunks: &mpsc::Sender<Vec<u8>>,
) -> io::Result<()> {
    let mut left = length;
    while left > 0 {
        let part = read_frame(stream, limit).await?;
        let size = u64::try_from(part.len()).map_err(io::Error::other)?;
        if size == 0 || size > left {
            return Err(invalid("snapshot chunk does not fit the snapshot"));
        }
        left = left.saturating_sub(size);
        chunks
            .send(part)
            .await
            .map_err(|_| io::Error::other("snapshot writer stopped"))?;
    }
    Ok(())
}

/// Reads and drops the `length` snapshot bytes that follow on `stream`,
/// keeping the connection in step after a refused snapshot.
pub(super) async fn skip(stream: &mut TcpStream, length: u64, limit: u32) -> io::Result<()> {
    let mut left = length;
    while left > 0 {
        let part = read_frame(stream, limit).await?;
        let size = u64::try_from(part.len()).map_err(io::Error::other)?;
        if size == 0 || size > left {
            return Err(invalid("snapshot chunk does not fit the snapshot"));
        }
        left = left.saturating_sub(size);
    }
    Ok(())
}
