//! A started frame must finish within the frame deadline (audit gap G66),
//! and the listener holds at most its connection limit (G67), as Elixir's
//! `native_frame_assembly_timeout_ms` and `maxclients` do.

use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use wf_api_tcp::session::Connections;

use super::typed::{Typed, bin, encode, map, reply};
use super::{Client, TestResult, serving_connections};

const PING: u16 = 0x0003;
const COMMAND_EXEC: u16 = 0x0100;

fn frame(opcode: u16, request: u64, body: &[u8]) -> TestResult<Vec<u8>> {
    let mut frame = b"FSNP".to_vec();
    frame.extend_from_slice(&[0x01, 0x00]);
    frame.extend_from_slice(&0_u32.to_be_bytes());
    frame.extend_from_slice(&opcode.to_be_bytes());
    frame.extend_from_slice(&request.to_be_bytes());
    frame.extend_from_slice(&u32::try_from(body.len())?.to_be_bytes());
    frame.extend_from_slice(body);
    Ok(frame)
}

fn guards(frame_timeout_ms: u64, max: u32) -> Connections {
    Connections {
        max,
        frame_timeout: Duration::from_millis(frame_timeout_ms),
    }
}

/// Whether the server closed `stream` (a read sees the end or a reset).
fn closed(stream: &mut TcpStream) -> TestResult<bool> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut byte = [0_u8; 1];
    match stream.read(&mut byte) {
        Ok(0) => Ok(true),
        Ok(_) => Ok(false),
        Err(error) if error.kind() == ErrorKind::ConnectionReset => Ok(true),
        Err(error) => Err(error.into()),
    }
}

#[test]
fn a_frame_waits_for_bytes_that_arrive_within_the_deadline() -> TestResult {
    // Elixir: "preserves an incomplete frame until the remaining socket
    // bytes arrive".
    let running = serving_connections(guards(2_000, 10))?;
    let mut client = Client::connect(&running)?;
    let ping = frame(PING, 42, &[])?;
    let (head, tail) = ping.split_at(ping.len() - 2);
    client.stream.write_all(head)?;
    std::thread::sleep(Duration::from_millis(50));
    client.stream.write_all(tail)?;
    let answer = client.receive()?;
    assert_eq!((answer.request_id, reply(&answer.body)?.0), (42, 0));
    Ok(())
}

#[test]
fn a_frame_unfinished_at_its_deadline_closes_the_connection() -> TestResult {
    // Elixir: "partial frame assembly has an absolute deadline", in the
    // header and in the body.
    let running = serving_connections(guards(100, 10))?;
    let body = vec![b'x'; 128];
    let whole = frame(PING, 47, &body)?;
    for cut in [10, 30] {
        let mut stream = TcpStream::connect(running.address)?;
        stream.write_all(whole.get(..cut).ok_or("short frame")?)?;
        std::thread::sleep(Duration::from_millis(300));
        assert!(closed(&mut stream)?, "cut at {cut}");
    }
    Ok(())
}

#[test]
fn an_idle_connection_is_not_a_late_frame() -> TestResult {
    // The deadline runs from a frame's first byte, not from the last reply.
    let running = serving_connections(guards(100, 10))?;
    let mut client = Client::connect(&running)?;
    std::thread::sleep(Duration::from_millis(300));
    let answer = client.call(PING, 0, 0, &[])?;
    assert_eq!(reply(&answer.body)?.0, 0);
    Ok(())
}

#[test]
fn a_large_frame_sent_in_pieces_is_read_whole() -> TestResult {
    // Elixir: "accepts a multi-megabyte frame sent as 64 KiB socket
    // chunks": a text PING echoes its 4 MiB message.
    let running = serving_connections(guards(5_000, 10))?;
    let mut client = Client::connect(&running)?;
    let message = "m".repeat(4 * 1024 * 1024);
    let body = encode(&map(&[
        ("command", bin("PING")),
        ("args", Typed::List(vec![bin(&message)])),
    ]))?;
    let whole = frame(COMMAND_EXEC, 7, &body)?;
    for piece in whole.chunks(64 * 1024) {
        client.stream.write_all(piece)?;
    }
    let answer = client.receive()?;
    assert_eq!(answer.request_id, 7);
    assert_eq!(reply(&answer.body)?, (0, bin(&message)));
    Ok(())
}

#[test]
fn the_listener_takes_no_connection_past_its_limit_until_one_closes() -> TestResult {
    // Elixir: "Ranch listener capacity matches the configured global
    // maxclients gate": past the limit a connection waits to be taken.
    let running = serving_connections(guards(5_000, 1))?;
    let mut first = Client::connect(&running)?;
    assert_eq!(reply(&first.call(PING, 0, 0, &[])?.body)?.0, 0);
    let mut second = TcpStream::connect(running.address)?;
    second.write_all(&frame(PING, 9, &[])?)?;
    second.set_read_timeout(Some(Duration::from_millis(300)))?;
    let mut header = [0_u8; 24];
    let waited = second.read_exact(&mut header);
    assert!(
        waited.as_ref().is_err_and(|error| matches!(
            error.kind(),
            ErrorKind::WouldBlock | ErrorKind::TimedOut
        )),
        "{waited:?}"
    );
    drop(first);
    second.set_read_timeout(Some(Duration::from_secs(5)))?;
    second.read_exact(&mut header)?;
    assert_eq!(header.get(12..20), Some(&9_u64.to_be_bytes()[..]));
    Ok(())
}
