//! Flow commands beyond the queue fast path (ADR-014): the typed-map body is
//! handed to the service as options and its term reply is encoded as a
//! typed value.

use super::typed::{Typed, bin, encode, map};
use super::{Client, TestResult, start};

const GET: u16 = 0x0202;
const CLAIM_DUE: u16 = 0x0203;
const RECLAIM: u16 = 0x0215;

fn ok(value: &Typed) -> TestResult<Vec<u8>> {
    let mut body = 0_u16.to_be_bytes().to_vec();
    body.extend(encode(value)?);
    Ok(body)
}

#[test]
fn flow_get_replies_with_the_service_term() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let request = encode(&map(&[("id", bin("f1")), ("partition_key", bin("p1"))]))?;
    let reply = client.call(GET, 1, 0, &request)?;
    assert_eq!(
        reply.body,
        ok(&map(&[("id", bin("f1")), ("partition_key", bin("p1"))]))?
    );
    let missing = client.call(GET, 1, 0, &encode(&map(&[("id", bin("none1"))]))?)?;
    assert_eq!(missing.body, ok(&Typed::Nil)?);
    Ok(())
}

#[test]
fn flow_get_options_are_read_by_the_application() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let request = encode(&map(&[("id", bin("f1")), ("bogus", Typed::Int(1))]))?;
    let reply = client.call(GET, 1, 0, &request)?;
    assert_eq!(
        reply.body,
        wf_api_tcp::response::error(
            wf_api_tcp::response::STATUS_ERROR,
            "ERR FLOW.GET option bogus is not supported by this server"
        )?
    );
    Ok(())
}

#[test]
fn a_typed_map_claim_replies_in_the_requested_shape() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let request = encode(&map(&[
        ("type", bin("jobs")),
        ("worker", bin("w")),
        ("return", bin("jobs_compact")),
    ]))?;
    let reply = client.call(CLAIM_DUE, 1, 0, &request)?;
    let job = Typed::List(vec![
        bin("f1"),
        bin("__flow_auto__:7"),
        bin("w:1:1"),
        Typed::Int(1),
    ]);
    assert_eq!(reply.body, ok(&Typed::List(vec![job]))?);
    Ok(())
}

#[test]
fn reclaim_claims_running_flows_as_records() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let request = encode(&map(&[("type", bin("jobs")), ("worker", bin("w"))]))?;
    let reply = client.call(RECLAIM, 1, 0, &request)?;
    let record = map(&[
        ("fencing_token", Typed::Int(1)),
        ("id", bin("f1")),
        ("lease_token", bin("w:1:1")),
        ("partition_key", bin("__flow_auto__:7")),
        ("state", bin("running")),
    ]);
    // The scripted service echoes the claimed states as the record's state.
    assert_eq!(reply.body, ok(&Typed::List(vec![record]))?);
    Ok(())
}

#[test]
fn every_single_flow_mutation_opcode_reaches_the_service() -> TestResult {
    let running = start()?;
    let mut client = Client::connect(&running)?;
    let guard = [
        ("lease_token", bin("w:1:1")),
        ("fencing_token", Typed::Int(1)),
    ];
    let states = [("from_state", bin("a")), ("to_state", bin("b"))];
    for (opcode, id, extra) in [
        (0x0204_u16, "complete", &[][..]),
        (0x0205, "transition", &states[..]),
        (0x0206, "retry", &[][..]),
        (0x0207, "fail", &[][..]),
        (0x0208, "cancel", &[][..]),
        (0x0209, "extend", &[][..]),
        (0x0222, "step", &states[..]),
    ] {
        let mut entries = vec![("id", bin(id))];
        entries.extend(guard.iter().cloned());
        entries.extend(extra.iter().cloned());
        let reply = client.call(opcode, 1, 0, &encode(&map(&entries))?)?;
        // The scripted service answers a mutation with its Flow id.
        assert_eq!(reply.body, ok(&bin(id))?, "opcode {opcode:#06x}");
    }
    let stale = encode(&map(&[
        ("id", bin("stale1")),
        ("lease_token", bin("w:1:1")),
        ("fencing_token", Typed::Int(1)),
    ]))?;
    let refused = client.call(0x0206, 1, 0, &stale)?;
    assert_eq!(
        refused.body,
        wf_api_tcp::response::error(wf_api_tcp::response::STATUS_ERROR, "ERR stale flow lease")?
    );
    Ok(())
}
