//! Lanes are independent; replies on one lane leave in request order, and
//! a session frame on a data lane answers in its place (rest-of-suite
//! audit, `native/lane_barrier_review_test.exs` and
//! `native/connection_decode_budget_test.exs`).

use super::typed::{Typed, bin, encode, map};
use super::{Client, TestResult, start};

const CREATE_MANY: u16 = 0x020F;
const GET: u16 = 0x0202;
const OPTIONS: u16 = 0x000B;
const PING: u16 = 0x0003;

/// An independent queue batch of `id`, which the scripted service answers
/// after 150 ms when `id` starts with `slow`.
fn batch(id: &str) -> TestResult<Vec<u8>> {
    encode(&map(&[
        ("type", bin("jobs")),
        ("state", bin("queued")),
        ("independent", Typed::Bool(true)),
        ("items", Typed::List(vec![Typed::List(vec![bin(id)])])),
        ("return", bin("ok_on_success")),
    ]))
}

#[test]
fn a_slow_command_holds_only_its_own_lane() -> TestResult {
    // Elixir: "a blocking command behind a busy lane does not stall
    // another lane", "heartbeat and flow-control frames answer while a
    // data lane is busy" and "a deferred blocking command keeps lane
    // order around it". The scripted queue service answers a batch
    // holding a `slow` id after 150 ms.
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let get = encode(&map(&[("id", bin("f1"))]))?;
    let held = client.send(CREATE_MANY, 1, 0, &batch("slow-1")?)?;
    let behind = client.send(GET, 1, 0, &get)?;
    let other = client.send(GET, 2, 0, &get)?;
    let ping = client.send(PING, 0, 0, &[])?;
    let mut order = Vec::new();
    for _ in 0..4 {
        order.push(client.receive()?.request_id);
    }
    let position = |id: u64| order.iter().position(|seen| *seen == id);
    assert!(position(other) < position(held), "{order:?}");
    assert!(position(ping) < position(held), "{order:?}");
    assert_eq!(order.get(2..), Some(&[held, behind][..]), "{order:?}");
    Ok(())
}

#[test]
fn a_session_frame_on_a_data_lane_answers_in_its_place() -> TestResult {
    // Elixir: "preserves FIFO when a data frame precedes a session frame
    // on one lane", "flushes a synchronous session response before
    // following data on one lane" and "orders multiple session and control
    // frames around one lane batch".
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let get = encode(&map(&[("id", bin("f1"))]))?;
    let sent = [
        client.send(CREATE_MANY, 3, 0, &batch("slow-2")?)?,
        client.send(OPTIONS, 3, 0, &[])?,
        client.send(GET, 3, 0, &get)?,
        client.send(PING, 3, 0, &[])?,
    ];
    let mut order = Vec::new();
    for _ in 0..4 {
        order.push(client.receive()?.request_id);
    }
    assert_eq!(order, sent);
    Ok(())
}
