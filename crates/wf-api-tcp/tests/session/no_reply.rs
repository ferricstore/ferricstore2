//! `NO_REPLY` (flag `0x10`): the command runs and its reply, success or
//! refusal, is not sent; later replies on the lane keep their order.

use super::typed::{bin, encode, map, reply};
use super::{Client, RECORDED, TestResult, start};

const NO_REPLY: u8 = 0x10;
const CREATE: u16 = 0x0201;
const GET: u16 = 0x0202;
const OPTIONS: u16 = 0x000B;
const HELLO: u16 = 0x0001;
const PING: u16 = 0x0003;

fn create(id: &str) -> TestResult<Vec<u8>> {
    encode(&map(&[
        ("id", bin(id)),
        ("type", bin("t")),
        ("state", bin("a")),
    ]))
}

#[test]
fn no_reply_commands_run_without_answering() -> TestResult {
    // Elixir: "NO_REPLY lane commands execute without encoding a
    // response", "NO_REPLY suppresses a data command rejected by the
    // inflight gate" and "NO_REPLY control commands skip response
    // encoding".
    let running = start()?;
    let mut client = Client::connect(&running)?;
    client.send(CREATE, 1, NO_REPLY, &create("record-no-reply")?)?;
    client.send(CREATE, 1, NO_REPLY, &create("dup-no-reply")?)?;
    client.send(OPTIONS, 0, NO_REPLY, &[])?;
    client.send(HELLO, 0, NO_REPLY, &encode(&map(&[]))?)?;
    let get = client.send(GET, 1, 0, &encode(&map(&[("id", bin("f1"))]))?)?;
    let ping = client.send(PING, 0, 0, &[])?;
    let mut answered = vec![client.receive()?.request_id, client.receive()?.request_id];
    answered.sort_unstable();
    assert_eq!(answered, [get, ping]);
    let recorded = RECORDED.lock().map_err(|_| "poisoned")?.clone();
    assert!(
        recorded.contains(&b"record-no-reply".to_vec()),
        "{recorded:?}"
    );
    // Nothing else was queued for this connection.
    let last = client.call(PING, 0, 0, &[])?;
    assert_eq!(reply(&last.body)?, (0, bin("PONG")));
    Ok(())
}
