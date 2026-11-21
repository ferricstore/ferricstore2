//! Backpressure at the connection (ADR-017): a busy refusal is Elixir's
//! `busy` reply, which clients retry; and a connection with its in-flight
//! limit of requests or bytes reached reads nothing more until a reply is
//! written, so a client that floods the node or never reads is held back by
//! TCP instead of the node's memory.

use wf_api_tcp::frame::FLAG_CUSTOM_PAYLOAD;
use wf_api_tcp::session::InFlight;

use super::handshake::negotiate;
use super::typed::{Typed, bin, encode, map, reply};
use super::{Client, TestResult, serving, start};

const PING: u16 = 0x0003;
const CREATE: u16 = 0x0201;
const CLAIM_DUE: u16 = 0x0203;
const CREATE_MANY: u16 = 0x020F;

const SDK_CLAIM_DUE: &[u8] = include_bytes!("../fsnp/fixtures/sdk_claim_due_request.bin");

fn busy() -> Typed {
    map(&[
        ("code", bin("busy")),
        (
            "message",
            bin("BUSY flow shards are overloaded, retry later"),
        ),
        ("retry_after_ms", Typed::Int(100)),
        ("retryable", Typed::Bool(true)),
        ("safe_to_retry", Typed::Bool(true)),
    ])
}

fn create_many(id: &str) -> TestResult<Vec<u8>> {
    encode(&map(&[
        ("type", bin("jobs")),
        ("state", bin("queued")),
        ("independent", Typed::Bool(true)),
        ("items", Typed::List(vec![Typed::List(vec![bin(id)])])),
    ]))
}

#[test]
fn a_busy_refusal_is_the_retryable_busy_reply() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    negotiate(&mut client)?;
    let fast = client.call(CREATE_MANY, 1, 0, &create_many("busy1")?)?;
    assert_eq!(reply(&fast.body)?, (4, busy()), "queue create");
    let single = encode(&map(&[
        ("id", bin("busy2")),
        ("type", bin("jobs")),
        ("state", bin("queued")),
    ]))?;
    let generic = client.call(CREATE, 1, 0, &single)?;
    assert_eq!(reply(&generic.body)?, (4, busy()), "FLOW.CREATE");
    let typed_claim = encode(&map(&[("type", bin("jobs")), ("worker", bin("busy"))]))?;
    let claimed = client.call(CLAIM_DUE, 1, 0, &typed_claim)?;
    assert_eq!(reply(&claimed.body)?, (4, busy()), "typed claim");
    let mut compact = SDK_CLAIM_DUE.to_vec();
    let worker = wf_api_tcp::request::claim_due(FLAG_CUSTOM_PAYLOAD, SDK_CLAIM_DUE)?.worker;
    let at = compact
        .windows(worker.len())
        .position(|window| window == worker.as_slice())
        .ok_or("no worker in the fixture")?;
    // Same length, so the fixture's length prefix still holds.
    let replaced = b"busy".iter().chain(std::iter::repeat(&b'y'));
    for (byte, new) in compact.iter_mut().skip(at).take(worker.len()).zip(replaced) {
        *byte = *new;
    }
    let claimed = client.call(CLAIM_DUE, 1, FLAG_CUSTOM_PAYLOAD, &compact)?;
    assert_eq!(reply(&claimed.body)?, (4, busy()), "compact claim");
    Ok(())
}

#[test]
fn a_connection_at_its_in_flight_limit_reads_nothing_more_until_a_reply_leaves() -> TestResult {
    let limits = [
        InFlight {
            requests: 1,
            ..InFlight::default()
        },
        InFlight {
            bytes: 16,
            ..InFlight::default()
        },
    ];
    for limit in limits {
        let running = serving(false, limit)?;
        let mut client = Client::connect(&running)?;
        let slow = client.send(CREATE_MANY, 3, 0, &create_many("slow1")?)?;
        let ping = client.send(PING, 4, 0, &[])?;
        assert_eq!(client.receive()?.request_id, slow, "{limit:?}");
        assert_eq!(client.receive()?.request_id, ping, "{limit:?}");
    }
    // Without the limit, the other lane's reply leaves first.
    let running = start()?;
    let mut client = Client::connect(&running)?;
    client.send(CREATE_MANY, 3, 0, &create_many("slow1")?)?;
    let ping = client.send(PING, 4, 0, &[])?;
    assert_eq!(client.receive()?.request_id, ping);
    Ok(())
}
