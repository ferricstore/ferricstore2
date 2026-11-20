//! The SDK's queue frames through the listener, with Elixir-shaped replies.

use wf_api_tcp::frame::FLAG_CUSTOM_PAYLOAD;
use wf_api_tcp::request::ClaimShape;
use wf_api_tcp::response::{self, ClaimedJob, ItemOutcome, STATUS_ERROR};

use super::handshake::negotiate;
use super::typed::{Typed, bin, encode, map};
use super::{Client, TestResult, start};

const CREATE: u16 = 0x0201;
const CLAIM_DUE: u16 = 0x0203;
const COMPLETE: u16 = 0x0204;
const CREATE_MANY: u16 = 0x020F;
const COMPLETE_MANY: u16 = 0x0210;

const SDK_CREATE_MANY: &[u8] = include_bytes!("../fsnp/fixtures/sdk_create_many_request.bin");
const SDK_CLAIM_DUE: &[u8] = include_bytes!("../fsnp/fixtures/sdk_claim_due_request.bin");
const SDK_COMPLETE_MANY: &[u8] = include_bytes!("../fsnp/fixtures/sdk_complete_many_request.bin");

fn job(worker: &[u8]) -> (Vec<u8>, Vec<u8>) {
    (
        b"f1".to_vec(),
        worker.iter().chain(b":1:1").copied().collect(),
    )
}

fn create_many(ids: &[&str], ok_on_success: bool) -> TestResult<Vec<u8>> {
    let mut entries = vec![
        ("type", bin("jobs")),
        ("state", bin("queued")),
        ("independent", Typed::Bool(true)),
        (
            "items",
            Typed::List(ids.iter().map(|id| Typed::List(vec![bin(id)])).collect()),
        ),
    ];
    if ok_on_success {
        entries.push(("return", bin("ok_on_success")));
    }
    encode(&map(&entries))
}

#[test]
fn the_sdk_queue_frames_get_elixir_shaped_compact_replies() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    negotiate(&mut client)?;
    let created = client.call(CREATE_MANY, 1, 0, SDK_CREATE_MANY)?;
    assert_eq!(
        created.body,
        response::ok_text(b"OK")?,
        "ok_on_success batch"
    );
    let claimed = client.call(CLAIM_DUE, 1, FLAG_CUSTOM_PAYLOAD, SDK_CLAIM_DUE)?;
    assert_eq!(claimed.flags & FLAG_CUSTOM_PAYLOAD, FLAG_CUSTOM_PAYLOAD);
    let worker = wf_api_tcp::request::claim_due(FLAG_CUSTOM_PAYLOAD, SDK_CLAIM_DUE)?.worker;
    let (id, token) = job(&worker);
    let expected = ClaimedJob {
        id: &id,
        partition: b"__flow_auto__:7",
        lease_token: &token,
        fencing_token: 1,
        run_state: b"queued",
    };
    assert_eq!(
        claimed.body,
        response::compact_claim_jobs(&[expected], ClaimShape::JobsAttributes)?
    );
    let completed = client.call(COMPLETE_MANY, 1, FLAG_CUSTOM_PAYLOAD, SDK_COMPLETE_MANY)?;
    assert_eq!(
        completed.body,
        response::ok_text(b"OK")?,
        "non-independent batch"
    );
    let listed = client.call(CREATE_MANY, 1, 0, &create_many(&["a", "b"], false)?)?;
    assert_eq!(listed.flags & FLAG_CUSTOM_PAYLOAD, FLAG_CUSTOM_PAYLOAD);
    assert_eq!(listed.body, response::compact_ok_list(2)?);
    Ok(())
}

#[test]
fn without_negotiation_replies_are_typed() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let claimed = client.call(CLAIM_DUE, 1, FLAG_CUSTOM_PAYLOAD, SDK_CLAIM_DUE)?;
    assert_eq!(claimed.flags & FLAG_CUSTOM_PAYLOAD, 0);
    let worker = wf_api_tcp::request::claim_due(FLAG_CUSTOM_PAYLOAD, SDK_CLAIM_DUE)?.worker;
    let (id, token) = job(&worker);
    let expected = ClaimedJob {
        id: &id,
        partition: b"__flow_auto__:7",
        lease_token: &token,
        fencing_token: 1,
        run_state: b"queued",
    };
    assert_eq!(
        claimed.body,
        response::typed_claim_jobs(&[expected], ClaimShape::JobsAttributes)?
    );
    let listed = client.call(CREATE_MANY, 1, 0, &create_many(&["a", "b"], false)?)?;
    let ok = Typed::List(vec![bin("OK"), bin("OK")]);
    assert_eq!(super::typed::reply(&listed.body)?, (0, ok));
    Ok(())
}

#[test]
fn failures_are_reported_per_item_or_as_the_error_map() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    negotiate(&mut client)?;
    let partial = client.call(CREATE_MANY, 1, 0, &create_many(&["dup1", "a"], true)?)?;
    let expected = [
        ItemOutcome::Error("ERR flow already exists"),
        ItemOutcome::Ok,
    ];
    assert_eq!(partial.body, response::item_outcomes(&expected)?);
    let single = encode(&map(&[
        ("id", bin("dup2")),
        ("type", bin("jobs")),
        ("state", bin("queued")),
    ]))?;
    let refused = client.call(CREATE, 1, 0, &single)?;
    assert_eq!(
        refused.body,
        response::error(STATUS_ERROR, "ERR flow already exists")?
    );
    let stale = encode(&map(&[
        ("id", bin("stale1")),
        ("lease_token", bin("w:1:1")),
        ("fencing_token", Typed::Int(1)),
    ]))?;
    let refused = client.call(COMPLETE, 1, 0, &stale)?;
    assert_eq!(
        refused.body,
        response::error(STATUS_ERROR, "ERR stale flow lease")?
    );
    Ok(())
}

#[test]
fn replies_on_one_lane_keep_request_order() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let slow = client.send(CREATE_MANY, 3, 0, &create_many(&["slow1"], true)?)?;
    let fast = client.send(CREATE_MANY, 3, 0, &create_many(&["fast1"], true)?)?;
    assert_eq!(client.receive()?.request_id, slow);
    assert_eq!(client.receive()?.request_id, fast);
    Ok(())
}

#[test]
fn unsupported_commands_and_flags_are_refused_explicitly() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let (status, value) =
        super::typed::reply(&client.call(0x0101, 1, 0, &encode(&map(&[]))?)?.body)?;
    assert_eq!(status, STATUS_ERROR);
    let message = super::typed::at(&value, &["message"])?;
    assert!(
        matches!(message, Typed::Bin(text) if String::from_utf8_lossy(text).contains("not supported")),
        "{message:?}"
    );
    let compressed = client.call(CREATE_MANY, 1, 0x08, &create_many(&["a"], true)?)?;
    assert_eq!(super::typed::reply(&compressed.body)?.0, STATUS_ERROR);
    Ok(())
}
